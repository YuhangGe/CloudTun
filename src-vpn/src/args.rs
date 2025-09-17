use crate::{Result, error::Error};
use tproxy_config::IpCidr;

use std::net::{IpAddr};
use std::str::FromStr;

pub struct Args {
  pub tun_fd: Option<i32>,

  /// Set whether to close the received raw file descriptor on drop or not.
  /// This setting is dependent on [tun_fd].
  #[cfg(unix)]
  pub close_fd_on_drop: Option<bool>,

  /// IPv6 enabled
  pub ipv6_enabled: bool,

  /// Routing and system setup, which decides whether to setup the routing and system configuration.
  /// This option requires root-like privileges on every platform.
  /// It is very important on Linux, see `capabilities(7)`.
  pub setup: bool,

  /// DNS handling strategy
  pub dns: ArgDns,

  /// DNS resolver address
  pub dns_addr: IpAddr,

  /// IP address pool to be used by virtual DNS in CIDR notation.
  pub virtual_dns_pool: IpCidr,

  /// IPs used in routing setup which should bypass the tunnel,
  /// in the form of IP or IP/CIDR. Multiple IPs can be specified,
  /// e.g. --bypass 3.4.5.0/24 --bypass 5.6.7.8
  pub bypass: Vec<IpCidr>,

  pub tcp_timeout: u64,

  pub udp_timeout: u64,

  pub daemonize: bool,

  pub exit_on_fatal_error: bool,

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
    #[cfg(target_os = "linux")]
    let setup = false;
    #[cfg(not(target_os = "linux"))]
    let setup = true;
    Args {
      #[cfg(unix)]
      tun_fd: None,
      #[cfg(unix)]
      close_fd_on_drop: None,
      ipv6_enabled: false,
      setup,
      dns: ArgDns::Virtual,
      dns_addr: "8.8.8.8".parse().unwrap(),
      bypass: vec![],
      tcp_timeout: 600,
      udp_timeout: 10,
      virtual_dns_pool: IpCidr::from_str("198.18.0.0/15").unwrap(),
      daemonize: false,
      exit_on_fatal_error: false,
      max_sessions: 200,
    }
  }
}

impl Args {
  
  pub fn dns(&mut self, dns: ArgDns) -> &mut Self {
    self.dns = dns;
    self
  }

  #[cfg(unix)]
  pub fn tun_fd(&mut self, tun_fd: Option<i32>) -> &mut Self {
    self.tun_fd = tun_fd;
    self
  }

  #[cfg(unix)]
  pub fn close_fd_on_drop(&mut self, close_fd_on_drop: bool) -> &mut Self {
    self.close_fd_on_drop = Some(close_fd_on_drop);
    self
  }

  pub fn dns_addr(&mut self, dns_addr: IpAddr) -> &mut Self {
    self.dns_addr = dns_addr;
    self
  }

  pub fn bypass(&mut self, bypass: IpCidr) -> &mut Self {
    self.bypass.push(bypass);
    self
  }

  pub fn ipv6_enabled(&mut self, ipv6_enabled: bool) -> &mut Self {
    self.ipv6_enabled = ipv6_enabled;
    self
  }

  pub fn setup(&mut self, setup: bool) -> &mut Self {
    self.setup = setup;
    self
  }
}

/// DNS query handling strategy
/// - Virtual: Use a virtual DNS server to handle DNS queries, also known as Fake-IP mode
/// - OverTcp: Use TCP to send DNS queries to the DNS server
/// - Direct: Do not handle DNS by relying on DNS server bypassing

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ArgDns {
  Virtual = 0,
  OverTcp,
  Direct,
}
