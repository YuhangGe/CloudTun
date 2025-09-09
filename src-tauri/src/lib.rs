#[cfg(desktop)]
mod notify;
#[cfg(desktop)]
mod v2ray;
#[cfg(mobile)]
mod vpn;

mod ping;
mod tencent;
mod util;

use tauri::{async_runtime::block_on, Manager, WebviewWindowBuilder};
use tauri::{AppHandle, Runtime, Wry};

#[cfg(desktop)]
use tauri::{
  menu::{Menu, MenuItem},
  tray::TrayIconBuilder,
};

#[cfg(desktop)]
use notify::*;
#[cfg(desktop)]
use v2ray::*;

#[cfg(mobile)]
use vpn::tauri_start_vpn;

use ping::*;
use tencent::*;
use util::*;

#[cfg(desktop)]
fn open_main_window<R: Runtime>(app: &AppHandle<R>) {
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  let mut builder = tauri::Builder::default()
    .plugin(tauri_plugin_os::init())
    .plugin(tauri_plugin_store::Builder::new().build())
    .plugin(tauri_plugin_http::init());

  #[cfg(desktop)]
  {
    builder = builder
      .plugin(tauri_plugin_autostart::init(
        tauri_plugin_autostart::MacosLauncher::LaunchAgent,
        None,
      ))
      .plugin(tauri_plugin_clipboard_manager::init())
      .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
        let win = app.get_webview_window("main").expect("no main window");
        win.show().unwrap();
        let _ = win.set_focus();
      }))
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
      ]);
  }

  #[cfg(mobile)]
  {
    use crate::vpn::init_tauri_vpn;

    builder = builder
      .plugin(init_tauri_vpn())
      .invoke_handler(tauri::generate_handler![
        tauri_generate_uuid,
        tauri_calc_tencent_cloud_api_signature,
        tauri_interval_ping_start,
        tauri_interval_ping_stop,
        tauri_start_vpn
      ]);
  }

  builder
    .setup(|_app| {
      #[cfg(desktop)]
      {
        _app.manage(V2RayProc::new());

        let quit_i = MenuItem::with_id(_app, "quit", "退出CloudV2Ray", true, None::<&str>).unwrap();
        let menu = Menu::with_items(_app, &[&quit_i]).unwrap();
        let _ = TrayIconBuilder::new()
          .icon(_app.default_window_icon().unwrap().clone())
          .menu(&menu)
          .tooltip("CloudV2Ray - 基于云计算的 V2Ray 客户端")
          .show_menu_on_left_click(false)
          .on_tray_icon_event(|ic, event| {
            use tauri::tray::TrayIconEvent;

            if let TrayIconEvent::Click { button, .. } = &event {
              use tauri::tray::MouseButton;

              if matches!(button, MouseButton::Left) {
                open_main_window(ic.app_handle());
              }
            }
          })
          .on_menu_event(|app, event| match event.id.as_ref() {
            "quit" => {
              block_on(stop_v2ray_server(app.state()));
              app.exit(0);
            }
            _ => {
              println!("menu item {:?} not handled", event.id);
            }
          })
          .build(_app)
          .unwrap();
        _app.manage(NotifyWindow::<Wry>::new());
      }
      _app.manage(Ping::new());

      Ok(())
    })
    // .on_window_event(|window, event| match event {
    //   WindowEvent::CloseRequested { api, .. } => {
    //     // api.prevent_close();
    //     // window.hide().unwrap();
    //     // TODO: 如果正在创建实例，不允许关闭。
    //   }
    //   _ => {} // event.window().hide().unwrap();
    // })
    .build(tauri::generate_context!())
    .expect("error while running tauri application")
    .run(|_app, event| {
      match event {
        #[cfg(desktop)]
        tauri::RunEvent::ExitRequested { api, code, .. } => {
          if code.is_none() {
            api.prevent_exit();
          } else {
            //
          }
        }
        #[cfg(target_os = "macos")]
        tauri::RunEvent::Reopen { .. } => {
          open_main_window(_app);
        }
        _ => {
          // println!("event: {:?}", event);
        }
      }
    });
}
