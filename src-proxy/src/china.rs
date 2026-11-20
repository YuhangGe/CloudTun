use std::{
  collections::HashSet,
  net::IpAddr,
  sync::{Arc, RwLock},
};

/// Immutable mainland-only rules, shared by all proxy connections.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct ChinaRules {
  ipv4: Box<[(u32, u32)]>,
  ipv6: Box<[(u128, u128)]>,
  domains: HashSet<Box<str>>,
}
impl ChinaRules {
  pub fn parse(ipv4: &str, ipv6: &str, domains: &str) -> anyhow::Result<Self> {
    let v4 = parse_ranges(ipv4, false)?;
    let v6 = parse_ranges(ipv6, true)?;
    let ipv4 = merge(v4)
      .into_iter()
      .map(|(a, b)| (a as u32, b as u32))
      .collect();
    let ipv6 = merge(v6).into_boxed_slice();
    let mut names = HashSet::new();
    for line in domains
      .lines()
      .map(str::trim)
      .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
      let domain = line
        .strip_prefix("server=/")
        .and_then(|line| line.split('/').next())
        .ok_or_else(|| anyhow::anyhow!("大陆域名文件格式无效"))?;
      anyhow::ensure!(
        !domain.is_empty()
          && domain.len() <= 253
          && domain
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b".-_".contains(&c)),
        "大陆域名无效"
      );
      names.insert(domain.to_ascii_lowercase().into_boxed_str());
      anyhow::ensure!(names.len() <= 300_000, "大陆域名条目过多");
    }
    anyhow::ensure!(!names.is_empty(), "大陆域名列表为空");
    Ok(Self {
      ipv4,
      ipv6,
      domains: names,
    })
  }
  pub fn contains_ip(&self, ip: IpAddr) -> bool {
    match ip {
      IpAddr::V4(ip) => contains(&self.ipv4, u32::from(ip)),
      IpAddr::V6(ip) => match ip.to_ipv4_mapped() {
        Some(ip) => contains(&self.ipv4, u32::from(ip)),
        None => contains(&self.ipv6, u128::from(ip)),
      },
    }
  }
  pub fn contains_domain(&self, host: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host == "cn" || host.ends_with(".cn") {
      return true;
    }
    let mut suffix = host.as_str();
    loop {
      if self.domains.contains(suffix) {
        return true;
      }
      let Some((_, rest)) = suffix.split_once('.') else {
        return false;
      };
      suffix = rest;
    }
  }
  pub fn counts(&self) -> (usize, usize, usize) {
    (self.ipv4.len(), self.ipv6.len(), self.domains.len())
  }
}
fn parse_ranges(text: &str, ipv6: bool) -> anyhow::Result<Vec<(u128, u128)>> {
  let mut ranges = Vec::new();
  for line in text
    .lines()
    .map(str::trim)
    .filter(|line| !line.is_empty() && !line.starts_with('#'))
  {
    let network: ipnet::IpNet = line
      .parse()
      .map_err(|_| anyhow::anyhow!("大陆 IP 文件包含无效 CIDR"))?;
    match network {
      ipnet::IpNet::V4(net) if !ipv6 => ranges.push((
        u32::from(net.network()) as u128,
        u32::from(net.broadcast()) as u128,
      )),
      ipnet::IpNet::V6(net) if ipv6 => {
        ranges.push((u128::from(net.network()), u128::from(net.broadcast())))
      }
      _ => anyhow::bail!("大陆 IP 文件的地址族错误"),
    }
    anyhow::ensure!(ranges.len() <= 100_000, "大陆 IP 条目过多");
  }
  anyhow::ensure!(!ranges.is_empty(), "大陆 IP 列表为空");
  Ok(ranges)
}
fn merge(mut ranges: Vec<(u128, u128)>) -> Vec<(u128, u128)> {
  ranges.sort_unstable();
  let mut merged: Vec<(u128, u128)> = Vec::new();
  for (start, end) in ranges {
    if let Some(last) = merged.last_mut() {
      if start <= last.1 || last.1.checked_add(1) == Some(start) {
        last.1 = last.1.max(end);
        continue;
      }
    }
    merged.push((start, end));
  }
  merged
}
fn contains<T: Ord + Copy>(ranges: &[(T, T)], ip: T) -> bool {
  let index = ranges.partition_point(|(start, _)| *start <= ip);
  index > 0 && ip <= ranges[index - 1].1
}
#[derive(Default)]
pub struct ChinaRulesStore(RwLock<(u64, Option<Arc<ChinaRules>>)>);
impl ChinaRulesStore {
  pub fn snapshot(&self) -> (u64, Option<Arc<ChinaRules>>) {
    self.0.read().unwrap_or_else(|e| e.into_inner()).clone()
  }
  pub fn replace(&self, rules: ChinaRules) {
    let mut state = self.0.write().unwrap_or_else(|e| e.into_inner());
    state.0 += 1;
    state.1 = Some(Arc::new(rules));
  }
}
#[cfg(test)]
mod tests {
  use super::*;
  fn rules() -> ChinaRules {
    ChinaRules::parse(
      "1.0.0.128/25\n1.0.0.0/25\n1.0.0.1/32\n255.255.255.255/32",
      "2001:db8::/127\n2001:db8::2/127\nffff:ffff:ffff:ffff:ffff:ffff:ffff:ffff/128",
      "server=/example.com/114.114.114.114",
    )
    .unwrap()
  }
  #[test]
  fn merges_and_checks_inclusive_boundaries() {
    let rules = rules();
    assert_eq!(rules.counts(), (2, 2, 1));
    for ip in [
      "1.0.0.0",
      "1.0.0.255",
      "255.255.255.255",
      "2001:db8::",
      "2001:db8::3",
      "ffff:ffff:ffff:ffff:ffff:ffff:ffff:ffff",
      "::ffff:1.0.0.1",
    ] {
      assert!(rules.contains_ip(ip.parse().unwrap()), "{ip}");
    }
    for ip in ["0.255.255.255", "1.0.1.0", "2001:db8::4"] {
      assert!(!rules.contains_ip(ip.parse().unwrap()), "{ip}");
    }
    assert!(rules.contains_domain("WWW.EXAMPLE.COM."));
    assert!(!rules.contains_domain("notexample.com"));
  }
  #[test]
  fn zero_prefix_and_corrupt_data() {
    let rules = ChinaRules::parse("0.0.0.0/0", "::/0", "server=/example.com/1.1.1.1").unwrap();
    assert!(rules.contains_ip("255.255.255.255".parse().unwrap()));
    assert!(rules.contains_ip("ffff:ffff:ffff:ffff:ffff:ffff:ffff:ffff".parse().unwrap()));
    assert!(ChinaRules::parse("<html>", "::/0", "server=/example.com/1.1.1.1").is_err());
    assert!(ChinaRules::parse("1.1.1.1/32", "::/0", "error").is_err());
    assert!(ChinaRules::parse("::/0", "::/0", "server=/example.com/1.1.1.1").is_err());
  }
  #[test]
  fn replacement_keeps_existing_snapshots_valid() {
    let store = ChinaRulesStore::default();
    assert!(store.snapshot().1.is_none());
    store.replace(rules());
    let (version, old) = store.snapshot();
    store
      .replace(ChinaRules::parse("2.0.0.0/24", "2002::/16", "server=/other.com/1.1.1.1").unwrap());
    let (next, new) = store.snapshot();
    assert!(next > version);
    assert!(old.unwrap().contains_ip("1.0.0.1".parse().unwrap()));
    assert!(!new.unwrap().contains_ip("1.0.0.1".parse().unwrap()));
  }
}
