use http::{HeaderMap, HeaderValue, StatusCode};

pub async fn ping_cloudtun_proxy_server(ip: &str, token: &str) -> bool {
  let req = reqwest::Client::new();
  let mut headers = HeaderMap::new();
  headers.insert("x-token", HeaderValue::from_str(token).unwrap());

  let x = req
    .get(format!("http://{}:24816/ping", ip))
    .headers(headers)
    .send()
    .await;
  // let x = get(format!("http://{}:2081/ping", ip, token)).await;
  let Ok(resp) = x else {
    return false;
  };
  if resp.status() != StatusCode::OK {
    return false;
  };
  let Ok(txt) = resp.text().await else {
    return false;
  };
  return txt.eq("pong!");
}
