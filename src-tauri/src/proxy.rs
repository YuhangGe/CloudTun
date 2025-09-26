use std::sync::Arc;

use cloudtun_proxy::{run_proxy_loop, ProxyArgs};
use tauri::{AppHandle, Runtime};
use tokio_util::sync::CancellationToken;

use crate::util::emit_log;

use anyhow_tauri::{IntoTAResult, TAResult};
use tauri::State;
use tokio::sync::Mutex;

pub struct ProxyLoop(Arc<Mutex<Option<CancellationToken>>>);

impl ProxyLoop {
  pub fn new() -> Self {
    Self(Arc::new(Mutex::new(None)))
  }
}

// async fn read<R: Runtime, T: AsyncRead + Unpin>(stdo: T, h: &AppHandle<R>) {
//   let reader = tokio::io::BufReader::new(stdo);
//   let mut lines_reader = reader.lines();
//   loop {
//     match lines_reader.next_line().await {
//       Ok(line) => {
//         if let Some(l) = line {
//           emit_log(h, "log::v2ray", &l);
//         } else {
//           break;
//         }
//       }
//       Err(e) => {
//         eprintln!("{}", e);
//         break;
//       }
//     }
//   }
// }

pub async fn stop_proxy_client(state: State<'_, ProxyLoop>) {
  let proxy_loop = state.0.clone();
  if let Some(proc) = proxy_loop.lock().await.take() {
    proc.cancel();
  };
}

async fn start_proxy_client<R: Runtime>(
  h: AppHandle<R>,
  state: State<'_, ProxyLoop>,
  server_ip: &str,
  token: &str,
  cvm_id: &str,
) -> anyhow::Result<()> {
  emit_log(&h, "log::proxy", "starting proxy client...");
  let proxy_loop = state.0.clone();
  if let Some(proc) = proxy_loop.lock().await.take() {
    proc.cancel();
  }

  let mut password = Vec::with_capacity(16);
  let cvm_id_bytes = cvm_id.as_bytes();
  let len = cvm_id_bytes.len();
  for i in 0..16 {
    password.push(cvm_id_bytes[i % len]);
  }
  let proxy_args = ProxyArgs {
    server_addr: (server_ip.to_string(), 24816, token.to_string()),
    local_addr: ("0.0.0.0".to_string(), 7892),
    default_rule: cloudtun_proxy::MatchType::Proxy,
    rules_config_file: None,
    password,
  };
  let shutdown_token = CancellationToken::new();
  let h2 = h.clone();
  let log_fn = move |log_type: &str, log_message: &str| {
    emit_log(&h2, log_type, log_message);
  };
  tokio::spawn(async move {
    {
      proxy_loop
        .clone()
        .lock()
        .await
        .replace(shutdown_token.clone());
    }
    if let Err(e) = run_proxy_loop(proxy_args, shutdown_token, log_fn).await {
      eprintln!("failed run_proxy_loop: {e}");
    }
  });

  Ok(())
}

#[tauri::command]
pub async fn tauri_start_proxy_client<R: Runtime>(
  handle: AppHandle<R>,
  state: State<'_, ProxyLoop>,
  server_ip: &str,
  token: &str,
  cvm_id: &str,
) -> TAResult<()> {
  start_proxy_client(handle, state, server_ip, token, cvm_id)
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
