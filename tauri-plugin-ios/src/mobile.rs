use serde::de::DeserializeOwned;
use tauri::{
  plugin::{PluginApi, PluginHandle},
  AppHandle, Runtime,
};

use crate::models::*;

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_ios);

// initializes the Kotlin or Swift plugin classes
pub fn init<R: Runtime, C: DeserializeOwned>(
  _app: &AppHandle<R>,
  api: PluginApi<R, C>,
) -> crate::Result<Ios<R>> {
  #[cfg(target_os = "ios")]
  let handle = api.register_ios_plugin(init_plugin_ios)?;
  Ok(Ios(handle))
}

/// Access to the ios APIs.
pub struct Ios<R: Runtime>(PluginHandle<R>);

impl<R: Runtime> Ios<R> {
  pub fn start_proxy(&self, payload: PingRequest) -> crate::Result<PingResponse> {
    self
      .0
      .run_mobile_plugin("startProxy", payload)
      .map_err(Into::into)
  }
  pub fn stop_proxy(&self, payload: PingRequest) -> crate::Result<PingResponse> {
    self
      .0
      .run_mobile_plugin("stopProxy", payload)
      .map_err(Into::into)
  }
}
