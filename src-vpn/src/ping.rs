use cloudtun_common::ping::ping_cloudtun_proxy_server;
use std::time::Duration;
use tokio::time::{MissedTickBehavior, interval};
use tokio_util::sync::CancellationToken;
pub async fn start_ping_interval(ip: &str, token: &str, cancel: &CancellationToken) {
  let mut ticks = interval(Duration::from_secs(30));
  ticks.set_missed_tick_behavior(MissedTickBehavior::Skip);
  loop {
    tokio::select! {biased;_=cancel.cancelled()=>break,_=ticks.tick()=>{}}
    let success = tokio::select! {biased;_=cancel.cancelled()=>break,result=ping_cloudtun_proxy_server(ip,token)=>result};
    if !success {
      log::warn!("cloudtun server ping failed: {ip}");
    }
  }
}
