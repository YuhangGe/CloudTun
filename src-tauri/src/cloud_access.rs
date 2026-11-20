use anyhow_tauri::{IntoTAResult, TAResult};
use cloudtun_common::tls::{private_write, CloudEndpoint};
use std::{collections::BTreeMap, path::PathBuf, sync::Mutex};
use tauri::State;
pub struct CloudAccess {
  directory: PathBuf,
  tokens: Mutex<BTreeMap<String, String>>,
}
impl CloudAccess {
  pub fn new(directory: PathBuf) -> anyhow::Result<Self> {
    let path = directory.join("cloud-access.json");
    let tokens = match std::fs::read(path) {
      Ok(bytes) => serde_json::from_slice(&bytes)?,
      Err(error) if error.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
      Err(error) => return Err(error.into()),
    };
    Ok(Self {
      directory,
      tokens: Mutex::new(tokens),
    })
  }
  pub fn endpoint(&self, host: &str, identity: &str) -> anyhow::Result<CloudEndpoint> {
    let tokens = self
      .tokens
      .lock()
      .map_err(|_| anyhow::anyhow!("访问凭据存储不可用"))?;
    let token = tokens
      .get(identity)
      .ok_or_else(|| anyhow::anyhow!("缺少该实例的 WSS 访问凭据，请重新初始化或重建云主机"))?;
    Ok(CloudEndpoint::new(
      host.into(),
      24816,
      identity.into(),
      token.clone(),
      self.directory.join("cloud-trust"),
    )?)
  }
}
#[tauri::command]
pub fn tauri_new_cloud_access_token() -> String {
  rand::random::<[u8; 32]>()
    .iter()
    .map(|byte| format!("{byte:02x}"))
    .collect()
}
#[tauri::command]
pub fn tauri_has_cloud_access(state: State<'_, CloudAccess>, instance_id: String) -> bool {
  state
    .tokens
    .lock()
    .is_ok_and(|tokens| tokens.contains_key(&instance_id))
}
#[tauri::command]
pub fn tauri_remember_cloud_access(
  state: State<'_, CloudAccess>,
  instance_id: String,
  token: String,
) -> TAResult<()> {
  (|| -> anyhow::Result<()> {
    CloudEndpoint::new(
      "127.0.0.1".into(),
      24816,
      instance_id.clone(),
      token.clone(),
      state.directory.clone(),
    )?;
    let mut tokens = state
      .tokens
      .lock()
      .map_err(|_| anyhow::anyhow!("访问凭据存储不可用"))?;
    if let Some(existing) = tokens.get(&instance_id) {
      anyhow::ensure!(
        existing == &token,
        "该实例的访问凭据已变化，请重新初始化云主机"
      );
      return Ok(());
    }
    let mut next = tokens.clone();
    next.insert(instance_id, token);
    private_write(
      &state.directory.join("cloud-access.json"),
      &serde_json::to_vec(&next)?,
    )?;
    *tokens = next;
    Ok(())
  })()
  .into_ta_result()
}

#[tauri::command]
pub async fn tauri_ping_cloud_server(
  state: State<'_, CloudAccess>,
  ip: String,
  token: String,
) -> TAResult<bool> {
  let server = state.endpoint(&ip, &token).into_ta_result()?;
  match cloudtun_common::ping::ping_cloudtun_proxy_server(&server).await {
    Ok(ready) => Ok(ready),
    Err(error) if error.to_string().contains("证书") || error.to_string().contains("信任") => {
      Err(anyhow::anyhow!("WSS 信任校验失败：{error}")).into_ta_result()
    }
    Err(_) => Ok(false),
  }
}
