// static GEOSITE_DATA: &[u8] = include_bytes!("data/geosite.dat");
// static GEOIP_DATA: &[u8] = include_bytes!("data/geoip.dat");

use std::{collections::HashMap, sync::Arc};

use tokio::sync::RwLock;

#[derive(Debug, Clone, Copy)]
pub enum MatchType {
  Direct,
  Proxy,
  Deny,
}

#[derive(Debug, Default)]
struct TrieNode {
  match_type: Option<MatchType>,
  next: HashMap<u8, TrieNode>,
}

#[derive(Debug, Clone)]
struct Trie {
  root: Arc<RwLock<TrieNode>>,
}

impl Trie {
  pub fn new() -> Self {
    Trie {
      root: Arc::new(RwLock::new(TrieNode::default())),
    }
  }

  pub async fn insert(&mut self, url: &str, match_type: MatchType) {
    let trie = &mut self.root.write().await;
    let mut cnext: &mut HashMap<u8, TrieNode> = &mut trie.next;
    let len = url.len() - 1;
    for (index, data) in url.as_bytes().iter().rev().enumerate() {
      if index == len {
        cnext
          .entry(*data)
          .and_modify(|_| {
            panic!("duplicated rule domain: {}", url);
          })
          .or_insert(TrieNode {
            match_type: Some(match_type),
            next: HashMap::new(),
          });
        println!("insert new rule: {}, {:?}", url, match_type);
      } else {
        let x = cnext.entry(*data).or_default();
        cnext = &mut x.next;
      }
    }
  }

  pub async fn search(&self, url: &str) -> Option<MatchType> {
    let mut ret = None;
    let mut cnext = &self.root.read().await.next;
    for data in url.as_bytes().iter().rev() {
      match cnext.get(data) {
        Some(x) => {
          if let Some(match_type) = x.match_type {
            ret.replace(match_type);
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
}

impl RouteMatcher {
  pub fn new() -> Self {
    RouteMatcher { trie: Trie::new() }
  }

  pub async fn config_rules(&mut self, rules: Option<String>) {
    self.trie = Trie::new();

    let Some(rules) = rules else {
      return;
    };

    for (line_number, line_content) in rules.split('\n').enumerate() {
      let line_cnt = line_content.trim_ascii();
      if line_cnt.is_empty() || line_cnt.starts_with('#') {
        return;
      }
      let mut line_it = line_content.splitn(2, ':');
      let Some(domain) = line_it.next() else {
        println!("bad rule config at line {}: missing domain", line_number);
        return;
      };
      let Some(match_type) = line_it.next() else {
        println!(
          "bad rule config at line {}: missing match type",
          line_number
        );
        return;
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
          return;
        }
      };
      self.trie.insert(domain, match_type).await;
    }
  }
  pub async fn match_domain(&self, domain: &str) -> Option<MatchType> {
    self.trie.search(domain).await
  }
}
