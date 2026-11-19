// static GEOSITE_DATA: &[u8] = include_bytes!("data/geosite.dat");
// static GEOIP_DATA: &[u8] = include_bytes!("data/geoip.dat");

use std::{
  collections::HashMap,
  fmt::Display,
  ops::{Add, DerefMut},
  sync::Arc,
};

use tokio::sync::{Mutex, RwLock};

#[derive(Debug, Clone, Copy)]
pub enum MatchType {
  Direct,
  Proxy,
  Deny,
}

impl Display for MatchType {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str(match self {
      Self::Deny => "deny",
      Self::Direct => "direct",
      Self::Proxy => "proxy",
    })
  }
}

#[derive(Debug, Default)]
struct TrieNode {
  match_type: Option<MatchType>,
  next: HashMap<u8, TrieNode>,
}

#[derive(Debug, Clone)]
struct Trie {
  rules_count: Arc<Mutex<usize>>,
  root: Arc<RwLock<TrieNode>>,
}

impl Trie {
  pub fn new() -> Self {
    Trie {
      rules_count: Arc::new(Mutex::new(0)),
      root: Arc::new(RwLock::new(TrieNode::default())),
    }
  }

  pub async fn insert(&mut self, url: &str, match_type: MatchType) {
    let trie = &mut self.root.write().await;
    let rules_count = &mut self.rules_count.lock().await;
    let mut node = &mut **trie;
    for data in url.as_bytes().iter().rev() {
      node = node.next.entry(*data).or_default();
    }
    // Existing nodes may already have child rules. Preserve them.
    if node.match_type.replace(match_type).is_none() {
      *rules_count.deref_mut() = rules_count.add(1);
    }
  }

  pub async fn search(&self, url: &str) -> Option<MatchType> {
    let mut ret = None;
    let mut cnext = &self.root.read().await.next;
    for (index, data) in url.as_bytes().iter().rev().enumerate() {
      match cnext.get(data) {
        Some(x) => {
          if let Some(match_type) = x.match_type {
            if index + 1 == url.len() || url.as_bytes()[url.len() - index - 2] == b'.' {
              ret.replace(match_type);
            }
          }
          cnext = &x.next;
        }
        _ => {
          break;
        }
      }
    }
    ret
  }
}

#[derive(Debug, Clone)]
pub struct RouteMatcher {
  trie: Trie,
  pub default_rule: MatchType,
  // pub rules_config_file: Option<String>,
}

async fn load_rules(proxy_rules: &str, trie: &mut Trie) -> std::io::Result<()> {
  for (line_number, line_content) in proxy_rules.split('\n').enumerate() {
    let line_cnt = line_content.trim_ascii();
    if line_cnt.is_empty() || line_cnt.starts_with('#') {
      continue;
    }
    let mut line_it = line_content.splitn(2, ':');
    let Some(domain) = line_it
      .next()
      .map(|v| v.trim_ascii())
      .filter(|v| !v.is_empty())
    else {
      println!("bad rule config at line {}: missing domain", line_number);
      continue;
    };
    let Some(match_type) = line_it
      .next()
      .map(|v| v.trim_ascii())
      .filter(|v| !v.is_empty())
    else {
      println!(
        "bad rule config at line {}: missing match type",
        line_number
      );
      continue;
    };
    let match_type = match match_type {
      "deny" => MatchType::Deny,
      "direct" => MatchType::Direct,
      "proxy" => MatchType::Proxy,
      _ => {
        println!(
          "bad rule config at line {}: invalid match type",
          line_number
        );
        continue;
      }
    };
    trie
      .insert(
        &domain.trim_end_matches('.').to_ascii_lowercase(),
        match_type,
      )
      .await;
  }
  Ok(())
}

impl RouteMatcher {
  #[cfg(test)]
  pub async fn get_count(&self) -> usize {
    *self.trie.rules_count.lock().await
  }

  pub async fn load(
    default_rule: MatchType,
    rules_config_file: Option<String>,
  ) -> std::io::Result<Self> {
    let mut trie = Trie::new();
    if let Some(rules_config_file) = &rules_config_file {
      load_rules(rules_config_file, &mut trie).await?;
    }
    Ok(RouteMatcher { trie, default_rule })
  }

  pub async fn match_domain(&self, domain: &str) -> MatchType {
    self
      .trie
      .search(&domain.trim_end_matches('.').to_ascii_lowercase())
      .await
      .unwrap_or(self.default_rule)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  #[tokio::test]
  async fn matches_labels_and_normalizes_names() {
    let router = RouteMatcher::load(MatchType::Proxy, Some("example.com:direct".into()))
      .await
      .unwrap();
    assert!(matches!(
      router.match_domain("EXAMPLE.COM.").await,
      MatchType::Direct
    ));
    assert!(matches!(
      router.match_domain("www.example.com").await,
      MatchType::Direct
    ));
    assert!(matches!(
      router.match_domain("notexample.com").await,
      MatchType::Proxy
    ));
  }
  #[tokio::test]
  async fn preserves_children_when_parent_is_added_later() {
    let router = RouteMatcher::load(
      MatchType::Deny,
      Some("www.example.com:proxy\nexample.com:direct\nexample.com:direct".into()),
    )
    .await
    .unwrap();
    assert!(matches!(
      router.match_domain("www.example.com").await,
      MatchType::Proxy
    ));
    assert!(matches!(
      router.match_domain("example.com").await,
      MatchType::Direct
    ));
    assert_eq!(router.get_count().await, 2);
  }
}
