use std::os::fd::RawFd;

use anyhow_tauri::TAResult;
use serde::{Deserialize, Serialize};
use tauri::ipc::{Channel, InvokeResponseBody};
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

// #[derive(Serialize)]
// #[serde(rename_all = "camelCase")]
// pub struct EventHandler {
//   pub handler: Channel,
// }
// #[derive(serde::Deserialize)]
// struct Event {
//   config_str: String,
//   tun_fd: RawFd,
// }

pub fn init_tauri_vpn<R: Runtime>() -> TauriPlugin<R> {
  Builder::<R>::new("vpn")
    .setup(|app, api| {
      let handle = api
        .register_android_plugin("com.cloudv2ray.app", "CloudV2RayPlugin")
        .unwrap();

      // handle.run_mobile_plugin(
      //   "setEventHandler",
      //   imp::EventHandler {
      //     handler: Channel::new(move |event| match event {
      //       InvokeResponseBody::Json(payload) => {
      //         use tun2socks::{main_from_str, quit};
      //         serde_json::from_str::<Event>(&payload).ok().map(|payload| {
      //           if payload.config_str.is_empty() {
      //             quit();
      //           } else {
      //             main_from_str(&payload.config_str, payload.tun_fd);
      //           }
      //         })
      //       }
      //       _ => (),
      //     }),
      //   },
      // );

      let vpn = Vpn(handle);
      app.manage(vpn);
      Ok(())
    })
    .build()
}
