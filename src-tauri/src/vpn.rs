use anyhow_tauri::TAResult;
use serde::{Deserialize, Serialize};
use tauri::plugin::{Builder, PluginHandle, TauriPlugin};
use tauri::{AppHandle, Manager, Runtime, State};

use crate::util::emit_log;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartVpnArgs {
  pub config: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartVpnResponse {
  pub success: bool,
}

#[tauri::command]
pub async fn tauri_start_vpn<R: Runtime>(
  config: &str,
  h: AppHandle<R>,
  state: State<'_, Vpn<R>>,
) -> TAResult<bool> {
  state.start_vpn(h, config.into())
}

pub struct Vpn<R: Runtime>(PluginHandle<R>);

impl<R: Runtime> Vpn<R> {
  pub fn start_vpn(&self, h: AppHandle<R>, config: String) -> TAResult<bool> {
    let ret = self
      .0
      .run_mobile_plugin::<StartVpnResponse>("startVpn", StartVpnArgs { config: config });

    match ret {
      Ok(x) => Ok(x.success),
      Err(e) => {
        let msg = e.to_string();
        emit_log(&h, "vpn", &format!("failed startVpn due to: {}", &msg));
        Ok(false)
      }
    }
  }
}

pub fn init_tauri_vpn<R: Runtime>() -> TauriPlugin<R> {
  Builder::<R>::new("vpn")
    .setup(|app, api| {
      let handle = api
        .register_android_plugin("com.cloudv2ray.app", "CloudV2RayPlugin")
        .unwrap();

      let vpn = Vpn(handle);
      app.manage(vpn);
      Ok(())
    })
    .build()
}
