use crate::{
  MatchType, RoutingMode,
  outbound::{DynamicOutbound, Outbound},
  routing::Routing,
  traffic::TrafficCounter,
};
use std::{io, sync::Arc};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum VpnOutbound {
  Cloud {
    host: String,
    port: u16,
    identity: String,
    token: String,
    trust_dir: std::path::PathBuf,
  },
  Subscription {
    node: crate::subscription::SubscriptionNode,
  },
}
#[derive(serde::Serialize, serde::Deserialize)]
pub struct VpnConfig {
  #[serde(default)]
  pub cloud_monitor: Option<crate::cloud_health::CloudMonitor>,
  #[serde(default)]
  pub failover: FailoverConfig,
  pub outbound: VpnOutbound,
  pub mode: RoutingMode,
  pub rules: Option<crate::china::ChinaRules>,
}
#[derive(Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FailoverConfig {
  pub enabled: bool,
  pub candidates: Vec<crate::subscription::SubscriptionNode>,
}
impl VpnConfig {
  pub fn connector(self) -> io::Result<ProxyConnector> {
    let outbound = match self.outbound {
      VpnOutbound::Cloud {
        host,
        port,
        identity,
        token,
        trust_dir,
      } => Outbound::Cloud {
        server: Arc::new(cloudtun_common::tls::CloudEndpoint::new(
          host, port, identity, token, trust_dir,
        )?),
      },
      VpnOutbound::Subscription { node } => {
        if node.unsupported_reason.is_some() || node.vmess.is_none() {
          return Err(io::Error::other("unsupported node"));
        }
        Outbound::Vmess(node)
      }
    };
    let rules = Arc::new(crate::china::ChinaRulesStore::default());
    if let Some(data) = self.rules {
      rules.replace(data);
    }
    Ok(ProxyConnector::new(
      outbound,
      Routing::with_rules(self.mode, rules),
      TrafficCounter::new(),
    ))
  }
}

/// TUN sessions share the HTTP proxy's outbound, live routing rules and counters.
#[derive(Clone)]
pub struct ProxyConnector {
  outbound: DynamicOutbound,
  routing: Routing,
  traffic: TrafficCounter,
}
impl ProxyConnector {
  pub fn cloud_endpoint(&self) -> Option<Arc<cloudtun_common::tls::CloudEndpoint>> {
    match self.outbound.snapshot().0 {
      Outbound::Cloud { server } => Some(server),
      Outbound::Vmess(_) => None,
    }
  }
  pub fn config(&self) -> VpnConfig {
    let outbound = match self.outbound.snapshot().0 {
      Outbound::Cloud { server } => VpnOutbound::Cloud {
        host: server.host.clone(),
        port: server.port,
        identity: server.identity.clone(),
        token: server.token.clone(),
        trust_dir: server.trust_dir.clone(),
      },
      Outbound::Vmess(node) => VpnOutbound::Subscription { node: node.clone() },
    };
    VpnConfig {
      cloud_monitor: None,
      failover: FailoverConfig::default(),
      outbound,
      mode: self.routing.current_mode(),
      rules: self.routing.rules().snapshot().1.as_deref().cloned(),
    }
  }
  pub fn update_routing(&self, config: VpnConfig) {
    self.routing.set_mode(config.mode);
    if let Some(rules) = config.rules {
      self.routing.rules().replace(rules);
    }
  }
  pub fn traffic_snapshot(&self) -> crate::TrafficSnapshot {
    self.traffic.snapshot()
  }

  pub(crate) fn new(outbound: Outbound, routing: Routing, traffic: TrafficCounter) -> Self {
    Self {
      outbound: DynamicOutbound::new(outbound),
      routing,
      traffic,
    }
  }
  pub(crate) fn with_shared(
    outbound: DynamicOutbound,
    routing: Routing,
    traffic: TrafficCounter,
  ) -> Self {
    Self {
      outbound,
      routing,
      traffic,
    }
  }
  pub fn switch_node(&self, node: crate::subscription::SubscriptionNode) -> io::Result<()> {
    self.outbound.set_node(node)
  }
  pub fn current_node(&self) -> Option<crate::subscription::SubscriptionNode> {
    match self.outbound.snapshot().0 {
      Outbound::Vmess(node) => Some(node),
      _ => None,
    }
  }
  pub fn routing_mode(&self) -> RoutingMode {
    self.routing.current_mode()
  }
  pub fn cloud(server: cloudtun_common::tls::CloudEndpoint) -> Self {
    Self::new(
      Outbound::Cloud {
        server: Arc::new(server),
      },
      Routing::mode(RoutingMode::Global),
      TrafficCounter::new(),
    )
  }
  pub async fn relay_tcp<S, F>(
    &self,
    local: S,
    host: String,
    port: u16,
    cancel: CancellationToken,
    log: Arc<F>,
  ) -> io::Result<()>
  where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    F: Fn(&str, &str) + Send + Sync + 'static,
  {
    let mut local = crate::traffic::MeteredStream(local, self.traffic.clone());
    let tracker = TaskTracker::new();
    let (outbound, epoch) = self.outbound.snapshot();
    let session = epoch.child_token();
    let result = async {
      let route = self.routing.decide(&host, port).await;
      let mut remote: crate::outbound::AnyStream =
        tokio::time::timeout(cloudtun_common::proxy::CONNECT_TIMEOUT, async {
          match route {
            MatchType::Direct => {
              Ok(Box::new(crate::tcp::connect(&host, port).await?) as crate::outbound::AnyStream)
            }
            MatchType::Proxy => {
              outbound
                .connect(&host, port, session.clone(), tracker.clone(), log.clone())
                .await
            }
            MatchType::Deny => Err(io::Error::other("route denied")),
          }
        })
        .await??;
      log("proxy::info", &format!("VPN TCP ==> {host}:{port}"));
      tokio::io::copy_bidirectional(&mut local, &mut remote).await?;
      Ok(())
    };
    let result = tokio::select! { _ = cancel.cancelled() => Ok(()), _ = epoch.cancelled() => Ok(()), result = result => result };
    session.cancel();
    tracker.close();
    tracker.wait().await;
    result
  }
  pub async fn relay_udp<S, F>(
    &self,
    local: S,
    host: String,
    port: u16,
    cancel: CancellationToken,
    log: Arc<F>,
    max_response: usize,
  ) -> io::Result<()>
  where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    F: Fn(&str, &str) + Send + Sync + 'static,
  {
    let local = crate::traffic::MeteredStream(local, self.traffic.clone());
    let (outbound, epoch) = self.outbound.snapshot();
    let relay = async {
      let route = self.routing.decide(&host, port).await;
      if matches!(route, MatchType::Deny) {
        return Err(io::Error::other("route denied"));
      }
      if matches!(route, MatchType::Proxy) {
        if let Outbound::Cloud { server } = &outbound {
          return cloudtun_common::proxy::proxy_udp_to_cloudtun_server(
            local,
            server.clone(),
            host,
            port,
            log,
            max_response,
          )
          .await;
        }
      }
      let (mut reader, mut writer) = tokio::io::split(local);
      let mut upload = vec![0; 65535];
      let mut download = vec![0; 65535];
      if matches!(route, MatchType::Direct) {
        let target = tokio::net::lookup_host((host.as_str(), port))
          .await?
          .next()
          .ok_or_else(|| io::Error::other("DNS returned no addresses"))?;
        let socket = tokio::net::UdpSocket::bind(if target.is_ipv4() {
          "0.0.0.0:0"
        } else {
          "[::]:0"
        })
        .await?;
        socket.connect(target).await?;
        loop {
          tokio::select! {
            n = reader.read(&mut upload) => { let n = n?; socket.send(&upload[..n]).await?; },
            n = socket.recv(&mut download) => { let n = n?; if n <= max_response { writer.write_all(&download[..n]).await?; } },
          }
        }
      }
      let Outbound::Vmess(node) = &outbound else {
        unreachable!()
      };
      let datagram = tokio::time::timeout(
        cloudtun_common::proxy::CONNECT_TIMEOUT,
        crate::outbound::vmess_datagram(node, &host, port),
      )
      .await??;
      let (mut receive, mut send) = datagram.split();
      let target = leaf::session::SocksAddr::try_from((host.as_str(), port))?;
      log("proxy::info", &format!("VPN UDP ==> {host}:{port}"));
      loop {
        tokio::select! {
          n = reader.read(&mut upload) => { let n = n?; send.send_to(&upload[..n], &target).await?; },
          packet = receive.recv_from(&mut download) => { let (n, _) = packet?; if n == 0 { return Ok(()); } if n <= max_response { writer.write_all(&download[..n]).await?; } },
        }
      }
    };
    tokio::select! { _ = cancel.cancelled() => Ok(()), _ = epoch.cancelled() => Ok(()), result = relay => result }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use tokio::net::{TcpListener, UdpSocket};

  fn direct_connector() -> ProxyConnector {
    // This endpoint is never reached: loopback traffic must be routed directly.
    ProxyConnector::cloud(
      cloudtun_common::tls::CloudEndpoint::new(
        "192.0.2.1".into(),
        24816,
        "test".into(),
        "a".repeat(64),
        std::path::PathBuf::from("unused"),
      )
      .unwrap(),
    )
  }

  #[tokio::test]
  async fn vpn_tcp_forwards_both_directions_and_counts_live_traffic() {
    let connector = direct_connector();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (mut client, tunnel) = tokio::io::duplex(4096);
    let cancel = CancellationToken::new();
    let relay = tokio::spawn({
      let connector = connector.clone();
      let cancel = cancel.clone();
      async move {
        connector
          .relay_tcp(
            tunnel,
            "127.0.0.1".into(),
            port,
            cancel,
            Arc::new(|_: &str, _: &str| {}),
          )
          .await
      }
    });
    let (mut remote, _) = listener.accept().await.unwrap();
    client.write_all(b"request").await.unwrap();
    let mut request = [0; 7];
    remote.read_exact(&mut request).await.unwrap();
    assert_eq!(&request, b"request");
    remote.write_all(b"response").await.unwrap();
    let mut response = [0; 8];
    client.read_exact(&mut response).await.unwrap();
    assert_eq!(&response, b"response");
    assert_eq!(connector.traffic_snapshot().total_bytes, 15);
    cancel.cancel();
    tokio::time::timeout(std::time::Duration::from_secs(1), relay)
      .await
      .unwrap()
      .unwrap()
      .unwrap();
  }

  #[tokio::test]
  async fn vpn_udp_preserves_packets_and_drops_responses_larger_than_mtu() {
    let connector = direct_connector();
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let port = socket.local_addr().unwrap().port();
    let (mut client, tunnel) = tokio::io::duplex(4096);
    let cancel = CancellationToken::new();
    let relay = tokio::spawn({
      let cancel = cancel.clone();
      async move {
        connector
          .relay_udp(
            tunnel,
            "127.0.0.1".into(),
            port,
            cancel,
            Arc::new(|_: &str, _: &str| {}),
            10,
          )
          .await
      }
    });
    client.write_all(b"packet").await.unwrap();
    let mut packet = [0; 64];
    let (n, peer) = socket.recv_from(&mut packet).await.unwrap();
    assert_eq!(&packet[..n], b"packet");
    socket.send_to(b"too large packet", peer).await.unwrap();
    socket.send_to(b"ok", peer).await.unwrap();
    let n = client.read(&mut packet).await.unwrap();
    assert_eq!(&packet[..n], b"ok");
    cancel.cancel();
    tokio::time::timeout(std::time::Duration::from_secs(1), relay)
      .await
      .unwrap()
      .unwrap()
      .unwrap();
  }

  #[test]
  fn independent_library_config_round_trip_keeps_routing_and_outbound() {
    let connector = direct_connector();
    let mut config = connector.config();
    config.rules = Some(
      crate::china::ChinaRules::parse(
        "1.0.1.0/24",
        "2400::/12",
        "server=/example.cn/114.114.114.114",
      )
      .unwrap(),
    );
    let json = serde_json::to_string(&config).unwrap();
    let config: VpnConfig = serde_json::from_str(&json).unwrap();
    let rebuilt = config.connector().unwrap();
    assert!(matches!(
      rebuilt.config().outbound,
      VpnOutbound::Cloud { port: 24816, .. }
    ));
    let mut update = rebuilt.config();
    update.mode = RoutingMode::BypassChina;
    rebuilt.update_routing(update);
    assert!(matches!(rebuilt.config().mode, RoutingMode::BypassChina));
  }
}
