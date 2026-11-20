#[cfg(desktop)]
mod debug_logs;
#[cfg(not(target_os = "ios"))]
mod geodata;
mod logs;
use logs::{tauri_append_log, tauri_get_logs};
#[cfg(desktop)]
use logs::{tauri_debug_log_status, tauri_export_debug_logs, tauri_set_debug_logging};
#[cfg(target_os = "android")]
mod android;
mod cloud_access;
#[cfg(all(desktop, debug_assertions))]
mod dev_bridge;
#[cfg(desktop)]
mod notify;
#[cfg(desktop)]
mod ping;
#[cfg(not(target_os = "ios"))]
mod proxy;
#[cfg(not(target_os = "ios"))]
mod proxy_config;
#[cfg(not(target_os = "ios"))]
mod system_proxy;
use cloud_access::*;
mod tencent;
mod util;

use log::info;
#[cfg(not(target_os = "ios"))]
use proxy::*;
#[cfg(desktop)]
use tauri::{
  menu::{Menu, MenuItem},
  tray::TrayIconBuilder,
  AppHandle, Manager, Runtime, WebviewWindowBuilder, Wry,
};

#[cfg(desktop)]
use notify::*;
#[cfg(desktop)]
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
    .min_inner_size(
      cfg.min_width.unwrap_or(320.0),
      cfg.min_height.unwrap_or(480.0),
    )
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

  #[cfg(target_os = "android")]
  android_logger::init_once(
    android_logger::Config::default()
      .with_tag("cloudtun")
      .with_max_level(log::LevelFilter::Info),
  );

  let mut builder = tauri::Builder::default()
    .plugin(tauri_plugin_os::init())
    .plugin(tauri_plugin_clipboard_manager::init())
    .plugin(tauri_plugin_store::Builder::new().build())
    .plugin(tauri_plugin_http::init());

  info!("xxx cloudtun startup");

  #[cfg(desktop)]
  {
    builder = builder
      .plugin(tauri_plugin_autostart::init(
        tauri_plugin_autostart::MacosLauncher::LaunchAgent,
        None,
      ))
      .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
        let win = app.get_webview_window("main").expect("no main window");
        win.show().unwrap();
        let _ = win.set_focus();
      }))
      .invoke_handler(tauri::generate_handler![
        tauri_debug_log_status,
        tauri_set_debug_logging,
        tauri_export_debug_logs,
        tauri_get_logs,
        tauri_append_log,
        tauri_generate_uuid,
        tauri_new_cloud_access_token,
        tauri_ping_cloud_server,
        tauri_has_cloud_access,
        tauri_remember_cloud_access,
        tauri_exit_process,
        tauri_call_tencent_api,
        tauri_open_devtools,
        tauri_calc_tencent_cloud_api_signature,
        tauri_base64_covert,
        tauri_start_proxy_client,
        tauri_stop_proxy_client,
        tauri_proxy_status,
        tauri_proxy_traffic,
        tauri_geo_status,
        tauri_update_geo,
        tauri_get_proxy_config,
        tauri_set_proxy_config,
        tauri_get_subscription,
        tauri_update_subscription,
        tauri_ping_subscription_nodes,
        tauri_check_subscription_node,
        tauri_connect_subscription,
        tauri_set_system_proxy,
        tauri_set_routing_mode,
        tauri_show_notify_window,
        tauri_close_notify_window,
        tauri_interval_ping_start,
        tauri_interval_ping_stop
      ]);
  }

  #[cfg(target_os = "android")]
  {
    use crate::android::{
      tauri_android_get_vpn_connected, tauri_android_init_plugin,
      tauri_android_request_notification_permission, tauri_android_start_vpn,
      tauri_android_stop_vpn,
    };

    builder = builder
      .plugin(tauri_android_init_plugin())
      .invoke_handler(tauri::generate_handler![
        tauri_get_logs,
        tauri_append_log,
        tauri_generate_uuid,
        tauri_new_cloud_access_token,
        tauri_ping_cloud_server,
        tauri_has_cloud_access,
        tauri_remember_cloud_access,
        tauri_base64_covert,
        tauri_android_request_notification_permission,
        tauri_call_tencent_api,
        tauri_calc_tencent_cloud_api_signature,
        tauri_android_start_vpn,
        tauri_android_stop_vpn,
        tauri_android_get_vpn_connected,
        tauri_android_set_failover,
        tauri_start_proxy_client,
        tauri_stop_proxy_client,
        tauri_proxy_status,
        tauri_proxy_traffic,
        tauri_geo_status,
        tauri_update_geo,
        tauri_get_proxy_config,
        tauri_set_proxy_config,
        tauri_get_subscription,
        tauri_update_subscription,
        tauri_ping_subscription_nodes,
        tauri_check_subscription_node,
        tauri_connect_subscription,
        tauri_set_routing_mode,
      ]);
  }

  #[cfg(target_os = "ios")]
  {
    use tauri_plugin_ios::init;

    builder = builder
      .plugin(init())
      .invoke_handler(tauri::generate_handler![
        tauri_get_logs,
        tauri_append_log,
        tauri_generate_uuid,
        tauri_new_cloud_access_token,
        tauri_ping_cloud_server,
        tauri_has_cloud_access,
        tauri_remember_cloud_access,
        tauri_base64_covert,
        tauri_calc_tencent_cloud_api_signature,
      ]);
  }

  builder
    .setup(|_app| {
      use tauri::Manager;
      #[cfg(desktop)]
      _app.manage(logs::Logs::with_debug_directory(
        _app.path().app_log_dir()?.join("debug"),
      )?);
      #[cfg(not(desktop))]
      _app.manage(logs::Logs::new());
      _app.manage(CloudAccess::new(_app.path().app_data_dir()?)?);
      crate::util::emit_log(_app.handle(), "log::info", "CloudTun 后台已启动");
      #[cfg(not(target_os = "ios"))]
      {
        use crate::proxy::ProxyLoop;

        let proxy_loop = ProxyLoop::new(_app.path().app_data_dir()?)?;
        proxy_loop.log_geo_status(_app.handle());
        _app.manage(proxy_loop);
      }
      #[cfg(desktop)]
      {
        let quit_i = MenuItem::with_id(_app, "quit", "退出CloudTun", true, None::<&str>).unwrap();
        let menu = Menu::with_items(_app, &[&quit_i]).unwrap();
        let _tray = TrayIconBuilder::new()
          .icon(_app.default_window_icon().unwrap().clone())
          .menu(&menu)
          .tooltip("CloudTun - 自用网络代理方案")
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
        #[cfg(target_os = "macos")]
        crate::proxy::update_tray_speed(_tray);
        _app.manage(NotifyWindow::<Wry>::new());
        _app.manage(Ping::new());
        #[cfg(debug_assertions)]
        crate::dev_bridge::start(_app.handle().clone());
      }

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
            tauri::async_runtime::block_on(crate::proxy::stop_proxy_client(_app.state()));
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
