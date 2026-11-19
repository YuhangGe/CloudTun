use std::sync::Arc;

use cloudtun_proxy::{PreparedProxy, ProxyArgs};
use tauri::{AppHandle, Runtime};
use tokio_util::sync::CancellationToken;

use crate::util::emit_log;

use anyhow_tauri::{IntoTAResult, TAResult};
use tauri::State;
use tokio::sync::Mutex;

struct RunningProxy {
  cancel: CancellationToken,
  task: tokio::task::JoinHandle<()>,
}
pub struct ProxyLoop(Arc<Mutex<Option<RunningProxy>>>);
impl ProxyLoop {
  pub fn new() -> Self {
    Self(Arc::new(Mutex::new(None)))
  }
}
pub async fn stop_proxy_client(state: State<'_, ProxyLoop>) {
  let mut state = state.0.lock().await;
  if let Some(running) = state.take() {
    running.cancel.cancel();
    let _ = running.task.await;
  }
}
async fn start_proxy_client<R: Runtime>(
  h: AppHandle<R>,
  state: State<'_, ProxyLoop>,
  server_ip: String,
  cvm_id: String,
  proxy_rules: Option<String>,
) -> anyhow::Result<()> {
  anyhow::ensure!(
    !server_ip.is_empty() && !cvm_id.is_empty(),
    "server IP and instance ID are required"
  );
  let mut state = state.0.lock().await;
  if let Some(running) = state.take() {
    running.cancel.cancel();
    let _ = running.task.await;
  }
  let password = cvm_id.as_bytes().iter().copied().cycle().take(16).collect();
  let proxy = PreparedProxy::bind(ProxyArgs {
    server_addr: (server_ip, 24816, cvm_id),
    local_addr: ("0.0.0.0".into(), 7892),
    default_rule: cloudtun_proxy::MatchType::Proxy,
    proxy_rules,
    password,
  })
  .await?;
  let cancel = CancellationToken::new();
  let token = cancel.clone();
  let task = tokio::spawn(async move {
    let h2 = h.clone();
    if let Err(e) = proxy
      .run(token, move |kind, message| emit_log(&h2, kind, message))
      .await
    {
      emit_log(&h, "proxy::error", &format!("proxy stopped: {e}"));
    }
  });
  *state = Some(RunningProxy { cancel, task });
  Ok(())
}

#[tauri::command]
pub async fn tauri_start_proxy_client<R: Runtime>(
  handle: AppHandle<R>,
  state: State<'_, ProxyLoop>,
  server_ip: String,
  cvm_id: String,
  proxy_rules: String,
) -> TAResult<()> {
  start_proxy_client(
    handle,
    state,
    server_ip,
    cvm_id,
    if proxy_rules.is_empty() {
      None
    } else {
      Some(proxy_rules)
    },
  )
  .await
  .into_ta_result()
}

#[tauri::command]
pub async fn tauri_stop_proxy_client<R: Runtime>(
  handle: AppHandle<R>,
  state: State<'_, ProxyLoop>,
) -> TAResult<()> {
  stop_proxy_client(state).await;
  emit_log(&handle, "log::proxy", "proxy client stopped.");
  Ok(())
}
