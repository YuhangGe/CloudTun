mod notify;
mod ping;
mod tencent;
mod util;
mod v2ray;

use tauri::{Manager, WebviewWindowBuilder, WindowEvent, Wry};

use notify::*;
use ping::*;
use tencent::*;
use util::*;
use v2ray::*;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .plugin(tauri_plugin_autostart::init(
      tauri_plugin_autostart::MacosLauncher::LaunchAgent,
      None,
    ))
    .plugin(tauri_plugin_clipboard_manager::init())
    .plugin(tauri_plugin_os::init())
    .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
      let win = app.get_webview_window("main").expect("no main window");
      win.show().unwrap();
      let _ = win.set_focus();
    }))
    .plugin(tauri_plugin_notification::init())
    .plugin(tauri_plugin_store::Builder::new().build())
    .plugin(tauri_plugin_http::init())
    .invoke_handler(tauri::generate_handler![
      tauri_generate_uuid,
      tauri_exit_process,
      tauri_open_devtools,
      tauri_calc_tencent_cloud_api_signature,
      tauri_start_v2ray_server,
      tauri_stop_v2ray_server,
      tauri_kill_progress_by_pid,
      tauri_show_notify_window,
      tauri_close_notify_window,
      tauri_interval_ping_start,
      tauri_interval_ping_stop
    ])
    .setup(|_app| {
      #[cfg(desktop)]
      _app.manage(V2RayProc::new());
      _app.manage(NotifyWindow::<Wry>::new());
      _app.manage(Ping::new());
      Ok(())
    })
    .on_window_event(|window, event| match event {
      WindowEvent::CloseRequested { api, .. } => {
        // api.prevent_close();
        // window.hide().unwrap();
        // TODO: 如果正在创建实例，不允许关闭。
      }
      _ => {} // event.window().hide().unwrap();
    })
    .build(tauri::generate_context!())
    .expect("error while running tauri application")
    .run(|app, event| match event {
      tauri::RunEvent::ExitRequested { api, code, .. } => {
        if code.is_none() {
          api.prevent_exit();
        } else {
          //
        }
      }
      tauri::RunEvent::Reopen { .. } => {
        if let Some(x) = app.get_webview_window("main") {
          x.show().unwrap();
          let _ = x.set_focus();
        } else {
          let cfg = &app.config().app.windows[0];
          let _ = WebviewWindowBuilder::new(
            app,
            "main",
            tauri::WebviewUrl::App("/index.html?mode=reopen".into()),
          )
          .title(cfg.title.clone())
          .inner_size(cfg.width, cfg.height)
          .build()
          .unwrap();
        }
      }
      _ => {
        // println!("event: {:?}", event);
      }
    });
}
