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
) -> anyhow::Result<()> {
  emit_log(&h, "log::proxy", "starting proxy client...");
  let proxy_loop = state.0.clone();
  if let Some(proc) = proxy_loop.lock().await.take() {
    proc.cancel();
  }

  let proxy_args = ProxyArgs {
    server_addr: ("127.0.0.1".to_string(), 24816),
    local_addr: ("127.0.0.1".to_string(), 7891),
    default_rule: cloudtun_proxy::MatchType::Proxy,
    rules_config_file: None,
  };
  let shutdown_token = CancellationToken::new();
  tokio::task::spawn(async move {
    {
      proxy_loop
        .clone()
        .lock()
        .await
        .replace(shutdown_token.clone());
    }

    run_proxy_loop(proxy_args, shutdown_token)
  });

  Ok(())
}

#[tauri::command]
pub async fn tauri_start_proxy_client<R: Runtime>(
  handle: AppHandle<R>,
  state: State<'_, ProxyLoop>,
) -> TAResult<()> {
  start_proxy_client(handle, state).await.into_ta_result()
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
