use crate::{Result, error::Error};
use tproxy_config::IpCidr;

use std::str::FromStr;

pub struct Args {
  pub tun_fd: Option<i32>,

  /// IP address pool to be used by virtual DNS in CIDR notation.
  pub virtual_dns_pool: IpCidr,

  pub tcp_timeout: u64,

  pub udp_timeout: u64,

  pub max_sessions: usize,
}

fn validate_tun(p: &str) -> Result<String> {
  #[cfg(target_os = "macos")]
  if p.len() <= 4 || &p[..4] != "utun" {
    return Err(Error::from("Invalid tun interface name, please use utunX"));
  }
  Ok(p.to_string())
}

impl Default for Args {
  fn default() -> Self {
    Args {
      #[cfg(unix)]
      tun_fd: None,

      tcp_timeout: 600,
      udp_timeout: 10,
      virtual_dns_pool: IpCidr::from_str("198.18.0.0/15").unwrap(),

      max_sessions: 200,
    }
  }
}
