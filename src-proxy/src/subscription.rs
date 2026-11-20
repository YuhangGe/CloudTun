use base64::{
  Engine,
  engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};
use tokio::time::timeout;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionNode {
  pub id: String,
  pub name: String,
  pub protocol: String,
  pub address: String,
  pub port: u16,
  pub unsupported_reason: Option<String>,
  #[serde(default)]
  pub vmess: Option<VmessConfig>,
}

// Kept in the backend; node credentials are never sent to the node-list UI.
#[derive(Clone, Serialize, Deserialize)]
pub struct VmessConfig {
  pub uuid: String,
  pub cipher: String,
  pub network: String,
  pub tls: bool,
  pub host: String,
  pub path: String,
  pub server_name: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeLatency {
  pub id: String,
  pub latency_ms: Option<u64>,
}

fn decode(value: &str) -> anyhow::Result<Vec<u8>> {
  let compact: String = value.chars().filter(|c| !c.is_whitespace()).collect();
  for engine in [&STANDARD, &STANDARD_NO_PAD, &URL_SAFE, &URL_SAFE_NO_PAD] {
    if let Ok(bytes) = engine.decode(compact.as_bytes()) {
      return Ok(bytes);
    }
  }
  anyhow::bail!("订阅 Base64 编码无效")
}

pub fn parse_subscription(body: &str) -> anyhow::Result<Vec<SubscriptionNode>> {
  let body = body.trim_start_matches('\u{feff}').trim();
  let decoded;
  let text = if body.contains("://") {
    body
  } else {
    decoded =
      String::from_utf8(decode(body)?).map_err(|_| anyhow::anyhow!("订阅内容不是 UTF-8 文本"))?;
    decoded.as_str()
  };
  let mut nodes = Vec::new();
  let mut seen = std::collections::HashSet::new();
  for line in text
    .lines()
    .map(str::trim)
    .filter(|s| !s.is_empty() && !s.starts_with('#'))
  {
    let Some((protocol, payload)) = line.split_once("://") else {
      continue;
    };
    let id = Sha256::digest(line.as_bytes())
      .iter()
      .map(|byte| format!("{byte:02x}"))
      .collect::<String>();
    if !seen.insert(id.clone()) {
      continue;
    }
    let mut node = SubscriptionNode {
      id,
      name: String::new(),
      protocol: protocol.to_ascii_lowercase(),
      address: String::new(),
      port: 0,
      unsupported_reason: None,
      vmess: None,
    };
    if node.protocol == "vmess" {
      match parse_vmess(payload) {
        Ok((name, address, port, config, reason)) => {
          node.name = name;
          node.address = address;
          node.port = port;
          node.vmess = Some(config);
          node.unsupported_reason = reason;
        }
        Err(_) => {
          node.name = "无效 VMess 节点".into();
          node.unsupported_reason = Some("VMess 配置无效".into());
        }
      }
    } else {
      node.unsupported_reason = Some(format!("暂不支持 {} 协议", node.protocol.to_uppercase()));
      if let Ok(uri) = url::Url::parse(line) {
        node.address = uri.host_str().unwrap_or_default().into();
        node.port = uri.port().unwrap_or_default();
        node.name = uri
          .fragment()
          .map(|s| {
            percent_encoding::percent_decode_str(s)
              .decode_utf8_lossy()
              .into_owned()
          })
          .unwrap_or_default();
      }
    }
    if node.name.is_empty() {
      node.name = if node.address.is_empty() {
        node.protocol.to_uppercase()
      } else {
        node.address.clone()
      };
    }
    nodes.push(node);
    anyhow::ensure!(nodes.len() <= 2000, "订阅节点超过 2000 条");
  }
  anyhow::ensure!(
    !nodes.is_empty(),
    "没有找到节点；当前支持 Base64 或明文链接列表订阅"
  );
  Ok(nodes)
}

fn field(value: &serde_json::Value, key: &str) -> String {
  value
    .get(key)
    .map(|v| {
      if let Some(s) = v.as_str() {
        s.to_owned()
      } else {
        v.to_string()
      }
    })
    .unwrap_or_default()
}

fn parse_vmess(
  payload: &str,
) -> anyhow::Result<(String, String, u16, VmessConfig, Option<String>)> {
  let data: serde_json::Value = serde_json::from_slice(&decode(payload)?)?;
  let address = field(&data, "add");
  let port = field(&data, "port").parse::<u16>()?;
  anyhow::ensure!(
    !address.is_empty() && !address.chars().any(char::is_whitespace) && port != 0,
    "invalid address"
  );
  let uuid = field(&data, "id");
  uuid::Uuid::parse_str(&uuid)?;
  let network = match field(&data, "net").as_str() {
    "" => "tcp".to_string(),
    v => v.to_ascii_lowercase(),
  };
  let cipher = match field(&data, "scy").as_str() {
    "" | "auto" => "aes-128-gcm".to_string(),
    v => v.to_ascii_lowercase(),
  };
  let tls_field = field(&data, "tls");
  let tls = tls_field == "tls";
  let host = field(&data, "host");
  let sni = field(&data, "sni");
  let reason = if !matches!(field(&data, "aid").as_str(), "" | "0") {
    Some("暂不支持旧版 alterId VMess".into())
  } else if !matches!(network.as_str(), "tcp" | "ws") {
    Some(format!("暂不支持 {network} 传输"))
  } else if !matches!(
    cipher.as_str(),
    "aes-128-gcm" | "chacha20-poly1305" | "chacha20-ietf-poly1305"
  ) {
    Some(format!("暂不支持 {cipher} 加密"))
  } else if !matches!(tls_field.as_str(), "" | "tls" | "none") {
    Some("暂不支持此 TLS 配置".into())
  } else if !matches!(field(&data, "type").as_str(), "" | "none") {
    Some("暂不支持此 TCP 伪装".into())
  } else {
    None
  };
  let server_name = if !sni.is_empty() {
    sni
  } else if !host.is_empty() {
    host.clone()
  } else {
    address.clone()
  };
  let path = match field(&data, "path").as_str() {
    "" => "/".into(),
    v => v.to_string(),
  };
  Ok((
    field(&data, "ps"),
    address,
    port,
    VmessConfig {
      uuid,
      cipher,
      network,
      tls,
      host,
      path,
      server_name,
    },
    reason,
  ))
}

pub async fn fetch_subscription(url: &str) -> anyhow::Result<Vec<SubscriptionNode>> {
  let uri = url::Url::parse(url).map_err(|_| anyhow::anyhow!("请输入有效的订阅链接"))?;
  anyhow::ensure!(
    matches!(uri.scheme(), "http" | "https"),
    "订阅链接必须是 HTTP 或 HTTPS"
  );
  let client = reqwest::Client::builder()
    .no_proxy()
    .connect_timeout(Duration::from_secs(10))
    .timeout(Duration::from_secs(30))
    .build()?;
  // Never return reqwest errors containing the subscription URL/token to logs or UI.
  let response = client
    .get(uri)
    .header("User-Agent", "v2rayN/CloudTun")
    .send()
    .await
    .map_err(|_| anyhow::anyhow!("订阅请求失败，请检查链接、网络或 HTTPS 证书"))?;
  anyhow::ensure!(
    response.status().is_success(),
    "订阅服务器返回 HTTP {}",
    response.status().as_u16()
  );
  let mut response = response;
  let mut body = Vec::new();
  while let Some(chunk) = response
    .chunk()
    .await
    .map_err(|_| anyhow::anyhow!("读取订阅内容失败"))?
  {
    anyhow::ensure!(
      body.len() + chunk.len() <= 5 * 1024 * 1024,
      "订阅内容超过 5 MB"
    );
    body.extend_from_slice(&chunk);
  }
  parse_subscription(std::str::from_utf8(&body).map_err(|_| anyhow::anyhow!("订阅内容编码无效"))?)
}

pub async fn ping_node(node: &SubscriptionNode) -> NodeLatency {
  let mut samples = Vec::new();
  if node.unsupported_reason.is_none() {
    let addresses = match timeout(
      Duration::from_secs(3),
      tokio::net::lookup_host((node.address.as_str(), node.port)),
    )
    .await
    {
      Ok(Ok(addresses)) => addresses.collect::<Vec<_>>(),
      _ => {
        return NodeLatency {
          id: node.id.clone(),
          latency_ms: None,
        };
      }
    };
    for _ in 0..3 {
      let start = Instant::now();
      if let Ok(Ok(_)) = timeout(
        Duration::from_millis(800),
        crate::tcp::connect_addresses(&addresses),
      )
      .await
      {
        samples.push(start.elapsed().as_millis() as u64);
      }
    }
  }
  samples.sort_unstable();
  NodeLatency {
    id: node.id.clone(),
    latency_ms: if samples.len() >= 2 {
      Some(samples[samples.len() / 2])
    } else {
      None
    },
  }
}

pub async fn ping_nodes(nodes: &[SubscriptionNode]) -> Vec<NodeLatency> {
  use futures_util::{StreamExt, stream};
  stream::iter(nodes.iter().map(ping_node))
    .buffer_unordered(6)
    .collect()
    .await
}

#[cfg(test)]
mod tests {
  use super::*;
  fn vmess(overrides: serde_json::Value) -> String {
    let mut data = serde_json::json!({"ps":"日本测试", "add":"example.net", "port":"443", "id":"00000000-0000-4000-8000-000000000001", "net":"ws", "host":"example.net", "path":"/proxy", "aid":"0", "scy":"auto", "tls":"tls"});
    for (key, value) in overrides.as_object().unwrap() {
      data[key] = value.clone();
    }
    format!(
      "vmess://{}",
      STANDARD_NO_PAD.encode(serde_json::to_vec(&data).unwrap())
    )
  }
  #[test]
  fn parses_base64_subscription_and_deduplicates() {
    let uri = vmess(serde_json::json!({}));
    let body = URL_SAFE_NO_PAD.encode(format!("{uri}\n{uri}\n"));
    let nodes = parse_subscription(&body).unwrap();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].name, "日本测试");
    assert_eq!(nodes[0].port, 443);
    let cfg = nodes[0].vmess.as_ref().unwrap();
    assert!(cfg.tls);
    assert_eq!(cfg.cipher, "aes-128-gcm");
    assert!(nodes[0].unsupported_reason.is_none());
  }
  #[test]
  fn preserves_unsupported_and_invalid_nodes_without_losing_valid_nodes() {
    let valid = vmess(serde_json::json!({"port":80,"tls":""}));
    let body = format!("{valid}\ntrojan://password@example.net:443#Other%20node\nvmess://invalid");
    let nodes = parse_subscription(&body).unwrap();
    assert_eq!(nodes.len(), 3);
    assert!(nodes[0].unsupported_reason.is_none());
    assert_eq!(nodes[1].name, "Other node");
    assert!(nodes[1].unsupported_reason.is_some());
    assert!(nodes[2].vmess.is_none());
  }
  #[test]
  fn rejects_invalid_uuid_and_marks_unsupported_transports() {
    for override_ in [
      serde_json::json!({"id":"invalid"}),
      serde_json::json!({"aid":64}),
      serde_json::json!({"net":"grpc"}),
      serde_json::json!({"port":0}),
    ] {
      let nodes = parse_subscription(&vmess(override_)).unwrap();
      assert!(nodes[0].unsupported_reason.is_some());
    }
    assert!(parse_subscription("<html>error</html>").is_err());
    assert!(parse_subscription("").is_err());
  }
}
