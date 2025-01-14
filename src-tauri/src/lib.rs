mod tencent;
mod util;

use tencent::*;
use util::*;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .plugin(tauri_plugin_autostart::init(
      tauri_plugin_autostart::MacosLauncher::LaunchAgent,
      None,
    ))
    .plugin(tauri_plugin_clipboard_manager::init())
    .plugin(tauri_plugin_os::init())
    .plugin(tauri_plugin_single_instance::init(|_app, _args, _cwd| {
      // Write your code here...
    }))
    .plugin(tauri_plugin_notification::init())
    .plugin(tauri_plugin_store::Builder::new().build())
    .plugin(tauri_plugin_http::init())
    .invoke_handler(tauri::generate_handler![
      tauri_generate_uuid,
      tauri_exit_process,
      tauri_open_devtools,
      tauri_calc_tencent_cloud_api_signature
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
