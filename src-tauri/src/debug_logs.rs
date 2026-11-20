//! Opt-in diagnostic files. Disk I/O stays on one bounded worker queue.
use std::{
  fs::{self, File, OpenOptions},
  io::{self, Write},
  path::{Path, PathBuf},
  sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc::{self, SyncSender},
    Arc,
  },
};

const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
const FILE_COUNT: usize = 3;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugStatus {
  enabled: bool,
  dropped_entries: u64,
  error: Option<String>,
}
#[derive(serde::Serialize)]
pub struct DebugExport {
  pub path: String,
}
enum Request {
  Entry(String),
  Configure(bool, mpsc::Sender<io::Result<DebugStatus>>),
  Status(mpsc::Sender<DebugStatus>),
  Export(PathBuf, mpsc::Sender<io::Result<DebugExport>>),
}
#[derive(Clone)]
pub struct DebugLogs {
  session: String,
  sender: SyncSender<Request>,
  enabled: Arc<AtomicBool>,
  dropped: Arc<AtomicU64>,
}
impl DebugLogs {
  pub fn new(directory: PathBuf, session: String) -> io::Result<Self> {
    let log_session = session.clone();
    let (sender, receiver) = mpsc::sync_channel(2048);
    let enabled = Arc::new(AtomicBool::new(false));
    let dropped = Arc::new(AtomicU64::new(0));
    let active = enabled.clone();
    let lost = dropped.clone();
    std::thread::Builder::new()
      .name("cloudtun-debug-log".into())
      .spawn(move || {
        let mut writer = Writer {
          directory,
          file: None,
          bytes: 0,
        };
        let mut error = None;
        let status = |error: &Option<String>| DebugStatus {
          enabled: active.load(Ordering::Relaxed),
          dropped_entries: lost.load(Ordering::Relaxed),
          error: error.clone(),
        };
        while let Ok(request) = receiver.recv() {
          match request {
            Request::Configure(value, reply) => {
              let result = if value {
                writer.open()
              } else {
                writer.file.take();
                Ok(())
              };
              match result {
                Ok(()) => {
                  active.store(value, Ordering::Relaxed);
                  error = None;
                  let _ = reply.send(Ok(status(&error)));
                }
                Err(failure) => {
                  active.store(false, Ordering::Relaxed);
                  error = Some(failure.to_string());
                  let _ = reply.send(Err(failure));
                }
              }
            }
            Request::Entry(line) if active.load(Ordering::Relaxed) => {
              if let Err(failure) = writer.write(&line) {
                active.store(false, Ordering::Relaxed);
                writer.file.take();
                error = Some(failure.to_string());
              }
            }
            Request::Entry(_) => {}
            Request::Status(reply) => {
              let _ = reply.send(status(&error));
            }
            Request::Export(destination, reply) => {
              let mut created = false;
              let result = (|| {
                let mut output = private_file(&destination, true)?;
                created = true;
                let metadata = serde_json::json!({
                  "type": "diagnostic-export", "version": env!("CARGO_PKG_VERSION"),
                  "os": std::env::consts::OS, "arch": std::env::consts::ARCH,
                  "sessionId": session, "status": status(&error),
                });
                writeln!(output, "{metadata}")?;
                for index in (0..FILE_COUNT).rev() {
                  match File::open(writer.path(index)) {
                    Ok(mut source) => {
                      io::copy(&mut source, &mut output)?;
                    }
                    Err(failure) if failure.kind() == io::ErrorKind::NotFound => {}
                    Err(failure) => return Err(failure),
                  }
                }
                output.sync_all()?;
                Ok(DebugExport {
                  path: destination.to_string_lossy().into_owned(),
                })
              })();
              if result.is_err() && created {
                let _ = fs::remove_file(destination);
              }
              let _ = reply.send(result);
            }
          }
        }
      })?;
    Ok(Self {
      session: log_session,
      sender,
      enabled,
      dropped,
    })
  }
  pub fn record(&self, text: &str) {
    if !self.enabled.load(Ordering::Relaxed) {
      return;
    }
    let mut end = text.len().min(8192);
    while !text.is_char_boundary(end) {
      end -= 1;
    }
    // No headers, bodies or configuration objects enter the diagnostic stream.
    let line = serde_json::json!({
      "sessionId": self.session,
      "timestamp": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
      "text": redact(&text[..end]),
    })
    .to_string();
    if self.sender.try_send(Request::Entry(line)).is_err() {
      self.dropped.fetch_add(1, Ordering::Relaxed);
    }
  }
  pub fn configure(&self, enabled: bool) -> io::Result<DebugStatus> {
    let (reply, response) = mpsc::channel();
    self
      .sender
      .send(Request::Configure(enabled, reply))
      .map_err(io::Error::other)?;
    response.recv().map_err(io::Error::other)?
  }
  pub fn status(&self) -> io::Result<DebugStatus> {
    let (reply, response) = mpsc::channel();
    self
      .sender
      .send(Request::Status(reply))
      .map_err(io::Error::other)?;
    response.recv().map_err(io::Error::other)
  }
  pub fn export(&self, destination: PathBuf) -> io::Result<DebugExport> {
    let (reply, response) = mpsc::channel();
    self
      .sender
      .send(Request::Export(destination, reply))
      .map_err(io::Error::other)?;
    response.recv().map_err(io::Error::other)?
  }
}
struct Writer {
  directory: PathBuf,
  file: Option<File>,
  bytes: u64,
}
impl Writer {
  fn path(&self, index: usize) -> PathBuf {
    self.directory.join(format!("debug-{index}.jsonl"))
  }
  fn open(&mut self) -> io::Result<()> {
    fs::create_dir_all(&self.directory)?;
    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt;
      fs::set_permissions(&self.directory, fs::Permissions::from_mode(0o700))?;
    }
    let file = private_file(&self.path(0), false)?;
    self.bytes = file.metadata()?.len();
    self.file = Some(file);
    Ok(())
  }
  fn write(&mut self, line: &str) -> io::Result<()> {
    if self.bytes + line.len() as u64 + 1 > MAX_FILE_BYTES {
      self.file.take();
      for index in (1..FILE_COUNT).rev() {
        let destination = self.path(index);
        match fs::remove_file(&destination) {
          Ok(()) => {}
          Err(failure) if failure.kind() == io::ErrorKind::NotFound => {}
          Err(failure) => return Err(failure),
        }
        match fs::rename(self.path(index - 1), destination) {
          Ok(()) => {}
          Err(failure) if failure.kind() == io::ErrorKind::NotFound => {}
          Err(failure) => return Err(failure),
        }
      }
      self.open()?;
    }
    writeln!(
      self
        .file
        .as_mut()
        .ok_or_else(|| io::Error::other("调试日志未开启"))?,
      "{line}"
    )?;
    self.bytes += line.len() as u64 + 1;
    Ok(())
  }
}
fn private_file(path: &Path, exclusive: bool) -> io::Result<File> {
  let mut options = OpenOptions::new();
  options.write(true);
  if exclusive {
    options.create_new(true);
  } else {
    options.create(true).append(true);
  }
  #[cfg(unix)]
  {
    use std::os::unix::fs::OpenOptionsExt;
    options.mode(0o600);
  }
  let file = options.open(path)?;
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
  }
  Ok(file)
}
fn redact(text: &str) -> String {
  use regex::Regex;
  use std::sync::LazyLock;
  static LINKS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)(?:https?|wss?|vmess)://[^\s\"<>]+"#).unwrap());
  static SECRETS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)((?:secret[_-]?(?:id|key)|access[_-]?token|token|password|authorization|cookie|uuid)\s*[\"']?\s*[:=]\s*[\"']?)[^\s,\"';}]+"#).unwrap()
  });
  let urls = LINKS.replace_all(text, |capture: &regex::Captures<'_>| {
    let link = &capture[0];
    if link.to_ascii_lowercase().starts_with("vmess://") {
      return "vmess://[redacted]".to_owned();
    }
    let (scheme, rest) = link.split_once("://").unwrap();
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = authority.rsplit('@').next().unwrap_or_default();
    format!("{scheme}://{host}/[redacted]")
  });
  SECRETS.replace_all(&urls, "${1}[redacted]").into_owned()
}

#[cfg(test)]
mod tests {
  use super::*;
  fn directory() -> PathBuf {
    std::env::temp_dir().join(format!("cloudtun-debug-test-{}", uuid::Uuid::new_v4()))
  }
  #[test]
  fn opt_in_export_flushes_records_and_disabling_stops_writing() {
    let root = directory();
    fs::create_dir_all(&root).unwrap();
    let logs = DebugLogs::new(root.join("logs"), "session-test".into()).unwrap();
    assert!(!logs.status().unwrap().enabled);
    logs.record("before-enable");
    assert!(!root.join("logs").exists());
    logs.configure(true).unwrap();
    logs.record("[conn=1 target=example.com:443] token=top-secret stage=connect.transfer");
    logs.configure(false).unwrap();
    logs.record("after-disable");
    let destination = root.join("export.jsonl");
    logs.export(destination.clone()).unwrap();
    let text = fs::read_to_string(&destination).unwrap();
    assert!(text.contains("example.com:443"));
    assert!(text.contains("session-test"));
    assert!(!text.contains("top-secret"));
    assert!(!text.contains("before-enable"));
    assert!(!text.contains("after-disable"));
    for line in text.lines() {
      serde_json::from_str::<serde_json::Value>(line).unwrap();
    }
    assert!(logs.export(destination.clone()).is_err());
    assert_eq!(fs::read_to_string(destination).unwrap(), text);
    fs::remove_dir_all(root).unwrap();
  }
  #[test]
  fn redacts_credentials_links_and_query_parameters() {
    let text = redact(
      r#"https://user:password@host.test/api/subscribe?token=secret vmess://encoded-secret SecretKey=key Cookie: session-secret UUID: 11111111-1111-1111-1111-111111111111"#,
    );
    for secret in [
      "user:",
      "password@",
      "token=secret",
      "encoded-secret",
      "SecretKey=key",
      "session-secret",
      "11111111",
    ] {
      assert!(!text.contains(secret), "{text}");
    }
    assert!(text.contains("host.test"));
  }
  #[test]
  fn rotation_keeps_only_three_bounded_files_in_order() {
    let root = directory();
    let mut writer = Writer {
      directory: root.clone(),
      file: None,
      bytes: 0,
    };
    writer.open().unwrap();
    for index in 0..8 {
      writer
        .write(&format!(
          "entry-{index}:{}",
          "x".repeat(MAX_FILE_BYTES as usize / 2)
        ))
        .unwrap();
    }
    assert_eq!(fs::read_dir(&root).unwrap().count(), FILE_COUNT);
    for index in 0..FILE_COUNT {
      assert!(fs::metadata(writer.path(index)).unwrap().len() <= MAX_FILE_BYTES);
    }
    assert!(fs::read_to_string(writer.path(0))
      .unwrap()
      .starts_with("entry-7:"));
    assert!(fs::read_to_string(writer.path(2))
      .unwrap()
      .starts_with("entry-5:"));
    drop(writer);
    fs::remove_dir_all(root).unwrap();
  }
  #[test]
  fn reports_disk_failure_without_enabling_capture() {
    let root = directory();
    fs::write(&root, "not a directory").unwrap();
    let logs = DebugLogs::new(root.clone(), "test".into()).unwrap();
    assert!(logs.configure(true).is_err());
    let status = logs.status().unwrap();
    assert!(!status.enabled);
    assert!(status.error.is_some());
    fs::remove_file(root).unwrap();
  }
}
