use cloudtun_proxy::{
  FailoverConfig, ProxyConnector, RoutingMode,
  subscription::{SubscriptionNode, ping_nodes},
};
use serde::Serialize;
use std::{
  future::Future,
  sync::{Arc, Mutex},
  time::Duration,
};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VpnStatus {
  pub running: bool,
  pub auto_switch: bool,
  pub source: String,
  pub node_id: Option<String>,
  pub name: String,
  pub mode: RoutingMode,
  pub system_proxy: bool,
  pub error: Option<String>,
  pub switching: bool,
  pub event_id: u64,
  pub event_kind: Option<String>,
  pub event_message: Option<String>,
}
struct Policy {
  config: FailoverConfig,
  revision: u64,
  failures: u8,
  outage_reported: bool,
  status: VpnStatus,
}
pub struct Failover {
  connector: ProxyConnector,
  policy: Mutex<Policy>,
}
impl Failover {
  pub fn new(connector: ProxyConnector, config: FailoverConfig) -> Arc<Self> {
    let node = connector.current_node();
    let name = node
      .as_ref()
      .map(|node| node.name.clone())
      .unwrap_or_else(|| {
        connector
          .cloud_endpoint()
          .map(|s| s.host.clone())
          .unwrap_or_default()
      });
    let status = VpnStatus {
      running: true,
      auto_switch: config.enabled && node.is_some(),
      source: if node.is_some() {
        "subscription"
      } else {
        "cloud"
      }
      .into(),
      node_id: node.map(|node| node.id),
      name,
      mode: connector.routing_mode(),
      system_proxy: false,
      error: None,
      switching: false,
      event_id: 0,
      event_kind: None,
      event_message: None,
    };
    Arc::new(Self {
      connector,
      policy: Mutex::new(Policy {
        config,
        revision: 0,
        failures: 0,
        outage_reported: false,
        status,
      }),
    })
  }
  pub fn cloud_lost(&self) {
    let mut policy = self.policy.lock().unwrap_or_else(|e| e.into_inner());
    if policy.status.source != "cloud" {
      return;
    }
    policy.status.running = false;
    policy.status.auto_switch = false;
    Self::event(
      &mut policy,
      "cloudLost",
      cloudtun_proxy::cloud_health::INSTANCE_LOST_MESSAGE.into(),
    );
  }
  pub fn snapshot(&self) -> VpnStatus {
    let mut status = self
      .policy
      .lock()
      .unwrap_or_else(|e| e.into_inner())
      .status
      .clone();
    status.mode = self.connector.routing_mode();
    status
  }
  pub fn update(&self, config: FailoverConfig) {
    let mut policy = self.policy.lock().unwrap_or_else(|e| e.into_inner());
    if policy.config == config {
      return;
    }
    policy.status.auto_switch = config.enabled && policy.status.source == "subscription";
    policy.config = config;
    policy.revision += 1;
    policy.failures = 0;
    policy.outage_reported = false;
    policy.status.switching = false;
    policy.status.error = None;
  }
  fn event(policy: &mut Policy, kind: &str, message: String) {
    policy.status.event_id += 1;
    policy.status.event_kind = Some(kind.into());
    policy.status.event_message = Some(message);
  }
  /// The native Service-owned runtime drives this loop, independently of the WebView.
  pub async fn run(self: Arc<Self>, cancel: CancellationToken) {
    let mut ticks = tokio::time::interval(Duration::from_secs(15));
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
      tokio::select! { biased; _ = cancel.cancelled() => break, _ = ticks.tick() => {} }
      tokio::select! { biased; _ = cancel.cancelled() => break, _ = self.tick(check, rank) => {} }
    }
  }
  async fn tick<C, CF, R, RF>(&self, check: C, rank: R)
  where
    C: Fn(SubscriptionNode) -> CF,
    CF: Future<Output = bool>,
    R: Fn(Vec<SubscriptionNode>) -> RF,
    RF: Future<Output = Vec<SubscriptionNode>>,
  {
    let Some(current) = self.connector.current_node() else {
      return;
    };
    let revision = {
      let policy = self.policy.lock().unwrap_or_else(|e| e.into_inner());
      if !policy.config.enabled || !policy.status.running {
        return;
      }
      policy.revision
    };
    let healthy = check(current.clone()).await;
    let candidates = {
      let mut policy = self.policy.lock().unwrap_or_else(|e| e.into_inner());
      if policy.revision != revision || !policy.config.enabled {
        return;
      }
      if healthy {
        policy.failures = 0;
        policy.status.switching = false;
        policy.status.error = None;
        if policy.outage_reported {
          policy.outage_reported = false;
          Self::event(
            &mut policy,
            "recovered",
            format!("节点连接已恢复：{}", current.name),
          );
        }
        return;
      }
      policy.failures += 1;
      if policy.failures < 3 {
        return;
      }
      policy.failures = 0;
      policy.status.switching = true;
      policy.status.error = Some("当前节点连接异常，正在检测候选节点".into());
      policy
        .config
        .candidates
        .iter()
        .filter(|node| node.id != current.id && node.unsupported_reason.is_none())
        .cloned()
        .collect::<Vec<_>>()
    };
    let candidates = rank(candidates).await;
    for candidate in candidates {
      {
        let policy = self.policy.lock().unwrap_or_else(|e| e.into_inner());
        if policy.revision != revision || !policy.config.enabled {
          return;
        }
      }
      if !check(candidate.clone()).await {
        continue;
      }
      let mut policy = self.policy.lock().unwrap_or_else(|e| e.into_inner());
      if policy.revision != revision || !policy.config.enabled {
        return;
      }
      if self.connector.switch_node(candidate.clone()).is_err() {
        continue;
      }
      policy.status.node_id = Some(candidate.id.clone());
      policy.status.name = candidate.name.clone();
      policy.status.error = None;
      policy.status.switching = false;
      policy.outage_reported = false;
      Self::event(
        &mut policy,
        "switched",
        format!("已自动切换至 {}", candidate.name),
      );
      log::info!("VPN auto-switch: {} -> {}", current.name, candidate.name);
      return;
    }
    let mut policy = self.policy.lock().unwrap_or_else(|e| e.into_inner());
    if policy.revision != revision || !policy.config.enabled {
      return;
    }
    policy.status.switching = false;
    policy.status.running = false;
    policy.status.auto_switch = false;
    policy.status.error = Some("当前无可用节点，请手动重试".into());
    if !policy.outage_reported {
      policy.outage_reported = true;
      Self::event(
        &mut policy,
        "unavailable",
        "当前无可用订阅节点，VPN 已断开".into(),
      );
    }
  }
}
async fn check(node: SubscriptionNode) -> bool {
  cloudtun_proxy::check_node(&node).await.is_ok()
}
async fn rank(nodes: Vec<SubscriptionNode>) -> Vec<SubscriptionNode> {
  let mut scores = ping_nodes(&nodes).await;
  scores.sort_by_key(|score| score.latency_ms.unwrap_or(u64::MAX));
  scores
    .into_iter()
    .filter(|score| score.latency_ms.is_some_and(|ms| ms < 5000))
    .filter_map(|score| nodes.iter().find(|node| node.id == score.id).cloned())
    .collect()
}

#[cfg(test)]
mod tests {
  use super::*;
  #[test]
  fn cloud_loss_retains_a_service_event_until_service_teardown() {
    let connector = cloudtun_proxy::ProxyConnector::cloud(
      cloudtun_common::tls::CloudEndpoint::new(
        "192.0.2.1".into(),
        24816,
        "ins-test".into(),
        "01".repeat(32),
        std::env::temp_dir().join("cloudtun-test-trust"),
      )
      .unwrap(),
    );
    let failover = Failover::new(connector, FailoverConfig::default());
    failover.cloud_lost();
    let status = failover.snapshot();
    assert!(!status.running);
    assert!(!status.auto_switch);
    assert_eq!(status.event_kind.as_deref(), Some("cloudLost"));
    assert_eq!(
      status.event_message.as_deref(),
      Some(cloudtun_proxy::cloud_health::INSTANCE_LOST_MESSAGE)
    );
  }
  fn node(id: &str) -> SubscriptionNode {
    cloudtun_proxy::subscription::parse_subscription(&format!("vmess://{}", base64_node(id)))
      .unwrap()
      .remove(0)
  }
  fn base64_node(id: &str) -> String {
    // Use real subscription parsing to retain production validation.
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(format!(r#"{{"v":"2","ps":"{id}","add":"example.com","port":"443","id":"11111111-1111-4111-8111-111111111111","aid":"0","net":"tcp","tls":""}}"#))
  }
  fn manager() -> Arc<Failover> {
    let a = node("A");
    let b = node("B");
    let connector = cloudtun_proxy::VpnConfig {
      cloud_monitor: None,
      outbound: serde_json::from_value(serde_json::json!({"type":"subscription","node":a}))
        .unwrap(),
      mode: RoutingMode::Global,
      rules: None,
      failover: FailoverConfig::default(),
    }
    .connector()
    .unwrap();
    Failover::new(
      connector,
      FailoverConfig {
        enabled: true,
        candidates: vec![b],
      },
    )
  }
  #[test]
  fn cloud_notification_identifies_host_and_never_enables_subscription_switching() {
    let connector = ProxyConnector::cloud(
      cloudtun_common::tls::CloudEndpoint::new(
        "192.0.2.20".into(),
        24816,
        "test".into(),
        "a".repeat(64),
        "unused".into(),
      )
      .unwrap(),
    );
    let manager = Failover::new(
      connector,
      FailoverConfig {
        enabled: true,
        candidates: vec![node("B")],
      },
    );
    let status = manager.snapshot();
    assert_eq!(status.source, "cloud");
    assert_eq!(status.name, "192.0.2.20");
    assert!(!status.auto_switch);
  }
  #[tokio::test]
  async fn requires_three_failures_and_switches_without_resetting_session() {
    let manager = manager();
    let session = manager.connector.traffic_snapshot().session_id;
    for n in 0..3 {
      manager
        .tick(
          |node| async move { node.name == "B" },
          |nodes| async { nodes },
        )
        .await;
      assert_eq!(manager.snapshot().name, if n < 2 { "A" } else { "B" });
    }
    assert_eq!(manager.connector.current_node().unwrap().name, "B");
    assert_eq!(manager.connector.traffic_snapshot().session_id, session);
    assert_eq!(manager.snapshot().event_kind.as_deref(), Some("switched"));
  }
  #[tokio::test]
  async fn identical_policy_refresh_does_not_restart_failure_detection() {
    let manager = manager();
    for _ in 0..2 {
      manager
        .tick(|_| async { false }, |nodes| async { nodes })
        .await;
    }
    let policy = manager.policy.lock().unwrap().config.clone();
    manager.update(policy);
    manager
      .tick(
        |node| async move { node.name == "B" },
        |nodes| async { nodes },
      )
      .await;
    assert_eq!(manager.snapshot().name, "B");
  }
  #[tokio::test]
  async fn all_unavailable_stops_checks_and_keeps_one_event() {
    let manager = manager();
    for _ in 0..9 {
      manager
        .tick(|_| async { false }, |nodes| async { nodes })
        .await;
    }
    assert_eq!(manager.snapshot().event_id, 1);
    assert_eq!(manager.snapshot().name, "A");
    manager
      .tick(|_| async { true }, |nodes| async { nodes })
      .await;
    assert!(!manager.snapshot().running);
    assert_eq!(manager.snapshot().event_id, 1);
    assert_eq!(
      manager.snapshot().event_kind.as_deref(),
      Some("unavailable")
    );
  }
  #[tokio::test]
  async fn disabling_during_probe_prevents_stale_switch() {
    let manager = manager();
    for _ in 0..2 {
      manager
        .tick(|_| async { false }, |nodes| async { nodes })
        .await;
    }
    manager
      .tick(
        |_| async { false },
        |nodes| {
          manager.update(FailoverConfig::default());
          async { nodes }
        },
      )
      .await;
    assert_eq!(manager.snapshot().name, "A");
    assert_eq!(manager.snapshot().event_id, 0);
  }
  #[tokio::test]
  async fn cancellation_stops_monitor_without_any_switch() {
    let manager = manager();
    let cancel = CancellationToken::new();
    cancel.cancel();
    manager.clone().run(cancel).await;
    assert_eq!(manager.snapshot().event_id, 0);
  }
}
