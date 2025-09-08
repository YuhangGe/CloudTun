use serde::de::DeserializeOwned;
use tauri::{
    plugin::{PluginApi, PluginHandle},
    AppHandle, Runtime,
};

use crate::models::*;

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_vpn);

// initializes the Kotlin or Swift plugin classes
pub fn init<R: Runtime, C: DeserializeOwned>(
    _app: &AppHandle<R>,
    api: PluginApi<R, C>,
) -> crate::Result<Vpn<R>> {
    #[cfg(target_os = "android")]
    let handle = api.register_android_plugin("com.cloudv2ray.vpn", "ExamplePlugin")?;
    #[cfg(target_os = "ios")]
    let handle = api.register_ios_plugin(init_plugin_vpn)?;
    Ok(Vpn(handle))
}

/// Access to the vpn APIs.
pub struct Vpn<R: Runtime>(PluginHandle<R>);

impl<R: Runtime> Vpn<R> {
    pub fn start_vpn(&self, payload: PingRequest) -> crate::Result<PingResponse> {
        self.0
            .run_mobile_plugin("startVpn", payload)
            .map_err(Into::into)
    }
}
