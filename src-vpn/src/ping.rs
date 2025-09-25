use std::time::Duration;

use cloudtun_common::ping::ping_cloudtun_proxy_server;
use tokio::time::interval;
use tokio_util::sync::CancellationToken;

pub async fn start_ping_interval(ip: &str, token: &str, cancel_token: &CancellationToken) {
  let mut interval = interval(Duration::from_secs(60));
  println!("cloudtun server ping started");
  loop {
    interval.tick().await;
    if cancel_token.is_cancelled() {
      // println!("xxx cancelled 22");
      break;
    }
    let success = ping_cloudtun_proxy_server(&ip, &token).await;
    if cancel_token.is_cancelled() {
      // println!("xxx cancelled 33");
      break;
    }
    if !success {
      // TODO：通过 kotlin/swift 层把广播传递出去
      eprintln!("cloudtun server ping failed!");
    } else {
      println!("cloudtun server ping success!");
      // emit_log(&h, "log::ping", "远程 V2Ray 运行中，服务器正常响应！");
    }
  }
  println!("cloudtun server ping stopped");
}
