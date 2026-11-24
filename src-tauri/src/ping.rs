use std::{sync::Arc, time::Duration};

use anyhow_tauri::{IntoTAResult, TAResult};
use cloudtun_common::ping::ping_cloudtun_proxy_server;
use tauri::{AppHandle, Manager, Runtime, State};
use tokio::{sync::Mutex, time::interval};
use tokio_util::sync::CancellationToken;

use crate::util::emit_log;

use cloudtun_proxy::cloud_health::instance_missing;
pub use cloudtun_proxy::cloud_health::CloudMonitor;

async fn confirm_missing<R: Runtime>(
  app: &AppHandle<R>,
  cloud: Option<&CloudMonitor>,
  id: &str,
) -> anyhow::Result<bool> {
  let cloud = cloud.ok_or_else(|| anyhow::anyhow!("未配置实例核查凭据，保留代理"))?;
  anyhow::ensure!(
    !cloud.secret_id.is_empty() && !cloud.secret_key.is_empty() && !cloud.region.is_empty(),
    "实例核查配置不完整，保留代理"
  );
  let value = crate::tencent::tauri_call_tencent_api(
    app.clone(),
    cloud.secret_id.clone(),
    cloud.secret_key.clone(),
    "cvm".into(),
    cloud.region.clone(),
    "DescribeInstances".into(),
    serde_json::json!({"InstanceIds": [id], "Limit": 1}).to_string(),
  )
  .await
  .map_err(|_| anyhow::anyhow!("实例状态查询失败，保留代理并继续重试"))?;
  instance_missing(&value, id)
}

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
  cloud: Option<CloudMonitor>,
  h: AppHandle<R>,
  state: State<'_, Ping>,
) -> TAResult<()> {
  let server = h
    .state::<crate::cloud_access::CloudAccess>()
    .endpoint(ip, token)
    .into_ta_result()?;
  let cancel_token = CancellationToken::new();
  let session_id = h
    .state::<crate::proxy::ProxyLoop>()
    .traffic_snapshot()
    .await
    .map(|snapshot| snapshot.session_id);
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
        if failures >= 3 {
          let missing = tokio::select! {
            biased;
            _ = cancel_token.cancelled() => break,
            result = confirm_missing(&h, cloud.as_ref(), &server.identity) => result,
          };
          match missing {
            Ok(false) => {
              emit_log(
                &h,
                "log::ping",
                "心跳失败，但云主机实例仍存在；保留代理并继续重试。",
              );
              continue;
            }
            Err(error) => {
              emit_log(&h, "log::ping", &error.to_string());
              continue;
            }
            Ok(true) => {}
          }
          let Some(session_id) = session_id else {
            break;
          };
          let stopped = h
            .state::<crate::proxy::ProxyLoop>()
            .stop_lost_cloud(&h, session_id, &cancel_token)
            .await;
          if matches!(stopped, Ok(false)) {
            break;
          }
          if let Err(error) = stopped {
            emit_log(
              &h,
              "proxy::error",
              &format!("实例已丢失，清理代理设置失败：{error}"),
            );
          }
          #[cfg(desktop)]
          {
            let h2 = h.clone();
            let h3 = h2.clone();
            let w = h3.state::<NotifyWindow<R>>();
            let _ = show_notify_window(
              "error",
              cloudtun_proxy::cloud_health::INSTANCE_LOST_MESSAGE,
              h2,
              w,
            )
            .await;
          }
          emit_log(
            &h,
            "log::disconnected",
            "云平台确认实例已丢失，代理已自动关闭。",
          );
          break;
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
