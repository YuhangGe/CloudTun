use std::{sync::Arc, time::Duration};

use anyhow_tauri::{IntoTAResult, TAResult};
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
use crate::notify::{NotifyWindow, show_notify_window};

#[tauri::command]
pub async fn tauri_interval_ping_start<R: Runtime>(
  ip: &str,
  token: &str,
  h: AppHandle<R>,
  state: State<'_, Ping>,
) -> TAResult<()> {
  let server = h
    .state::<crate::cloud_access::CloudAccess>()
    .endpoint(ip, token)
    .into_ta_result()?;
  let cancel_token = CancellationToken::new();
  if let Some(previous) = state.0.lock().await.replace(cancel_token.clone()) {
    previous.cancel();
  }

  tokio::spawn(async move {
    let mut interval = interval(Duration::from_secs(30));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut failures = 0u32;
    loop {
      tokio::select! { biased; _ = cancel_token.cancelled() => break, _ = interval.tick() => {} }
      let success = tokio::select! {
        biased;
        _ = cancel_token.cancelled() => break,
        result = ping_cloudtun_proxy_server(&server) => result.unwrap_or(false),
      };
      if !success {
        failures = failures.saturating_add(1);
        if failures == 3 {
          #[cfg(desktop)]
          {
            let h2 = h.clone();
            let h3 = h2.clone();
            let w = h3.state::<NotifyWindow<R>>();
            let _ = show_notify_window("error", "代理服务器连续失联，正在重试", h2, w).await;
          }
          emit_log(
            &h,
            "log::disconnected",
            "代理服务器连续失联，可能已被回收；将继续重试。",
          );
        }
      } else {
        if failures >= 3 {
          emit_log(&h, "log::ping", "代理服务器连接已恢复。");
        }
        failures = 0;
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
