use crate::china::ChinaRulesStore;
use crate::route::{MatchType, RouteMatcher};
use serde::{Deserialize, Serialize};
use std::{
  collections::HashMap,
  net::IpAddr,
  sync::{Arc, RwLock},
  time::{Duration, Instant},
};
use tokio::sync::Mutex;

#[derive(Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RoutingMode {
  Global,
  #[default]
  BypassChina,
}

#[derive(Clone)]
pub(crate) struct Routing {
  pub custom: Option<RouteMatcher>,
  mode: Arc<RwLock<RoutingMode>>,
  cache: Arc<Mutex<HashMap<String, (Instant, bool, u64)>>>,
  rules: Arc<ChinaRulesStore>,
}
impl Routing {
  pub fn mode(mode: RoutingMode) -> Self {
    Self::with_rules(mode, Arc::default())
  }
  pub fn with_rules(mode: RoutingMode, rules: Arc<ChinaRulesStore>) -> Self {
    Self {
      rules,
      custom: None,
      mode: Arc::new(RwLock::new(mode)),
      cache: Arc::default(),
    }
  }
  pub fn custom(custom: RouteMatcher) -> Self {
    Self {
      custom: Some(custom),
      ..Self::mode(RoutingMode::Global)
    }
  }
  pub fn current_mode(&self) -> RoutingMode {
    *self.mode.read().unwrap_or_else(|e| e.into_inner())
  }
  pub fn rules(&self) -> Arc<ChinaRulesStore> {
    self.rules.clone()
  }
  pub fn set_mode(&self, mode: RoutingMode) {
    *self.mode.write().unwrap_or_else(|e| e.into_inner()) = mode;
  }
  pub async fn decide(&self, host: &str, port: u16) -> MatchType {
    if let Some(custom) = &self.custom {
      return custom.match_domain(host).await;
    }
    if is_local_host(host) {
      return MatchType::Direct;
    }
    if *self.mode.read().unwrap_or_else(|e| e.into_inner()) == RoutingMode::Global {
      return MatchType::Proxy;
    }
    let (version, rules) = self.rules.snapshot();
    let Some(rules) = rules else {
      return MatchType::Proxy;
    };
    if rules.contains_domain(host) {
      return MatchType::Direct;
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
      return if rules.contains_ip(ip) {
        MatchType::Direct
      } else {
        MatchType::Proxy
      };
    }
    let key = host.to_ascii_lowercase();
    if let Some((at, direct, cached_version)) = self.cache.lock().await.get(&key) {
      if *cached_version == version && at.elapsed() < Duration::from_secs(300) {
        return if *direct {
          MatchType::Direct
        } else {
          MatchType::Proxy
        };
      }
    }
    let direct = match tokio::time::timeout(
      Duration::from_secs(2),
      tokio::net::lookup_host((host, port)),
    )
    .await
    {
      Ok(Ok(mut addresses)) => {
        addresses.any(|address| is_local_ip(address.ip()) || rules.contains_ip(address.ip()))
      }
      _ => false,
    };
    let mut cache = self.cache.lock().await;
    if cache.len() >= 4096 {
      cache.clear();
    }
    cache.insert(key, (Instant::now(), direct, version));
    if direct {
      MatchType::Direct
    } else {
      MatchType::Proxy
    }
  }
}

pub fn is_local_ip(ip: IpAddr) -> bool {
  match ip {
    IpAddr::V4(ip) => {
      ip.is_private() || ip.is_loopback() || ip.is_link_local() || ip.is_unspecified()
    }
    IpAddr::V6(ip) => {
      ip.is_loopback()
        || ip.is_unique_local()
        || ip.is_unicast_link_local()
        || ip.is_unspecified()
        || ip
          .to_ipv4_mapped()
          .is_some_and(|ip| is_local_ip(IpAddr::V4(ip)))
    }
  }
}
fn is_local_host(host: &str) -> bool {
  let host = host.trim_end_matches('.').to_ascii_lowercase();
  host.eq_ignore_ascii_case("localhost")
    || host.ends_with(".localhost")
    || host.ends_with(".local")
    || !host.contains('.') && !host.contains(':')
    || host.parse::<IpAddr>().is_ok_and(is_local_ip)
}
#[cfg(test)]
mod tests {
  use super::*;
  use crate::china::ChinaRules;
  #[tokio::test]
  async fn bypass_is_inactive_until_rules_arrive_and_updates_live() {
    let store = Arc::new(ChinaRulesStore::default());
    let routing = Routing::with_rules(RoutingMode::BypassChina, store.clone());
    assert!(matches!(
      routing.decide("127.0.0.1", 80).await,
      MatchType::Direct
    ));
    assert!(matches!(
      routing.decide("example.cn", 80).await,
      MatchType::Proxy
    ));
    assert!(matches!(
      routing.decide("114.114.114.114", 80).await,
      MatchType::Proxy
    ));
    store.replace(
      ChinaRules::parse("114.114.114.0/24", "240e::/16", "server=/baidu.com/1.1.1.1").unwrap(),
    );
    assert!(matches!(
      routing.decide("baidu.com", 80).await,
      MatchType::Direct
    ));
    assert!(matches!(
      routing.decide("example.cn", 80).await,
      MatchType::Direct
    ));
    assert!(matches!(
      routing.decide("114.114.114.114", 80).await,
      MatchType::Direct
    ));
    assert!(matches!(
      routing.decide("1.1.1.1", 80).await,
      MatchType::Proxy
    ));
    let global = Routing::with_rules(RoutingMode::Global, store);
    assert!(matches!(
      global.decide("baidu.com", 80).await,
      MatchType::Proxy
    ));
    assert!(is_local_host("printer.LOCAL"));
  }
}
