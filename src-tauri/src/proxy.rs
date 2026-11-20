use crate::{
  proxy_config::{ProxyConfig, ProxyConfigView},
  system_proxy,
  util::emit_log,
};
use anyhow_tauri::{IntoTAResult, TAResult};
use cloudtun_proxy::{
  PreparedProxy, ProxyArgs, RoutingMode, TrafficCounter, TrafficSnapshot,
  subscription::{NodeLatency, SubscriptionNode, fetch_subscription, ping_node, ping_nodes},
};
use serde::{Deserialize, Serialize};
use std::{
  path::{Path, PathBuf},
  sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
  },
  time::{SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter, Manager, Runtime, State};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProxyStatus {
  pub running: bool,
  pub source: Option<String>,
  pub node_id: Option<String>,
  pub name: Option<String>,
  pub mode: RoutingMode,
  pub system_proxy: bool,
  pub error: Option<String>,
}
struct RunningProxy {
  traffic: TrafficCounter,
  cancel: CancellationToken,
  task: tokio::task::JoinHandle<()>,
  listener: cloudtun_proxy::ProxyListenerControl,
}
#[derive(Default)]
struct RuntimeState {
  running: Option<RunningProxy>,
  status: ProxyStatus,
}
#[derive(Default, Serialize, Deserialize)]
struct StoredSubscription {
  url: String,
  nodes: Vec<SubscriptionNode>,
  updated_at: u64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeView {
  id: String,
  name: String,
  protocol: String,
  address: String,
  port: u16,
  unsupported_reason: Option<String>,
}
impl From<&SubscriptionNode> for NodeView {
  fn from(node: &SubscriptionNode) -> Self {
    Self {
      id: node.id.clone(),
      name: node.name.clone(),
      protocol: node.protocol.clone(),
      address: node.address.clone(),
      port: node.port,
      unsupported_reason: node.unsupported_reason.clone(),
    }
  }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionView {
  url: String,
  nodes: Vec<NodeView>,
  updated_at: u64,
}
impl From<&StoredSubscription> for SubscriptionView {
  fn from(value: &StoredSubscription) -> Self {
    Self {
      url: value.url.clone(),
      nodes: value.nodes.iter().map(NodeView::from).collect(),
      updated_at: value.updated_at,
    }
  }
}

pub struct ProxyLoop {
  transition: Mutex<()>,
  inner: Arc<Mutex<RuntimeState>>,
  subscription: Mutex<StoredSubscription>,
  revision: AtomicU64,
  config: Mutex<ProxyConfig>,
  directory: PathBuf,
  geodata: Arc<crate::geodata::GeoData>,
}
impl ProxyLoop {
  pub fn new(directory: PathBuf) -> anyhow::Result<Self> {
    std::fs::create_dir_all(&directory)?;
    let file = directory.join("subscription.json");
    let subscription = if file.exists() {
      serde_json::from_slice(&std::fs::read(file)?).unwrap_or_default()
    } else {
      StoredSubscription::default()
    };
    let config_file = directory.join("proxy-config.json");
    let config: ProxyConfig = if config_file.exists() {
      serde_json::from_slice(&std::fs::read(config_file)?)?
    } else {
      ProxyConfig::default()
    };
    config.validate()?;
    // Recover a proxy left behind by a previous crash before accepting a new connection.
    let recovery_error = system_proxy::restore(&directory.join("system-proxy-backup.json"))
      .err()
      .map(|e| e.to_string());
    let mut state = RuntimeState::default();
    state.status.error = recovery_error;
    Ok(Self {
      transition: Mutex::new(()),
      inner: Arc::new(Mutex::new(state)),
      subscription: Mutex::new(subscription),
      revision: AtomicU64::new(0),
      config: Mutex::new(config),
      geodata: Arc::new(crate::geodata::GeoData::new(directory.join("china-rules"))),
      directory,
    })
  }
  pub fn log_geo_status<R: Runtime>(&self, handle: &AppHandle<R>) {
    self.geodata.log_initial(handle);
  }
  pub async fn traffic_snapshot(&self) -> Option<TrafficSnapshot> {
    self
      .inner
      .lock()
      .await
      .running
      .as_ref()
      .map(|proxy| proxy.traffic.snapshot())
  }
  fn backup(&self) -> PathBuf {
    self.directory.join("system-proxy-backup.json")
  }
  async fn stop(&self) -> anyhow::Result<()> {
    let running = self.inner.lock().await.running.take();
    if let Some(running) = running {
      running.cancel.cancel();
      let _ = running.task.await;
    }
    let path = self.backup();
    let restored = tokio::task::spawn_blocking(move || system_proxy::restore(&path)).await?;
    let mut state = self.inner.lock().await;
    state.status = ProxyStatus {
      error: restored.as_ref().err().map(|e| e.to_string()),
      system_proxy: self.backup().exists(),
      ..ProxyStatus::default()
    };
    restored
  }
  async fn start<R: Runtime>(
    &self,
    proxy: PreparedProxy,
    h: AppHandle<R>,
    source: &str,
    node_id: Option<String>,
    name: String,
    mode: RoutingMode,
  ) -> anyhow::Result<()> {
    let listener = proxy.listener_control();
    let traffic = proxy.traffic_counter();
    let cancel = CancellationToken::new();
    let token = cancel.clone();
    let inner = self.inner.clone();
    let backup = self.backup();
    let app = h.clone();
    let mut state = self.inner.lock().await;
    let task = tokio::spawn(async move {
      let log_handle = app.clone();
      let error = proxy
        .run(token.clone(), move |kind, text| {
          emit_log(&log_handle, kind, text)
        })
        .await
        .err()
        .map(|e| e.to_string());
      if !token.is_cancelled() {
        let error = error.unwrap_or_else(|| "代理监听已意外停止".into());
        {
          let mut state = inner.lock().await;
          state.running = None;
          state.status.running = false;
          state.status.error = Some(error.clone());
        }
        let _ = tokio::task::spawn_blocking(move || system_proxy::restore(&backup)).await;
        let mut state = inner.lock().await;
        state.status.system_proxy = false;
        emit_log(&app, "proxy::error", &error);
        let _ = app.emit("proxy-status", &state.status);
      }
    });
    let update_cancel = cancel.clone();
    state.running = Some(RunningProxy {
      traffic,
      cancel,
      task,
      listener,
    });
    state.status = ProxyStatus {
      running: true,
      source: Some(source.into()),
      node_id,
      name: Some(name),
      mode,
      system_proxy: false,
      error: None,
    };
    let _ = h.emit("proxy-status", &state.status);
    drop(state);
    let geodata = self.geodata.clone();
    let port = self.config.lock().await.port;
    tokio::spawn(async move {
      geodata.check(h, port, update_cancel, false).await;
    });
    Ok(())
  }
}
pub(crate) fn save_private(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
  use std::io::Write;
  let temporary = path.with_extension("tmp");
  let mut options = std::fs::OpenOptions::new();
  options.write(true).create(true).truncate(true);
  #[cfg(unix)]
  {
    use std::os::unix::fs::OpenOptionsExt;
    options.mode(0o600);
  }
  let mut file = options.open(&temporary)?;
  file.write_all(bytes)?;
  file.sync_all()?;
  drop(file);
  std::fs::rename(temporary, path)?;
  Ok(())
}
pub async fn stop_proxy_client(state: State<'_, ProxyLoop>) {
  let _transition = state.transition.lock().await;
  if let Err(error) = state.stop().await {
    log::error!("Failed to restore system proxy: {error}");
  }
}

#[tauri::command]
pub async fn tauri_start_proxy_client<R: Runtime>(
  handle: AppHandle<R>,
  state: State<'_, ProxyLoop>,
  server_ip: String,
  cvm_id: String,
  mode: RoutingMode,
) -> TAResult<()> {
  async {
    anyhow::ensure!(
      !server_ip.is_empty() && !cvm_id.is_empty(),
      "请先配置云主机"
    );
    let _transition = state.transition.lock().await;
    let server = handle
      .state::<crate::cloud_access::CloudAccess>()
      .endpoint(&server_ip, &cvm_id)?;
    anyhow::ensure!(
      cloudtun_common::ping::ping_cloudtun_proxy_server(&server).await?,
      "云上主机代理尚未就绪或不可连接，当前代理未切换"
    );
    state.stop().await?;
    let config = state.config.lock().await.clone();
    let proxy = PreparedProxy::bind_with_rules(
      ProxyArgs {
        server,
        local_addr: (config.listen_host, config.port),
        default_rule: cloudtun_proxy::MatchType::Proxy,
        proxy_rules: None,
      },
      mode,
      state.geodata.rules.clone(),
    )
    .await?;
    state
      .start(proxy, handle, "cloud", None, server_ip, mode)
      .await
  }
  .await
  .into_ta_result()
}
#[tauri::command]
pub async fn tauri_stop_proxy_client<R: Runtime>(
  handle: AppHandle<R>,
  state: State<'_, ProxyLoop>,
) -> TAResult<()> {
  async {
    let _transition = state.transition.lock().await;
    let result = state.stop().await;
    let _ = handle.emit("proxy-status", &state.inner.lock().await.status);
    result
  }
  .await
  .into_ta_result()
}
#[tauri::command]
pub async fn tauri_proxy_status(state: State<'_, ProxyLoop>) -> TAResult<ProxyStatus> {
  Ok(state.inner.lock().await.status.clone())
}
#[cfg(target_os = "macos")]
pub fn update_tray_speed<R: Runtime>(tray: tauri::tray::TrayIcon<R>) {
  const TITLE_GAP: &str = "\u{2002}";

  tauri::async_runtime::spawn(async move {
    let original_width = tray
      .with_inner_tray_icon(|icon| icon.ns_status_item().map(|item| item.length()))
      .ok()
      .flatten();
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(1));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut previous: Option<TrafficSnapshot> = None;
    let mut displayed = None;
    loop {
      tick.tick().await;
      let current = tray
        .app_handle()
        .state::<ProxyLoop>()
        .traffic_snapshot()
        .await;
      let title = current.as_ref().map(|sample| {
        let rate = previous
          .as_ref()
          .filter(|last| {
            last.session_id == sample.session_id && sample.elapsed_ms > last.elapsed_ms
          })
          .map_or(0.0, |last| {
            sample.total_bytes.saturating_sub(last.total_bytes) as f64 * 1000.0
              / (sample.elapsed_ms - last.elapsed_ms) as f64
          });
        let (value, unit) = if rate >= 1_000_000_000.0 {
          (rate / 1_000_000_000.0, "GB")
        } else if rate >= 1_000_000.0 {
          (rate / 1_000_000.0, "MB")
        } else if rate >= 1_000.0 {
          (rate / 1_000.0, "KB")
        } else {
          (rate, "B")
        };
        if unit != "B" && value < 10.0 {
          format!("{TITLE_GAP}{value:.1} {unit}/s")
        } else {
          format!("{TITLE_GAP}{value:.0} {unit}/s")
        }
      });
      previous = current;
      if title == displayed {
        continue;
      }
      if displayed.is_none() && title.is_some() && original_width.is_some() {
        let _ = tray.with_inner_tray_icon(|icon| {
          if let Some(item) = icon.ns_status_item() {
            if let Some(button) = item.button(objc2::MainThreadMarker::new().unwrap()) {
              let old_title = button.title();
              let sample = objc2_foundation::NSString::from_str(&format!("{TITLE_GAP}8888 Kb/s"));
              button.setTitle(&sample);
              let width = button
                .cell()
                .map(|cell| cell.cellSize().width.ceil() - 10.0);
              button.setTitle(&old_title);
              if let Some(width) = width.filter(|width| *width > 0.0) {
                item.setLength(width);
              }
            }
          }
        });
      }
      if tray.set_title(title.as_deref()).is_ok() {
        let connected = title.is_some();
        if let Some(width) = original_width {
          let _ = tray.with_inner_tray_icon(move |icon| {
            if let Some(item) = icon.ns_status_item() {
              if !connected {
                item.setLength(width);
              }
              if let Some(button) = item.button(objc2::MainThreadMarker::new().unwrap()) {
                let alignment = if connected {
                  objc2_app_kit::NSTextAlignment::Left
                } else {
                  objc2_app_kit::NSTextAlignment::Center
                };
                button.setAlignment(alignment);
              }
            }
          });
        }
        displayed = title;
      }
    }
  });
}
#[tauri::command]
pub async fn tauri_proxy_traffic(state: State<'_, ProxyLoop>) -> TAResult<Option<TrafficSnapshot>> {
  Ok(state.traffic_snapshot().await)
}
#[tauri::command]
pub async fn tauri_get_subscription(state: State<'_, ProxyLoop>) -> TAResult<SubscriptionView> {
  Ok(SubscriptionView::from(&*state.subscription.lock().await))
}
#[tauri::command]
pub async fn tauri_update_subscription<R: Runtime>(
  handle: AppHandle<R>,
  state: State<'_, ProxyLoop>,
  url: String,
) -> TAResult<SubscriptionView> {
  async {
    let revision = state.revision.fetch_add(1, Ordering::SeqCst) + 1;
    let nodes = fetch_subscription(url.trim()).await?;
    let _transition = state.transition.lock().await;
    anyhow::ensure!(
      state.revision.load(Ordering::SeqCst) == revision,
      "订阅更新已被新的请求替代"
    );
    let data = StoredSubscription {
      url: url.trim().into(),
      nodes,
      updated_at: SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64,
    };
    // Write first: failed fetch/parse/write must not erase the previous subscription.
    save_private(
      &state.directory.join("subscription.json"),
      &serde_json::to_vec(&data)?,
    )?;
    let view = SubscriptionView::from(&data);
    *state.subscription.lock().await = data;
    if state.inner.lock().await.status.source.as_deref() == Some("subscription") {
      if let Err(error) = state.stop().await {
        emit_log(&handle, "proxy::error", &error.to_string());
      }
    }
    let _ = handle.emit("proxy-status", &state.inner.lock().await.status);
    Ok::<_, anyhow::Error>(view)
  }
  .await
  .into_ta_result()
}
#[tauri::command]
pub async fn tauri_ping_subscription_nodes(
  state: State<'_, ProxyLoop>,
  node_id: Option<String>,
  node_ids: Option<Vec<String>>,
) -> TAResult<Vec<NodeLatency>> {
  let nodes = state.subscription.lock().await.nodes.clone();
  if let Some(id) = node_id {
    let node = nodes
      .iter()
      .find(|node| node.id == id)
      .ok_or_else(|| anyhow::anyhow!("节点已不存在，请更新订阅"))
      .into_ta_result()?;
    Ok(vec![ping_node(node).await])
  } else {
    let selected = nodes
      .into_iter()
      .filter(|node| node_ids.as_ref().is_none_or(|ids| ids.contains(&node.id)))
      .collect::<Vec<_>>();
    Ok(ping_nodes(&selected).await)
  }
}

#[tauri::command]
pub async fn tauri_check_subscription_node(
  state: State<'_, ProxyLoop>,
  node_id: String,
) -> TAResult<bool> {
  let node = state
    .subscription
    .lock()
    .await
    .nodes
    .iter()
    .find(|node| node.id == node_id && node.unsupported_reason.is_none())
    .cloned();
  let Some(node) = node else {
    return Ok(false);
  };
  Ok(
    tokio::time::timeout(
      std::time::Duration::from_secs(5),
      cloudtun_proxy::check_node(&node),
    )
    .await
    .is_ok_and(|result| result.is_ok()),
  )
}

#[tauri::command]
pub async fn tauri_connect_subscription<R: Runtime>(
  handle: AppHandle<R>,
  state: State<'_, ProxyLoop>,
  node_id: String,
  mode: RoutingMode,
) -> TAResult<()> {
  async {
    let _transition = state.transition.lock().await;
    let node = state
      .subscription
      .lock()
      .await
      .nodes
      .iter()
      .find(|n| n.id == node_id)
      .cloned()
      .ok_or_else(|| anyhow::anyhow!("节点已不存在，请更新订阅"))?;
    anyhow::ensure!(node.unsupported_reason.is_none(), "此节点暂不支持");
    let latency = ping_node(&node).await;
    anyhow::ensure!(
      latency.latency_ms.is_some_and(|ms| ms < 800),
      "节点 TCP 延迟不可用或超过 800 ms，请选择其他节点"
    );
    cloudtun_proxy::check_node(&node)
      .await
      .map_err(|_| anyhow::anyhow!("TCP 可连接，但 VMess 实际转发失败，请换一个节点"))?;
    // Prepare the outbound before stopping an existing connection.
    state.stop().await?;
    let name = node.name.clone();
    let id = node.id.clone();
    let config = state.config.lock().await.clone();
    let proxy = PreparedProxy::bind_node_with_rules(
      node,
      mode,
      &config.listen_host,
      config.port,
      state.geodata.rules.clone(),
    )
    .await?;
    state
      .start(proxy, handle, "subscription", Some(id), name, mode)
      .await
  }
  .await
  .into_ta_result()
}
#[tauri::command]
pub async fn tauri_set_system_proxy<R: Runtime>(
  handle: AppHandle<R>,
  state: State<'_, ProxyLoop>,
  enabled: bool,
) -> TAResult<()> {
  async {
    let _transition = state.transition.lock().await;
    if enabled {
      anyhow::ensure!(state.inner.lock().await.status.running, "请先连接代理节点");
    }
    let path = state.backup();
    let port = state.config.lock().await.port;
    let result = tokio::task::spawn_blocking(move || {
      if enabled {
        system_proxy::enable(&path, port)
      } else {
        system_proxy::restore(&path)
      }
    })
    .await?;
    state.inner.lock().await.status.system_proxy = state.backup().exists();
    let _ = handle.emit("proxy-status", &state.inner.lock().await.status);
    result
  }
  .await
  .into_ta_result()
}

#[tauri::command]
pub async fn tauri_get_proxy_config(state: State<'_, ProxyLoop>) -> TAResult<ProxyConfigView> {
  Ok(state.config.lock().await.view())
}
#[tauri::command]
pub async fn tauri_set_proxy_config<R: Runtime>(
  handle: AppHandle<R>,
  state: State<'_, ProxyLoop>,
  config: ProxyConfig,
) -> TAResult<ProxyConfigView> {
  async {
    config.validate()?;
    let _transition = state.transition.lock().await;
    let old = state.config.lock().await.clone();
    if old == config {
      return Ok(config.view());
    }
    let (listener, system_proxy_enabled, cancel) = {
      let inner = state.inner.lock().await;
      (
        inner
          .running
          .as_ref()
          .map(|running| running.listener.clone()),
        inner.status.system_proxy,
        inner.running.as_ref().map(|running| running.cancel.clone()),
      )
    };
    if let Some(listener) = &listener {
      listener.rebind(&config.listen_host, config.port).await?;
    }
    let apply = async {
      save_private(
        &state.directory.join("proxy-config.json"),
        &serde_json::to_vec(&config)?,
      )?;
      if system_proxy_enabled {
        let backup = state.backup();
        let port = config.port;
        tokio::task::spawn_blocking(move || system_proxy::enable(&backup, port)).await??;
      }
      Ok::<_, anyhow::Error>(())
    }
    .await;
    if let Err(error) = apply {
      if let Some(listener) = &listener {
        if let Err(rollback) = listener.rebind(&old.listen_host, old.port).await {
          emit_log(
            &handle,
            "proxy::error",
            &format!("恢复监听失败：{rollback}"),
          );
        }
      }
      let _ = save_private(
        &state.directory.join("proxy-config.json"),
        &serde_json::to_vec(&old)?,
      );
      if system_proxy_enabled {
        let backup = state.backup();
        let port = old.port;
        let _ = tokio::task::spawn_blocking(move || system_proxy::enable(&backup, port)).await;
      }
      state.inner.lock().await.status.system_proxy = state.backup().exists();
      let _ = handle.emit("proxy-status", &state.inner.lock().await.status);
      emit_log(&handle, "proxy::error", &format!("监听设置未应用：{error}"));
      return Err(error);
    }
    *state.config.lock().await = config.clone();
    emit_log(
      &handle,
      "log::info",
      &format!(
        "监听设置已保存{}：{}:{}",
        if listener.is_some() {
          "并即时生效"
        } else {
          ""
        },
        config.listen_host,
        config.port
      ),
    );
    if let Some(cancel) = cancel {
      let geodata = state.geodata.clone();
      let app = handle.clone();
      let port = config.port;
      tokio::spawn(async move {
        geodata.check(app, port, cancel, false).await;
      });
    }
    Ok::<_, anyhow::Error>(config.view())
  }
  .await
  .into_ta_result()
}

#[tauri::command]
pub fn tauri_geo_status(state: State<'_, ProxyLoop>) -> crate::geodata::GeoStatus {
  state.geodata.status()
}
#[tauri::command]
pub async fn tauri_update_geo<R: Runtime>(
  handle: AppHandle<R>,
  state: State<'_, ProxyLoop>,
) -> TAResult<crate::geodata::GeoStatus> {
  async {
    let (port, cancel) = {
      let _transition = state.transition.lock().await;
      let inner = state.inner.lock().await;
      let running = inner
        .running
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("请先连接代理，再更新 GEO 规则"))?;
      (state.config.lock().await.port, running.cancel.clone())
    };
    emit_log(&handle, "log::geo", "手动下载 GEO 规则更新");
    state
      .geodata
      .clone()
      .check(handle, port, cancel, true)
      .await;
    Ok::<_, anyhow::Error>(state.geodata.status())
  }
  .await
  .into_ta_result()
}

#[tauri::command]
pub async fn tauri_set_routing_mode<R: Runtime>(
  handle: AppHandle<R>,
  state: State<'_, ProxyLoop>,
  mode: RoutingMode,
) -> TAResult<()> {
  let _transition = state.transition.lock().await;
  let mut inner = state.inner.lock().await;
  if let Some(running) = &inner.running {
    running.listener.set_routing_mode(mode);
  }
  inner.status.mode = mode;
  let _ = handle.emit("proxy-status", &inner.status);
  emit_log(
    &handle,
    "log::info",
    match mode {
      RoutingMode::Global => "分流模式已切换为全局代理（保留局域网直连）",
      RoutingMode::BypassChina => "分流模式已切换为绕过大陆与局域网",
    },
  );
  Ok(())
}
