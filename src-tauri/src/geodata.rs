use crate::util::emit_log;
use cloudtun_proxy::china::{ChinaRules, ChinaRulesStore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
  path::{Path, PathBuf},
  sync::{Arc, Mutex},
};
use tauri::{AppHandle, Runtime};
use tokio_util::sync::CancellationToken;

const IP_REPO: &str = "gaoyifan/china-operator-ip";
const DOMAIN_REPO: &str = "felixonmars/dnsmasq-china-list";
const MAX_FILE: usize = 8 * 1024 * 1024;
#[derive(Clone, Serialize, Deserialize)]
struct Manifest {
  schema: u8,
  ip_version: String,
  domain_version: String,
  hashes: [String; 3],
}
impl Manifest {
  fn validate(&self) -> anyhow::Result<()> {
    anyhow::ensure!(
      self.schema == 1 && valid_sha(&self.ip_version) && valid_sha(&self.domain_version),
      "大陆规则版本无效"
    );
    Ok(())
  }
  fn folder(&self) -> String {
    format!("{}-{}", self.ip_version, self.domain_version)
  }
}
fn valid_sha(sha: &str) -> bool {
  matches!(sha.len(), 40 | 64) && sha.bytes().all(|b| b.is_ascii_hexdigit())
}
fn hash(data: &[u8]) -> String {
  Sha256::digest(data)
    .iter()
    .map(|byte| format!("{byte:02x}"))
    .collect()
}
const FILES: [&str; 3] = ["china.txt", "china6.txt", "domains.conf"];
fn parse(files: &[Vec<u8>; 3]) -> anyhow::Result<ChinaRules> {
  ChinaRules::parse(
    std::str::from_utf8(&files[0])?,
    std::str::from_utf8(&files[1])?,
    std::str::from_utf8(&files[2])?,
  )
}
fn load(directory: &Path) -> anyhow::Result<(Manifest, ChinaRules)> {
  let manifest: Manifest = serde_json::from_slice(&std::fs::read(directory.join("current.json"))?)?;
  manifest.validate()?;
  let root = directory.join(manifest.folder());
  let mut files = [Vec::new(), Vec::new(), Vec::new()];
  for (i, name) in FILES.iter().enumerate() {
    anyhow::ensure!(
      std::fs::metadata(root.join(name))?.len() <= MAX_FILE as u64,
      "大陆规则缓存过大"
    );
    files[i] = std::fs::read(root.join(name))?;
    anyhow::ensure!(
      hash(&files[i]) == manifest.hashes[i],
      "大陆规则缓存校验失败"
    );
  }
  let rules = parse(&files)?;
  Ok((manifest, rules))
}
#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeoStatus {
  pub available: bool,
  pub ip_version: Option<String>,
  pub domain_version: Option<String>,
  pub phase: String,
  pub error: Option<String>,
  pub files: Vec<FileProgress>,
}
#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileProgress {
  pub name: String,
  pub downloaded: usize,
  pub total: Option<u64>,
  pub complete: bool,
}
pub struct GeoData {
  pub rules: Arc<ChinaRulesStore>,
  directory: PathBuf,
  manifest: Mutex<Option<Manifest>>,
  update: tokio::sync::Mutex<()>,
  initial_message: String,
  status: Mutex<GeoStatus>,
}
impl GeoData {
  pub fn new(directory: PathBuf) -> Self {
    let rules = Arc::new(ChinaRulesStore::default());
    let (manifest, initial_message) = if directory.join("current.json").exists() {
      match load(&directory) {
        Ok((manifest, data)) => {
          let (v4, v6, domains) = data.counts();
          rules.replace(data);
          let message = format!(
            "已加载大陆规则：IPv4 {v4} 个区间，IPv6 {v6} 个区间，{domains} 个域名；版本 {} / {}",
            &manifest.ip_version[..8],
            &manifest.domain_version[..8]
          );
          (Some(manifest), message)
        }
        Err(error) => (
          None,
          format!("本地大陆规则不可用：{error}；绕过大陆暂不生效，连接代理后重新下载"),
        ),
      }
    } else {
      (
        None,
        "尚未下载大陆规则：绕过大陆暂不生效，保留局域网直连；连接代理后自动下载".into(),
      )
    };
    let status = GeoStatus {
      available: manifest.is_some(),
      ip_version: manifest.as_ref().map(|m| m.ip_version.clone()),
      domain_version: manifest.as_ref().map(|m| m.domain_version.clone()),
      phase: if manifest.is_some() { "ready" } else { "idle" }.into(),
      ..Default::default()
    };
    Self {
      status: Mutex::new(status),
      rules,
      directory,
      manifest: Mutex::new(manifest),
      update: tokio::sync::Mutex::new(()),
      initial_message,
    }
  }
  pub fn status(&self) -> GeoStatus {
    self
      .status
      .lock()
      .unwrap_or_else(|e| e.into_inner())
      .clone()
  }
  fn progress(&self, index: usize, downloaded: usize, total: Option<u64>, complete: bool) {
    let mut status = self.status.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(file) = status.files.get_mut(index) {
      file.downloaded = downloaded;
      file.total = total;
      file.complete = complete;
    }
  }
  pub fn log_initial<R: Runtime>(&self, app: &AppHandle<R>) {
    emit_log(app, "log::geo", &self.initial_message);
  }
  pub async fn check<R: Runtime>(
    self: Arc<Self>,
    app: AppHandle<R>,
    port: u16,
    cancel: CancellationToken,
    force: bool,
  ) {
    if cancel.is_cancelled() {
      return;
    }
    let _guard = if force {
      tokio::select! { biased; _ = cancel.cancelled() => return, guard = self.update.lock() => guard }
    } else {
      let Ok(guard) = self.update.try_lock() else {
        return;
      };
      guard
    };
    {
      let mut status = self.status.lock().unwrap_or_else(|e| e.into_inner());
      status.phase = "checking".into();
      status.error = None;
      status.files.clear();
    }
    emit_log(
      &app,
      "log::geo",
      "代理已连接，正在通过当前代理检查大陆规则版本",
    );
    let result = self.update_rules(&app, port, cancel.clone(), force).await;
    if let Err(error) = result {
      {
        let mut status = self.status.lock().unwrap_or_else(|e| e.into_inner());
        status.phase = "error".into();
        status.error = Some(error.to_string());
      }
      let fallback = if self.rules.snapshot().1.is_some() {
        "继续使用现有大陆规则"
      } else {
        "绕过大陆仍不生效，保留局域网直连；下次连接时重试"
      };
      emit_log(
        &app,
        "log::geo",
        &format!("大陆规则更新未完成：{error}；{fallback}"),
      );
    }
  }
  async fn update_rules<R: Runtime>(
    &self,
    app: &AppHandle<R>,
    port: u16,
    cancel: CancellationToken,
    force: bool,
  ) -> anyhow::Result<()> {
    let client = reqwest::Client::builder()
      .no_proxy()
      .proxy(reqwest::Proxy::all(format!("http://127.0.0.1:{port}"))?)
      .user_agent("CloudTun-geodata")
      .connect_timeout(std::time::Duration::from_secs(10))
      .timeout(std::time::Duration::from_secs(300))
      .build()?;
    let old = self
      .manifest
      .lock()
      .unwrap_or_else(|e| e.into_inner())
      .clone();
    let versions = async {
      tokio::try_join!(
        head(&client, IP_REPO, "ip-lists"),
        head(&client, DOMAIN_REPO, "master")
      )
    };
    let (ip_version, domain_version) = tokio::select! { biased; _ = cancel.cancelled() => anyhow::bail!("代理连接已停止，取消版本检查"), result = versions => result? };
    emit_log(
      app,
      "log::geo",
      &format!(
        "远端大陆规则版本：IP {}，域名 {}",
        &ip_version[..8],
        &domain_version[..8]
      ),
    );
    if !force
      && old
        .as_ref()
        .is_some_and(|m| m.ip_version == ip_version && m.domain_version == domain_version)
    {
      self.status.lock().unwrap_or_else(|e| e.into_inner()).phase = "latest".into();
      emit_log(app, "log::geo", "大陆规则已是最新版本，无需下载");
      return Ok(());
    }
    emit_log(
      app,
      "log::geo",
      if force {
        "手动下载全部大陆规则，即使版本相同也重新下载"
      } else if old.is_some() {
        "发现新版大陆规则，正在下载变更文件"
      } else {
        "首次下载大陆规则；下载完成前绕过大陆暂不生效"
      },
    );
    {
      let mut status = self.status.lock().unwrap_or_else(|e| e.into_inner());
      status.phase = "downloading".into();
      status.files = FILES
        .iter()
        .map(|name| FileProgress {
          name: (*name).into(),
          ..Default::default()
        })
        .collect();
    }
    let root = old.as_ref().map(|m| self.directory.join(m.folder()));
    let urls = [
      format!("https://raw.githubusercontent.com/{IP_REPO}/{ip_version}/china.txt"),
      format!("https://raw.githubusercontent.com/{IP_REPO}/{ip_version}/china6.txt"),
      format!(
        "https://raw.githubusercontent.com/{DOMAIN_REPO}/{domain_version}/accelerated-domains.china.conf"
      ),
    ];
    let fetch_file = |index: usize| {
      let url = &urls[index];
      let root = root.as_ref();
      let reuse = !force
        && old.as_ref().is_some_and(|m| {
          if index < 2 {
            m.ip_version == ip_version
          } else {
            m.domain_version == domain_version
          }
        });
      let client = &client;
      async move {
        if reuse {
          if let Ok(bytes) = tokio::fs::read(root.unwrap().join(FILES[index])).await {
            self.progress(index, bytes.len(), Some(bytes.len() as u64), true);
            return Ok(bytes);
          }
        }
        download_progress(&client, url, |bytes, total, complete| {
          self.progress(index, bytes, total, complete)
        })
        .await
      }
    };
    let fetch = async {
      let (a, b, c) = tokio::try_join!(fetch_file(0), fetch_file(1), fetch_file(2))?;
      Ok::<_, anyhow::Error>([a, b, c])
    };
    let files = tokio::select! { biased; _ = cancel.cancelled() => anyhow::bail!("代理连接已停止，取消下载"), result = fetch => result? };
    self.status.lock().unwrap_or_else(|e| e.into_inner()).phase = "installing".into();
    let manifest = Manifest {
      schema: 1,
      ip_version,
      domain_version,
      hashes: files.each_ref().map(|file| hash(file)),
    };
    let directory = self.directory.clone();
    let next = manifest.clone();
    let store = self.rules.clone();
    let token = cancel.clone();
    let counts = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
      install(&directory, &next, &files, &store, &token)
    })
    .await??;
    {
      let mut status = self.status.lock().unwrap_or_else(|e| e.into_inner());
      status.available = true;
      status.ip_version = Some(manifest.ip_version.clone());
      status.domain_version = Some(manifest.domain_version.clone());
      status.phase = "ready".into();
    }
    *self.manifest.lock().unwrap_or_else(|e| e.into_inner()) = Some(manifest);
    emit_log(
      app,
      "log::geo",
      &format!(
        "大陆规则更新成功并已生效：IPv4 {} 个区间，IPv6 {} 个区间，{} 个域名；现有代理连接保持运行",
        counts.0, counts.1, counts.2
      ),
    );
    Ok(())
  }
}
fn install(
  directory: &Path,
  next: &Manifest,
  files: &[Vec<u8>; 3],
  store: &ChinaRulesStore,
  token: &CancellationToken,
) -> anyhow::Result<(usize, usize, usize)> {
  let data = parse(files)?;
  let counts = data.counts();
  anyhow::ensure!(!token.is_cancelled(), "代理连接已停止，取消规则安装");
  std::fs::create_dir_all(&directory)?;
  let target = directory.join(next.folder());
  std::fs::create_dir_all(&target)?;
  for (name, bytes) in FILES.iter().zip(files.iter()) {
    crate::proxy::save_private(&target.join(name), bytes)?;
  }
  anyhow::ensure!(!token.is_cancelled(), "代理连接已停止，保留旧版大陆规则");
  // This small manifest commits the complete version; a failed download never touches it.
  crate::proxy::save_private(&directory.join("current.json"), &serde_json::to_vec(&next)?)?;
  store.replace(data);
  // No mappings reference old files; runtime readers retain immutable Arc snapshots.
  if let Ok(entries) = std::fs::read_dir(directory) {
    for entry in entries.flatten() {
      if entry.file_type().is_ok_and(|kind| kind.is_dir())
        && entry.file_name() != target.file_name().unwrap()
        && entry.file_name().to_str().is_some_and(|name| {
          name
            .split_once('-')
            .is_some_and(|(ip, domain)| valid_sha(ip) && valid_sha(domain))
        })
      {
        let _ = std::fs::remove_dir_all(entry.path());
      }
    }
  }
  Ok(counts)
}
async fn download(client: &reqwest::Client, url: &str) -> anyhow::Result<Vec<u8>> {
  // Version metadata is small; rule files need a longer window on slow proxy links.
  tokio::time::timeout(
    std::time::Duration::from_secs(30),
    download_progress(client, url, |_, _, _| {}),
  )
  .await
  .map_err(|_| anyhow::anyhow!("规则版本检查超时"))?
}
async fn download_progress(
  client: &reqwest::Client,
  url: &str,
  mut progress: impl FnMut(usize, Option<u64>, bool),
) -> anyhow::Result<Vec<u8>> {
  let mut response = client
    .get(url)
    .send()
    .await
    .map_err(|_| anyhow::anyhow!("规则服务器请求失败"))?;
  anyhow::ensure!(
    response.status().is_success(),
    "规则服务器返回 HTTP {}",
    response.status().as_u16()
  );
  let total = response.content_length();
  anyhow::ensure!(
    total.is_none_or(|size| size <= MAX_FILE as u64),
    "规则文件超过大小限制"
  );
  progress(0, total, false);
  let mut bytes = Vec::new();
  while let Some(chunk) = response
    .chunk()
    .await
    .map_err(|_| anyhow::anyhow!("规则下载中断"))?
  {
    anyhow::ensure!(
      bytes.len() + chunk.len() <= MAX_FILE,
      "规则文件超过大小限制"
    );
    bytes.extend_from_slice(&chunk);
    progress(bytes.len(), total, false);
  }
  progress(bytes.len(), total, true);
  Ok(bytes)
}
async fn head(client: &reqwest::Client, repo: &str, branch: &str) -> anyhow::Result<String> {
  // Public proxy exits often exhaust GitHub's shared unauthenticated API quota.
  let api = async {
    let bytes = download(
      client,
      &format!("https://api.github.com/repos/{repo}/git/ref/heads/{branch}"),
    )
    .await?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)?;
    let sha = value["object"]["sha"]
      .as_str()
      .filter(|sha| valid_sha(sha))
      .ok_or_else(|| anyhow::anyhow!("规则服务器版本号无效"))?;
    Ok::<_, anyhow::Error>(sha.to_owned())
  }
  .await;
  if let Ok(sha) = api {
    return Ok(sha);
  }
  let bytes = download(
    client,
    &format!("https://github.com/{repo}/commits/{branch}.atom"),
  )
  .await?;
  atom_head(std::str::from_utf8(&bytes)?)
}
fn atom_head(feed: &str) -> anyhow::Result<String> {
  let sha = feed
    .split_once("<id>tag:github.com,2008:Grit::Commit/")
    .and_then(|(_, tail)| tail.split_once("</id>"))
    .map(|(sha, _)| sha)
    .filter(|sha| valid_sha(sha))
    .ok_or_else(|| anyhow::anyhow!("规则提交订阅源未返回有效版本号"))?;
  Ok(sha.to_owned())
}

#[cfg(test)]
mod tests {
  use super::*;
  #[tokio::test]
  async fn download_reports_received_bytes_and_content_length() {
    use tokio::{
      io::{AsyncReadExt, AsyncWriteExt},
      net::TcpListener,
    };
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
      let (mut socket, _) = listener.accept().await.unwrap();
      let mut request = [0u8; 2048];
      socket.read(&mut request).await.unwrap();
      socket
        .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 6\r\nConnection: close\r\n\r\nabc")
        .await
        .unwrap();
      tokio::time::sleep(std::time::Duration::from_millis(30)).await;
      socket.write_all(b"def").await.unwrap();
    });
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let mut samples = Vec::new();
    let data = download_progress(
      &client,
      &format!("http://{address}"),
      |received, total, complete| samples.push((received, total, complete)),
    )
    .await
    .unwrap();
    server.await.unwrap();
    assert_eq!(data, b"abcdef");
    assert_eq!(samples.first(), Some(&(0, Some(6), false)));
    assert!(
      samples
        .iter()
        .any(|&(received, _, complete)| received > 0 && received < 6 && !complete)
    );
    assert_eq!(samples.last(), Some(&(6, Some(6), true)));
  }
  #[test]
  fn commit_feed_requires_a_valid_commit_id() {
    let sha = "a".repeat(40);
    assert_eq!(
      atom_head(&format!(
        "<feed><id>branch</id><entry><id>tag:github.com,2008:Grit::Commit/{sha}</id></entry></feed>"
      ))
      .unwrap(),
      sha
    );
    assert!(atom_head("<id>tag:github.com,2008:Grit::Commit/../bad</id>").is_err());
    assert!(atom_head("<feed></feed>").is_err());
  }
  fn fixture(version: char) -> (Manifest, [Vec<u8>; 3]) {
    let files = [
      b"114.114.114.0/24".to_vec(),
      b"240e::/16".to_vec(),
      b"server=/baidu.com/1.1.1.1".to_vec(),
    ];
    (
      Manifest {
        schema: 1,
        ip_version: version.to_string().repeat(40),
        domain_version: "b".repeat(40),
        hashes: files.each_ref().map(|file| hash(file)),
      },
      files,
    )
  }
  #[test]
  fn cached_rules_load_without_network_and_corruption_is_rejected() {
    let directory = std::env::temp_dir().join(format!("cloudtun-geo-{}", uuid::Uuid::new_v4()));
    let (manifest, files) = fixture('a');
    let store = ChinaRulesStore::default();
    install(
      &directory,
      &manifest,
      &files,
      &store,
      &CancellationToken::new(),
    )
    .unwrap();
    let cached = GeoData::new(directory.clone());
    assert!(
      cached
        .rules
        .snapshot()
        .1
        .unwrap()
        .contains_ip("114.114.114.114".parse().unwrap())
    );
    std::fs::write(directory.join(manifest.folder()).join(FILES[0]), b"invalid").unwrap();
    assert!(GeoData::new(directory.clone()).rules.snapshot().1.is_none());
    std::fs::remove_dir_all(directory).unwrap();
  }
  #[test]
  fn invalid_or_cancelled_install_keeps_previous_cache_and_index() {
    let directory = std::env::temp_dir().join(format!("cloudtun-geo-{}", uuid::Uuid::new_v4()));
    let store = ChinaRulesStore::default();
    let (old, files) = fixture('a');
    install(&directory, &old, &files, &store, &CancellationToken::new()).unwrap();
    let before = store.snapshot().0;
    let (next, mut files) = fixture('c');
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert!(install(&directory, &next, &files, &store, &cancel).is_err());
    files[0] = b"<html>error</html>".to_vec();
    assert!(install(&directory, &next, &files, &store, &CancellationToken::new()).is_err());
    assert_eq!(store.snapshot().0, before);
    assert_eq!(load(&directory).unwrap().0.ip_version, old.ip_version);
    std::fs::remove_dir_all(directory).unwrap();
  }
}
