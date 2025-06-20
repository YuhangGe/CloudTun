use std::{sync::Arc, time::Duration};

use anyhow_tauri::TAResult;
use hmac::digest::KeyInit;
use serde::Serialize;
use tauri::{
  AppHandle, Emitter, EventTarget, Listener, LogicalPosition, Manager, Position, Runtime, State,
  Webview, WebviewUrl, WebviewWindow, WebviewWindowBuilder, Window, Wry,
};
use tokio::{sync::Mutex, task, time::interval};

use crate::util::emit_log;

pub struct NotifyWindow<R: Runtime>(Arc<Mutex<Option<WebviewWindow<R>>>>);

impl<R: Runtime> NotifyWindow<R> {
  pub fn new() -> Self {
    NotifyWindow(Arc::new(Mutex::new(None)))
  }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct NotifyPayload {
  notify_type: String,
  notify_message: String,
}

const WINDOW_WIDTH: f64 = 230.0;
const WINDOW_HEIGHT: f64 = 80.0;
const POSITION_Y: f64 = 90.0;

pub async fn show_notify_window<R: Runtime>(
  notify_type: &str,
  notify_message: &str,
  h: AppHandle<R>,
  state: State<'_, NotifyWindow<R>>,
) -> anyhow::Result<()> {
  emit_log(
    &h,
    "log::notify",
    &format!("Show notify window: {}, {}", notify_type, notify_message),
  );
  let loc = state.0.clone();
  let has_window = loc.clone().lock().await.is_some();
  if has_window {
    println!("emit notify-state-changed");
    let _ = h.emit_to(
      EventTarget::webview_window("notify_window"),
      "notify-state-changed",
      NotifyPayload {
        notify_type: notify_type.to_string(),
        notify_message: notify_message.to_string(),
      },
    );
  } else {
    let monitor = h.primary_monitor().unwrap().unwrap();
    let sw = (monitor.size().width as f64) / monitor.scale_factor();
    let entered_left = (sw - WINDOW_WIDTH - 40.0).floor();
    let leaved_left = sw.floor();

    let builder = WebviewWindowBuilder::new(
      &h,
      "notify_window",
      WebviewUrl::App(
        format!(
          "/notify.html?type={}&message={}",
          notify_type, notify_message
        )
        .into(),
      ),
    )
    .inner_size(WINDOW_WIDTH, WINDOW_HEIGHT)
    .position(leaved_left, POSITION_Y)
    .closable(false)
    .minimizable(false)
    .maximizable(false)
    .resizable(false)
    .skip_taskbar(true)
    .always_on_top(true)
    .decorations(cfg!(target_os = "macos"))
    .visible(true);

    #[cfg(target_os = "macos")]
    let builder = builder
      .title_bar_style(tauri::TitleBarStyle::Overlay)
      .hidden_title(true);

    let win = builder.build().unwrap();
    let loc2 = loc.clone();
    loc.lock().await.replace(win);

    let _ = task::spawn(async move {
      let mut interval = interval(Duration::from_millis(20));
      let mut current_left = leaved_left;
      loop {
        interval.tick().await;

        let win = loc2.lock().await;
        if let Some(x) = win.as_ref() {
          current_left -= 10.0;
          if current_left < entered_left {
            break;
          } else {
            x.set_position(Position::Logical(LogicalPosition::new(
              current_left,
              POSITION_Y,
            )))
            .unwrap();
          }
        } else {
          break;
        }
      }
    });
  }
  Ok(())
}

async fn close_notify_window<R: Runtime>(state: State<'_, NotifyWindow<R>>) -> anyhow::Result<()> {
  let loc = state.0.clone();
  if let Some(x) = loc.clone().lock().await.as_ref() {
    let _ = x.destroy();
  }
  loc.lock().await.take();
  Ok(())
}
#[tauri::command]
pub async fn tauri_show_notify_window<R: Runtime>(
  notify_type: &str,
  notify_message: &str,
  handle: AppHandle<R>,
  state: State<'_, NotifyWindow<R>>,
) -> TAResult<()> {
  let _ = show_notify_window(notify_type, notify_message, handle, state).await;
  Ok(())
}

#[tauri::command]
pub async fn tauri_close_notify_window<R: Runtime>(
  _handle: AppHandle<R>,
  state: State<'_, NotifyWindow<R>>,
) -> TAResult<()> {
  let _ = close_notify_window(state).await;
  Ok(())
}
