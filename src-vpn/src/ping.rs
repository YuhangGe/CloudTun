use cloudtun_common::ping::ping_cloudtun_proxy_server;
use std::time::Duration;
use tokio::time::{MissedTickBehavior, interval};
use tokio_util::sync::CancellationToken;
pub async fn start_ping_interval(
  server: &cloudtun_common::tls::CloudEndpoint,
  cancel: &CancellationToken,
) {
  let mut ticks = interval(Duration::from_secs(30));
  ticks.set_missed_tick_behavior(MissedTickBehavior::Skip);
  loop {
    tokio::select! {biased;_=cancel.cancelled()=>break,_=ticks.tick()=>{}}
    let success = tokio::select! {biased;_=cancel.cancelled()=>break,result=ping_cloudtun_proxy_server(server)=>result.unwrap_or(false)};
    if !success {
      log::warn!("cloudtun server ping failed");
    }
  }
}

#[cfg(any(target_os = "android", test))]
pub async fn monitor_cloud(
  server: std::sync::Arc<cloudtun_common::tls::CloudEndpoint>,
  monitor: Option<cloudtun_proxy::cloud_health::CloudMonitor>,
  cancel: CancellationToken,
  status: std::sync::Arc<crate::failover::Failover>,
) {
  let mut ticks = interval(Duration::from_secs(30));
  ticks.set_missed_tick_behavior(MissedTickBehavior::Skip);
  let mut failures = 0u32;
  loop {
    tokio::select! { biased; _ = cancel.cancelled() => break, _ = ticks.tick() => {} }
    let success = tokio::select! { biased; _ = cancel.cancelled() => break, result = ping_cloudtun_proxy_server(&server) => result.unwrap_or(false) };
    if success {
      failures = 0;
      continue;
    }
    failures = failures.saturating_add(1);
    if failures < 3 {
      continue;
    }
    let Some(monitor) = &monitor else {
      continue;
    };
    let result = tokio::select! { biased; _ = cancel.cancelled() => break, result = monitor.confirm_missing(&server.identity) => result };
    match result {
      Ok(true) if !cancel.is_cancelled() => {
        status.cloud_lost();
        break;
      }
      Ok(_) => log::info!("cloud instance still exists; keeping VPN"),
      Err(_) => log::warn!("cannot confirm cloud instance loss; keeping VPN"),
    }
  }
}
