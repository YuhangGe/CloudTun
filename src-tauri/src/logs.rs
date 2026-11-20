use anyhow_tauri::TAResult;
use serde::Serialize;
use std::{
  collections::VecDeque,
  sync::Mutex,
  time::{SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

const MAX_LOGS: usize = 1000;
const MAX_TEXT_BYTES: usize = 8192;
#[derive(Clone, Serialize)]
pub struct LogEntry {
  pub id: u64,
  pub timestamp: u64,
  pub text: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogSnapshot {
  pub session_id: String,
  pub logs: Vec<LogEntry>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEvent {
  pub session_id: String,
  pub entry: LogEntry,
}
struct Buffer {
  next_id: u64,
  entries: VecDeque<LogEntry>,
}
pub struct Logs {
  session_id: String,
  buffer: Mutex<Buffer>,
}
impl Logs {
  pub fn new() -> Self {
    Self {
      session_id: uuid::Uuid::new_v4().to_string(),
      buffer: Mutex::new(Buffer {
        next_id: 1,
        entries: VecDeque::with_capacity(MAX_LOGS),
      }),
    }
  }
  fn append(&self, text: &str) -> LogEvent {
    let mut end = text.len().min(MAX_TEXT_BYTES);
    while !text.is_char_boundary(end) {
      end -= 1;
    }
    let text = if end < text.len() {
      format!("{}…", &text[..end])
    } else {
      text.to_owned()
    };
    let mut buffer = self
      .buffer
      .lock()
      .unwrap_or_else(|error| error.into_inner());
    let entry = LogEntry {
      id: buffer.next_id,
      timestamp: SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64,
      text,
    };
    buffer.next_id += 1;
    if buffer.entries.len() == MAX_LOGS {
      buffer.entries.pop_front();
    }
    buffer.entries.push_back(entry.clone());
    LogEvent {
      session_id: self.session_id.clone(),
      entry,
    }
  }
  fn snapshot(&self) -> LogSnapshot {
    let buffer = self
      .buffer
      .lock()
      .unwrap_or_else(|error| error.into_inner());
    LogSnapshot {
      session_id: self.session_id.clone(),
      logs: buffer.entries.iter().cloned().collect(),
    }
  }
}
pub fn record<R: Runtime>(app: &AppHandle<R>, text: &str) {
  if let Some(logs) = app.try_state::<Logs>() {
    let event = logs.append(text);
    let _ = app.emit("log-entry", &event);
  }
}
#[tauri::command]
pub fn tauri_get_logs(logs: State<'_, Logs>) -> TAResult<LogSnapshot> {
  Ok(logs.snapshot())
}
#[tauri::command]
pub fn tauri_append_log<R: Runtime>(
  app: AppHandle<R>,
  logs: State<'_, Logs>,
  text: String,
) -> TAResult<LogEvent> {
  let event = logs.append(&text);
  let _ = app.emit("log-entry", &event);
  Ok(event)
}

#[cfg(test)]
mod tests {
  use super::*;
  #[test]
  fn keeps_latest_thousand_entries_across_snapshot_reads() {
    let logs = Logs::new();
    for i in 0..1050 {
      logs.append(&format!("entry {i}"));
    }
    let first = logs.snapshot();
    let reopened = logs.snapshot();
    assert_eq!(first.logs.len(), 1000);
    assert_eq!(first.logs[0].text, "entry 50");
    assert_eq!(first.logs[999].text, "entry 1049");
    assert_eq!(first.session_id, reopened.session_id);
    assert_eq!(reopened.logs[0].id, first.logs[0].id);
    assert_eq!(logs.append("next").entry.id, 1051);
  }
  #[test]
  fn concurrent_writers_get_unique_ordered_ids() {
    let logs = std::sync::Arc::new(Logs::new());
    let threads: Vec<_> = (0..4)
      .map(|_| {
        let logs = logs.clone();
        std::thread::spawn(move || {
          for _ in 0..400 {
            logs.append("message");
          }
        })
      })
      .collect();
    for thread in threads {
      thread.join().unwrap();
    }
    let snapshot = logs.snapshot();
    assert_eq!(snapshot.logs.len(), 1000);
    assert_eq!(snapshot.logs[0].id, 601);
    assert!(snapshot
      .logs
      .windows(2)
      .all(|pair| pair[1].id == pair[0].id + 1));
  }
  #[test]
  fn limits_large_messages_on_unicode_boundaries() {
    let logs = Logs::new();
    let event = logs.append(&"日志".repeat(3000));
    assert!(event.entry.text.len() <= MAX_TEXT_BYTES + "…".len());
    assert!(event.entry.text.ends_with('…'));
  }
}
