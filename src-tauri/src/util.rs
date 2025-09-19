use anyhow_tauri::TAResult;
use base64::Engine;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use uuid::Uuid;

pub fn emit_log<R: Runtime>(h: &AppHandle<R>, log_type: &str, log_message: &str) {
  println!("[{}] {}", log_type, log_message);
  let _ = h.emit(log_type, log_message);
}

#[cfg(desktop)]
use std::process;

#[tauri::command]
pub fn tauri_generate_uuid() -> TAResult<String> {
  let id = Uuid::new_v4();
  Ok(id.to_string())
}

#[cfg(desktop)]
#[tauri::command]
pub async fn tauri_exit_process() -> TAResult<()> {
  process::exit(0);
}

#[tauri::command]
pub async fn tauri_base64_covert(content: &str) -> TAResult<String> {
  Ok(base64::engine::general_purpose::STANDARD.encode(content))
}

#[cfg(desktop)]
#[tauri::command]
pub fn tauri_open_devtools<R: Runtime>(h: AppHandle<R>) -> TAResult<()> {
  println!("open dev tools");
  if let Some(win) = h.get_webview_window("main") {
    win.open_devtools();
  }
  Ok(())
}
