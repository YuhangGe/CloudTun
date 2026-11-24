use anyhow_tauri::{IntoTAResult, TAResult};
use serde::{Deserialize, Serialize};
// use tauri::ipc::{Channel, InvokeResponseBody};
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

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckVpnConnectedResponse {
  pub connected: bool,
}

#[tauri::command]
pub async fn tauri_android_start_vpn<R: Runtime>(
  server_ip: &str,
  cvm_id: &str,
  h: AppHandle<R>,
  state: State<'_, Vpn<R>>,
) -> TAResult<bool> {
  let _ = state;
  crate::proxy::tauri_start_proxy_client(
    h.clone(),
    h.state(),
    server_ip.into(),
    cvm_id.into(),
    cloudtun_proxy::RoutingMode::default(),
    None,
  )
  .await?;
  Ok(true)
}

#[tauri::command]
pub async fn tauri_android_stop_vpn<R: Runtime>(
  h: AppHandle<R>,
  state: State<'_, Vpn<R>>,
) -> TAResult<bool> {
  state.stop_vpn(h)
}

#[tauri::command]
pub async fn tauri_android_request_notification_permission<R: Runtime>(
  h: AppHandle<R>,
  state: State<'_, Vpn<R>>,
) -> TAResult<bool> {
  state.request_notification_permission(h)
}

#[tauri::command]
pub async fn tauri_android_get_vpn_connected<R: Runtime>(
  h: AppHandle<R>,
  state: State<'_, Vpn<R>>,
) -> TAResult<bool> {
  state.get_vpn_connected(h)
}

#[derive(Clone)]
pub struct Vpn<R: Runtime>(PluginHandle<R>);

impl<R: Runtime> Vpn<R> {
  pub fn service_connected(&self) -> anyhow::Result<bool> {
    Ok(
      self
        .0
        .run_mobile_plugin::<CheckVpnConnectedResponse>("getVpnConnected", ())?
        .connected,
    )
  }
  pub fn start_vpn(&self, h: AppHandle<R>, config: String) -> TAResult<bool> {
    let ret = self
      .0
      .run_mobile_plugin::<StartVpnResponse>("startVpn", StartVpnArgs { config });

    match ret {
      Ok(x) => Ok(x.success),
      Err(e) => {
        let msg = e.to_string();
        emit_log(&h, "vpn", &format!("failed startVpn due to: {}", &msg));
        Ok(false)
      }
    }
  }

  pub fn update_routing(&self, config: String) -> anyhow::Result<()> {
    self.0.run_mobile_plugin::<StartVpnResponse>(
      "updateRouting",
      serde_json::json!({ "config": config }),
    )?;
    Ok(())
  }
  pub fn update_failover(&self, config: String) -> anyhow::Result<()> {
    self.0.run_mobile_plugin::<StartVpnResponse>(
      "updateFailover",
      serde_json::json!({ "config": config }),
    )?;
    Ok(())
  }
  pub fn status(&self) -> anyhow::Result<Option<crate::proxy::ProxyStatus>> {
    #[derive(Deserialize)]
    struct Response {
      status: Option<crate::proxy::ProxyStatus>,
    }
    Ok(
      self
        .0
        .run_mobile_plugin::<Response>("getVpnStatus", ())?
        .status,
    )
  }
  pub fn traffic(&self) -> anyhow::Result<Option<cloudtun_proxy::TrafficSnapshot>> {
    #[derive(Deserialize)]
    struct Response {
      traffic: Option<cloudtun_proxy::TrafficSnapshot>,
    }
    Ok(
      self
        .0
        .run_mobile_plugin::<Response>("getVpnTraffic", ())?
        .traffic,
    )
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

  pub fn stop_vpn(&self, h: AppHandle<R>) -> TAResult<bool> {
    let ret = self.0.run_mobile_plugin::<StartVpnResponse>("stopVpn", ());

    match ret {
      Ok(x) => Ok(x.success),
      Err(e) => {
        let msg = e.to_string();
        emit_log(&h, "vpn", &format!("failed stopVpn due to: {}", &msg));
        Ok(false)
      }
    }
  }

  pub fn request_notification_permission(&self, h: AppHandle<R>) -> TAResult<bool> {
    let ret = self
      .0
      .run_mobile_plugin::<StartVpnResponse>("requestNotificationPermission", ());

    match ret {
      Ok(x) => Ok(x.success),
      Err(e) => {
        let msg = e.to_string();
        emit_log(
          &h,
          "vpn",
          &format!("failed requestNotificationPermission due to: {}", &msg),
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
