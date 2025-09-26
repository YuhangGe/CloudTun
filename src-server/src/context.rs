use std::{sync::Arc, time};

use cloudtun_common::tencent::TencentCloudClient;
use futures_util::lock::Mutex;

#[derive(Debug)]
pub struct Context {
  pub token: String,
  pub password: Arc<Vec<u8>>,
  pub cvm_name: String,
  last_ping_ts: Mutex<u64>,
  pub tx: TencentCloudClient,
}

pub fn now_ts() -> u64 {
  time::SystemTime::now()
    .duration_since(time::UNIX_EPOCH)
    .unwrap()
    .as_secs()
}

impl Context {
  pub fn new(token: String, password: Vec<u8>, cvm_name: String, tx: TencentCloudClient) -> Self {
    Context {
      token,
      password: Arc::new(password),
      cvm_name,
      last_ping_ts: Mutex::new(now_ts()),
      tx,
    }
  }

  pub async fn touch_ping_ts(&self) {
    let mut x = self.last_ping_ts.lock().await;
    *x = now_ts();
  }

  /// 检查最近一次 ping 是否已经过期。10分钟过期。
  pub async fn is_ping_expired(&self) -> bool {
    let now = now_ts();
    let x = self.last_ping_ts.lock().await;
    let diff = now - *x;
    println!("ping check {diff}");
    // diff > 10
    diff > 60 * 10
  }
}
