use std::{sync::Arc, time::Duration};

use anyhow_tauri::TAResult;
use cloudtun_common::ping::ping_cloudtun_proxy_server;
use tauri::{AppHandle, Manager, Runtime, State};
use tokio::{sync::Mutex, time::interval};
use tokio_util::sync::CancellationToken;

use crate::util::emit_log;

pub struct Ping(Arc<Mutex<Option<CancellationToken>>>);

impl Ping {
  pub fn new() -> Self {
    Ping(Arc::new(Mutex::new(None)))
  }
}

#[cfg(desktop)]
use crate::notify::{show_notify_window, NotifyWindow};

#[tauri::command]
pub async fn tauri_interval_ping_start<R: Runtime>(
  ip: &str,
  token: &str,
  h: AppHandle<R>,
  state: State<'_, Ping>,
) -> TAResult<()> {
  if let Some(prev_cancel_token) = state.0.lock().await.take() {
    prev_cancel_token.cancel();
  }

  let ip = ip.to_string();
  let token = token.to_string();
  let cancel_token = CancellationToken::new();
  state.0.lock().await.replace(cancel_token.clone());

  tokio::spawn(async move {
    let mut interval = interval(Duration::from_secs(60));
    println!("XXX ping interval started");
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
        #[cfg(desktop)]
        {
          let h2 = h.clone();
          let h3 = h2.clone();
          let w = h3.state::<NotifyWindow<R>>();
          let _ = show_notify_window("error", "V2Ray 远程主机失联！", h2, w).await;
        }
        emit_log(
          &h,
          "log::disconnected",
          "远程 V2Ray 响应异常，可能是竞价实例被回收，请刷新后重新购买！",
        );
        break;
      } else {
        emit_log(&h, "log::ping", "远程 V2Ray 运行中，服务器正常响应！");
      }
    }
    println!("XXX ping interval stopped");
  });
  Ok(())
}

#[tauri::command]
pub async fn tauri_interval_ping_stop(state: State<'_, Ping>) -> TAResult<()> {
  if let Some(x) = state.0.lock().await.take() {
    x.cancel();
  }
  Ok(())
}
