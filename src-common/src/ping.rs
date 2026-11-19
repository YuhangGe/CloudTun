use http::{HeaderValue, StatusCode};
use std::{sync::OnceLock, time::Duration};
pub async fn ping_cloudtun_proxy_server(ip: &str, token: &str) -> bool {
  ping_cloudtun_proxy_server_at(ip, 24816, token).await
}
pub async fn ping_cloudtun_proxy_server_at(ip: &str, port: u16, token: &str) -> bool {
  static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
  let client = CLIENT.get_or_init(|| reqwest::Client::new());
  let Ok(token) = HeaderValue::from_str(token) else {
    return false;
  };
  let host = if ip.contains(':') && !ip.starts_with('[') {
    format!("[{ip}]")
  } else {
    ip.to_owned()
  };
  let result = tokio::time::timeout(Duration::from_secs(10), async {
    let response = client
      .get(format!("http://{host}:{port}/ping"))
      .header("x-token", token)
      .send()
      .await
      .ok()?;
    if response.status() != StatusCode::OK {
      return None;
    }
    Some(response.text().await.ok()?.eq("pong!"))
  })
  .await;
  matches!(result, Ok(Some(true)))
}
