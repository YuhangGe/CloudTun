use std::{
  sync::Arc,
  time::{Duration, Instant},
};

use cloudtun_common::tencent::TencentCloudClient;
use futures_util::lock::Mutex;

#[derive(Debug)]
pub struct Context {
  pub token: String,
  pub password: Arc<Vec<u8>>,
  pub cvm_name: String,
  last_ping_ts: Mutex<Instant>,
  pub tx: TencentCloudClient,
}

impl Context {
  pub fn new(token: String, password: Vec<u8>, cvm_name: String, tx: TencentCloudClient) -> Self {
    Context {
      token,
      password: Arc::new(password),
      cvm_name,
      last_ping_ts: Mutex::new(Instant::now()),
      tx,
    }
  }

  pub async fn touch_ping_ts(&self) {
    let mut x = self.last_ping_ts.lock().await;
    *x = Instant::now();
  }

  /// 检查最近一次 ping 是否已经过期。10分钟过期。
  pub async fn is_ping_expired(&self) -> bool {
    self.last_ping_ts.lock().await.elapsed() > Duration::from_secs(60 * 10)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  #[tokio::test]
  async fn proxy_activity_resets_the_idle_reclamation_timer() {
    let ctx = Context::new(
      "token".into(),
      vec![0; 16],
      "unused".into(),
      TencentCloudClient::new("unused".into(), "unused".into(), "unused".into()),
    );
    assert!(!ctx.is_ping_expired().await);
    *ctx.last_ping_ts.lock().await = Instant::now() - Duration::from_secs(601);
    assert!(ctx.is_ping_expired().await);
    ctx.touch_ping_ts().await;
    assert!(!ctx.is_ping_expired().await);
  }
}
