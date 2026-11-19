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

/// Cloud control requests must not depend on the proxy they manage.
#[cfg(desktop)]
#[tauri::command]
pub async fn tauri_call_tencent_api<R: tauri::Runtime>(
  handle: tauri::AppHandle<R>,
  secret_id: String,
  secret_key: String,
  service: String,
  region: String,
  action: String,
  body: String,
) -> TAResult<serde_json::Value> {
  use crate::util::emit_log;
  use anyhow_tauri::IntoTAResult;
  let start = std::time::Instant::now();
  let result = async {
    let version = match service.as_str() {
      "cvm" => "2017-03-12",
      "tat" => "2020-10-28",
      "vpc" => "2017-03-12",
      "billing" => "2018-07-09",
      _ => anyhow::bail!("不支持的腾讯云服务"),
    };
    anyhow::ensure!(
      !action.is_empty()
        && action.len() <= 128
        && action.bytes().all(|b| b.is_ascii_alphanumeric()),
      "腾讯云操作无效"
    );
    anyhow::ensure!(body.len() <= 4 * 1024 * 1024, "腾讯云请求过大");
    let host = format!("{service}.tencentcloudapi.com");
    let timestamp = chrono::Utc::now().timestamp();
    let signature =
      tencent_cloud_api_signature(&secret_id, &secret_key, &host, &service, timestamp, &body);
    emit_log(&handle, "log::cloud", &format!("腾讯云 {action}：直连请求"));
    let client = reqwest::Client::builder()
      .no_proxy()
      .redirect(reqwest::redirect::Policy::none())
      .connect_timeout(std::time::Duration::from_secs(5))
      .timeout(std::time::Duration::from_secs(20))
      .build()?;
    let mut response = client
      .post(format!("https://{host}"))
      .header("Authorization", signature)
      .header("Content-Type", "application/json")
      .header("X-TC-Action", &action)
      .header("X-TC-Version", version)
      .header("X-TC-Timestamp", timestamp.to_string())
      .header("X-TC-Region", &region)
      .body(body)
      .send()
      .await?;
    anyhow::ensure!(
      response.status().is_success(),
      "腾讯云 HTTP {}",
      response.status().as_u16()
    );
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
      anyhow::ensure!(
        bytes.len() + chunk.len() <= 8 * 1024 * 1024,
        "腾讯云响应过大"
      );
      bytes.extend_from_slice(&chunk);
    }
    let value: serde_json::Value = serde_json::from_slice(&bytes)?;
    anyhow::ensure!(
      value
        .get("Response")
        .is_some_and(serde_json::Value::is_object),
      "腾讯云响应格式无效"
    );
    Ok::<_, anyhow::Error>(value)
  }
  .await;
  emit_log(
    &handle,
    "log::cloud",
    &format!(
      "腾讯云 {action}：{}，耗时 {} ms",
      if result.is_ok() {
        "收到响应"
      } else {
        "请求失败"
      },
      start.elapsed().as_millis()
    ),
  );
  result
    .map_err(|error| {
      if let Some(network) = error.downcast_ref::<reqwest::Error>() {
        if network.is_timeout() {
          anyhow::anyhow!("腾讯云 {action} 直连请求超时，请重试")
        } else {
          anyhow::anyhow!("腾讯云 {action} 直连请求失败，请检查网络")
        }
      } else {
        error
      }
    })
    .into_ta_result()
}
