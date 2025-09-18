// static GEOSITE_DATA: &[u8] = include_bytes!("data/geosite.dat");
// static GEOIP_DATA: &[u8] = include_bytes!("data/geoip.dat");

use std::{collections::HashMap, fmt::Display, sync::Arc};

use tokio::{fs::File, io::AsyncReadExt, sync::RwLock};

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
        // println!("insert new rule: {}, {}", url, match_type);
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
  pub default_rule: MatchType,
  // pub rules_config_file: Option<String>,
}

async fn load_rules(rules_config_file: &str, trie: &mut Trie) -> std::io::Result<()> {
  let mut f = File::open(rules_config_file).await?;
  let mut rules = String::new();
  f.read_to_string(&mut rules).await?;

  for (line_number, line_content) in rules.split('\n').enumerate() {
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
    trie.insert(domain, match_type).await;
  }
  Ok(())
}

impl RouteMatcher {
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
    self.trie.search(domain).await.unwrap_or(self.default_rule)
  }
}
