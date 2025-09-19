use anyhow_tauri::TAResult;
use cloudtun_common::tencent::tencent_cloud_api_signature;

#[tauri::command]
pub fn tauri_calc_tencent_cloud_api_signature(
  secret_id: &str,
  secret_key: &str,
  service_host: &str,
  service_name: &str,
  timestamp: i64,
  body: &str,
) -> TAResult<String> {
  Ok(tencent_cloud_api_signature(
    secret_id,
    secret_key,
    service_host,
    service_name,
    timestamp,
    body,
  ))
}
