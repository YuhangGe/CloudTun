use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ProxyConfig {
  pub listen_host: String,
  pub port: u16,
}
impl Default for ProxyConfig {
  fn default() -> Self {
    Self {
      listen_host: "0.0.0.0".into(),
      port: 7893,
    }
  }
}
impl ProxyConfig {
  pub fn validate(&self) -> anyhow::Result<()> {
    anyhow::ensure!(
      matches!(self.listen_host.as_str(), "0.0.0.0" | "127.0.0.1"),
      "监听地址请选择 0.0.0.0 或 127.0.0.1"
    );
    anyhow::ensure!(self.port != 0, "端口必须在 1–65535 之间");
    Ok(())
  }
  pub fn view(&self) -> ProxyConfigView {
    let mut addresses = vec![ProxyAddress {
      label: "本机".into(),
      url: format!("http://127.0.0.1:{}", self.port),
    }];
    if self.listen_host == "0.0.0.0" {
      let mut seen = std::collections::HashSet::new();
      for interface in if_addrs::get_if_addrs().unwrap_or_default() {
        if interface.is_loopback()
          || ["utun", "tun", "tap", "docker", "veth"]
            .iter()
            .any(|prefix| interface.name.starts_with(prefix))
        {
          continue;
        }
        if let std::net::IpAddr::V4(ip) = interface.ip() {
          if ip.is_private() && seen.insert(ip) {
            addresses.push(ProxyAddress {
              label: format!("局域网 · {}", interface.name),
              url: format!("http://{ip}:{}", self.port),
            });
          }
        }
      }
    }
    ProxyConfigView {
      config: self.clone(),
      addresses,
    }
  }
}
#[derive(Serialize)]
pub struct ProxyAddress {
  label: String,
  url: String,
}
#[derive(Serialize)]
pub struct ProxyConfigView {
  pub config: ProxyConfig,
  pub addresses: Vec<ProxyAddress>,
}

#[cfg(test)]
mod tests {
  use super::*;
  #[test]
  fn defaults_and_validation() {
    let config: ProxyConfig = serde_json::from_str("{}").unwrap();
    assert_eq!(config.port, 7893);
    assert_eq!(config.listen_host, "0.0.0.0");
    assert!(config.validate().is_ok());
    assert!(
      ProxyConfig {
        port: 0,
        ..config.clone()
      }
      .validate()
      .is_err()
    );
    assert!(
      ProxyConfig {
        listen_host: "example.com".into(),
        ..config
      }
      .validate()
      .is_err()
    );
  }
  #[test]
  fn local_only_does_not_advertise_lan_addresses() {
    let view = ProxyConfig {
      listen_host: "127.0.0.1".into(),
      port: 9000,
    }
    .view();
    assert_eq!(view.addresses.len(), 1);
    assert_eq!(view.addresses[0].url, "http://127.0.0.1:9000");
  }
}
