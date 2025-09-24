#[cfg(desktop)]
mod notify;
#[cfg(desktop)]
mod proxy;
#[cfg(target_os = "android")]
mod vpn;

mod ping;
mod tencent;
mod util;
use log::info;
#[cfg(desktop)]
use proxy::{tauri_start_proxy_client, tauri_stop_proxy_client};

use tauri::{AppHandle, Runtime, Wry};
use tauri::{Manager, WebviewWindowBuilder};

#[cfg(desktop)]
use tauri::{
  menu::{Menu, MenuItem},
  tray::TrayIconBuilder,
};

#[cfg(desktop)]
use notify::*;

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
  #[cfg(desktop)]
  {
    use env_logger::init;
    use std::env::set_var;
    set_var("RUST_LOG", "info");
    init();
  }

  #[cfg(target_os = "ios")]
  {
    use log::LevelFilter;
    use oslog::OsLogger;

    OsLogger::new("com.cloudtun.app")
      .level_filter(LevelFilter::Info)
      // .category_level_filter("Settings", LevelFilter::Trace)
      .init()
      .unwrap();
  }

  let mut builder = tauri::Builder::default()
    .plugin(tauri_plugin_os::init())
    .plugin(tauri_plugin_store::Builder::new().build())
    .plugin(tauri_plugin_http::init());

  println!("xxx cloudtun startup 111");
  info!("xxx cloudtun startup 222");

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
        tauri_base64_covert,
        tauri_start_proxy_client,
        tauri_stop_proxy_client,
        tauri_show_notify_window,
        tauri_close_notify_window,
        tauri_interval_ping_start,
        tauri_interval_ping_stop
      ]);
  }

  #[cfg(target_os = "android")]
  {
    use crate::vpn::init_tauri_vpn;
    use crate::vpn::tauri_start_vpn;

    builder = builder
      .plugin(init_tauri_vpn())
      .invoke_handler(tauri::generate_handler![
        tauri_generate_uuid,
        tauri_calc_tencent_cloud_api_signature,
        tauri_start_vpn
      ]);
  }

  #[cfg(target_os = "ios")]
  {
    use tauri_plugin_ios::init;

    builder = builder
      .plugin(init())
      .invoke_handler(tauri::generate_handler![
        tauri_generate_uuid,
        tauri_calc_tencent_cloud_api_signature,
      ]);
  }

  builder
    .setup(|_app| {
      #[cfg(desktop)]
      {
        use crate::proxy::ProxyLoop;

        _app.manage(ProxyLoop::new());

        let quit_i = MenuItem::with_id(_app, "quit", "退出CloudTun", true, None::<&str>).unwrap();
        let menu = Menu::with_items(_app, &[&quit_i]).unwrap();
        let _ = TrayIconBuilder::new()
          .icon(_app.default_window_icon().unwrap().clone())
          .menu(&menu)
          .tooltip("CloudTun - 基于云计算的网络代理方案")
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
              use crate::proxy::stop_proxy_client;
              use tauri::async_runtime::block_on;
              block_on(stop_proxy_client(app.state()));
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
