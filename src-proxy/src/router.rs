use clash_rules::ClashRuleMatcher;
use std::collections::HashMap;

use geosite_rs::{decode_geoip, decode_geosite, geoip_to_hashmap, geosite_to_hashmap};

static GEOSITE_DATA: &[u8] = include_bytes!("data/geosite.dat");
static GEOIP_DATA: &[u8] = include_bytes!("data/geoip.dat");

pub struct Router {
  site_matcher: ClashRuleMatcher,
  ip_matcher: ClashRuleMatcher,
}

impl Router {
  pub fn new() -> Self {
    let t = HashMap::new();
    let site_matcher = geosite_to_hashmap(&decode_geosite(GEOSITE_DATA).unwrap(), t);
    let ip_matcher = geoip_to_hashmap(&decode_geoip(GEOIP_DATA).unwrap(), HashMap::new());
    Router {
      site_matcher: ClashRuleMatcher::from_hashmap(site_matcher).unwrap(),
      ip_matcher: ClashRuleMatcher::from_hashmap(ip_matcher).unwrap(),
    }
  }

  pub fn match_site(&self, domain: &str) -> bool {
    self
      .site_matcher
      .check_domain(&domain.to_ascii_lowercase())
      .is_none()
  }
}
