use std::{sync::Arc, time::Duration};

use anyhow_tauri::TAResult;
use tauri::{
  http::{HeaderMap, HeaderValue, StatusCode},
  AppHandle, Manager, Runtime, State,
};
use tokio::{sync::Mutex, task::JoinHandle, time::interval};

use crate::util::emit_log;

#[cfg(desktop)]
use crate::notify::{show_notify_window, NotifyWindow};

pub struct Ping(Arc<Mutex<Option<JoinHandle<()>>>>);

impl Ping {
  pub fn new() -> Self {
    Ping(Arc::new(Mutex::new(None)))
  }
}

async fn ping(ip: &str, token: &str) -> bool {
  let req = tauri_plugin_http::reqwest::Client::new();
  let mut headers = HeaderMap::new();
  headers.insert("x-token", HeaderValue::from_str(token).unwrap());

  let x = req
    .get(format!("http://{}:24816/ping", ip))
    .headers(headers)
    .send()
    .await;
  // let x = get(format!("http://{}:2081/ping", ip, token)).await;
  let Ok(resp) = x else {
    return false;
  };
  if resp.status() != StatusCode::OK {
    return false;
  };
  let Ok(txt) = resp.text().await else {
    return false;
  };
  return txt.eq("pong!");
}

#[tauri::command]
pub async fn tauri_interval_ping_start<R: Runtime>(
  ip: &str,
  token: &str,
  h: AppHandle<R>,
  state: State<'_, Ping>,
) -> TAResult<()> {
  let loc = state.0.clone();
  if let Some(x) = loc.clone().lock().await.as_ref() {
    x.abort();
  }
  let a_ip = ip.to_string();
  let a_token = token.to_string();
  let loc2 = loc.clone();
  let handle = tokio::spawn(async move {
    let mut interval = interval(Duration::from_secs(60));
    loop {
      interval.tick().await;
      if !ping(&a_ip, &a_token).await {
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

        loc2.lock().await.take();
        break;
      } else {
        emit_log(&h, "log::ping", "远程 V2Ray 运行中，服务器正常响应！");
      }
    }
  });
  loc.lock().await.replace(handle);
  Ok(())
}

#[tauri::command]
pub async fn tauri_interval_ping_stop(state: State<'_, Ping>) -> TAResult<()> {
  let loc = state.0.clone();
  if let Some(x) = loc.lock().await.as_ref() {
    x.abort();
  }
  loc.clone().lock().await.take();
  Ok(())
}
