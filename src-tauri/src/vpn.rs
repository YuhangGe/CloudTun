use anyhow_tauri::TAResult;
use tauri::{AppHandle, Runtime};
use tauri_plugin_vpn::PingRequest;
use tauri_plugin_vpn::VpnExt;

#[tauri::command]
pub async fn tauri_start_vpn<R: Runtime>(h: AppHandle<R>) -> TAResult<Option<String>> {
  let ret = h.vpn().start_vpn(PingRequest {
    value: Some("hello".into()),
  }).unwrap();
  Ok(ret.value)
}
