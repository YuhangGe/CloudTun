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
  pub server_ip: String,
  pub token: String,
  pub cvm_id: String,
  pub proxy_apps: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartVpnResponse {
  pub success: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckVpnConnectedResponse {
  pub connected: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ListAllAppsResponseItem {
  pub name: String,
  pub icon: String,
  pub id: String,
}
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ListAllAppsResponse {
  pub apps: Vec<ListAllAppsResponseItem>,
}

#[tauri::command]
pub async fn tauri_android_start_vpn<R: Runtime>(
  server_ip: &str,
  token: &str,
  cvm_id: &str,
  proxy_apps: &str,
  h: AppHandle<R>,
  state: State<'_, Vpn<R>>,
) -> TAResult<bool> {
  state.start_vpn(
    h,
    server_ip.into(),
    token.into(),
    cvm_id.into(),
    proxy_apps.into(),
  )
}

#[tauri::command]
pub async fn tauri_android_list_all_apps<R: Runtime>(
  h: AppHandle<R>,
  state: State<'_, Vpn<R>>,
) -> TAResult<Vec<ListAllAppsResponseItem>> {
  state.list_all_apps(h)
}

#[tauri::command]
pub async fn tauri_android_get_vpn_connected<R: Runtime>(
  h: AppHandle<R>,
  state: State<'_, Vpn<R>>,
) -> TAResult<bool> {
  state.get_vpn_connected(h)
}

pub struct Vpn<R: Runtime>(PluginHandle<R>);

impl<R: Runtime> Vpn<R> {
  pub fn start_vpn(
    &self,
    h: AppHandle<R>,
    server_ip: String,
    token: String,
    cvm_id: String,
    proxy_apps: String,
  ) -> TAResult<bool> {
    let ret = self.0.run_mobile_plugin::<StartVpnResponse>(
      "startVpn",
      StartVpnArgs {
        proxy_apps,
        server_ip,
        token,
        cvm_id,
      },
    );

    match ret {
      Ok(x) => Ok(x.success),
      Err(e) => {
        let msg = e.to_string();
        emit_log(&h, "vpn", &format!("failed startVpn due to: {}", &msg));
        Ok(false)
      }
    }
  }

  pub fn list_all_apps(&self, h: AppHandle<R>) -> TAResult<Vec<ListAllAppsResponseItem>> {
    let ret = self
      .0
      .run_mobile_plugin::<ListAllAppsResponse>("listAllApps", ());
    match ret {
      Ok(x) => Ok(x.apps),
      Err(e) => {
        let msg = e.to_string();
        emit_log(&h, "vpn", &format!("failed listAllApps due to: {}", &msg));
        Ok(vec![])
      }
    }
  }

  pub fn get_vpn_connected(&self, h: AppHandle<R>) -> TAResult<bool> {
    let ret = self
      .0
      .run_mobile_plugin::<CheckVpnConnectedResponse>("getVpnConnected", ());
    match ret {
      Ok(x) => Ok(x.connected),
      Err(e) => {
        let msg = e.to_string();
        emit_log(
          &h,
          "vpn",
          &format!("failed getVpnConnected due to: {}", &msg),
        );
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

pub fn tauri_android_init_plugin<R: Runtime>() -> TauriPlugin<R> {
  Builder::<R>::new("vpn")
    .setup(|app, api| {
      let handle = api
        .register_android_plugin("com.cloudtun.app", "CloudTunPlugin")
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
