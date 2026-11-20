#![cfg(target_os = "android")]

use cloudtun_proxy::{ProxyConnector, VpnConfig};
use jni::{
  EnvUnowned,
  errors::ThrowRuntimeExAndDefault,
  objects::{JClass, JString},
  sys::{jint, jstring},
};
use std::{
  os::fd::{AsRawFd, BorrowedFd, OwnedFd},
  sync::Mutex,
  time::Duration,
};
use tokio_util::sync::CancellationToken;

struct Session {
  runtime: tokio::runtime::Runtime,
  tun_fd: OwnedFd,
  connector: ProxyConnector,
  failover: std::sync::Arc<crate::failover::Failover>,
  cancel: CancellationToken,
}
static SESSION: Mutex<Option<Session>> = Mutex::new(None);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn Java_com_cloudtun_app_CloudTunVpn_start(
  mut env: EnvUnowned,
  _: JClass,
  fd: jint,
  mtu: jint,
  config: JString,
) -> jint {
  android_logger::init_once(
    android_logger::Config::default()
      .with_tag("cloudtun-vpn")
      .with_max_level(log::LevelFilter::Info),
  );
  let result = (|| -> anyhow::Result<()> {
    anyhow::ensure!(
      fd >= 0 && (576..=65535).contains(&mtu),
      "invalid TUN descriptor or MTU"
    );
    let config: VpnConfig =
      serde_json::from_slice(&std::fs::read(java_string(&mut env, &config))?)?;
    let policy = config.failover.clone();
    let connector = config.connector()?;
    let failover = crate::failover::Failover::new(connector.clone(), policy);
    let mut state = SESSION
      .lock()
      .map_err(|_| anyhow::anyhow!("VPN lock poisoned"))?;
    anyhow::ensure!(state.is_none(), "VPN is already running");
    let owned = (unsafe { BorrowedFd::borrow_raw(fd) }).try_clone_to_owned()?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
      .worker_threads(2)
      .enable_all()
      .build()?;
    let device = {
      let _enter = runtime.enter();
      let mut config = tun::Configuration::default();
      // The session owns this duplicate explicitly, even if a spawned IP stack
      // task retains the async device during shutdown.
      config.raw_fd(owned.as_raw_fd()).close_fd_on_drop(false);
      tun::create_as_async(&config)?
    };
    let cancel = CancellationToken::new();
    if let Some(server) = connector.cloud_endpoint() {
      let heartbeat_cancel = cancel.clone();
      runtime.spawn(async move {
        crate::start_ping_interval(&server, &heartbeat_cancel).await;
      });
    }
    runtime.spawn(failover.clone().run(cancel.clone()));
    let task_cancel = cancel.clone();
    let task_connector = connector.clone();
    runtime.spawn(async move {
      let result = crate::start_run_vpn_with_connector(
        device,
        mtu as u16,
        task_connector,
        task_cancel.clone(),
        |kind, message| {
          log::info!("[{kind}] {message}");
        },
      )
      .await;
      if let Err(error) = result {
        log::error!("VPN stopped: {error}");
      }
      task_cancel.cancel();
    });
    *state = Some(Session {
      runtime,
      tun_fd: owned,
      connector,
      failover,
      cancel,
    });
    Ok(())
  })();
  match result {
    Ok(()) => 0,
    Err(error) => {
      log::error!("Cannot start VPN: {error}");
      -1
    }
  }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn Java_com_cloudtun_app_CloudTunVpn_stop(_env: EnvUnowned, _: JClass) {
  let session = SESSION.lock().ok().and_then(|mut state| state.take());
  if let Some(session) = session {
    session.cancel.cancel();
    session.runtime.shutdown_timeout(Duration::from_secs(3));
    drop(session.tun_fd);
  }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn Java_com_cloudtun_app_CloudTunVpn_updateRouting(
  mut env: EnvUnowned,
  _: JClass,
  config: JString,
) -> jint {
  let config = std::fs::read(java_string(&mut env, &config))
    .ok()
    .and_then(|bytes| serde_json::from_slice::<VpnConfig>(&bytes).ok());
  let Some(config) = config else {
    return -1;
  };
  if let Some(session) = SESSION
    .lock()
    .ok()
    .as_deref()
    .and_then(|state| state.as_ref())
  {
    session.connector.update_routing(config);
    return 0;
  }
  -1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn Java_com_cloudtun_app_CloudTunVpn_stats(
  mut env: EnvUnowned,
  _: JClass,
) -> jstring {
  let stats = SESSION.lock().ok().and_then(|state| {
    state
      .as_ref()
      .filter(|session| !session.cancel.is_cancelled())
      .map(|session| session.connector.traffic_snapshot())
  });
  let value = serde_json::to_string(&stats).unwrap_or_else(|_| "null".into());
  env
    .with_env(|env| env.new_string(value).map(|value| value.into_raw()))
    .resolve::<ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn Java_com_cloudtun_app_CloudTunVpn_updateFailover(
  mut env: EnvUnowned,
  _: JClass,
  config: JString,
) -> jint {
  let policy = std::fs::read(java_string(&mut env, &config))
    .ok()
    .and_then(|bytes| serde_json::from_slice::<cloudtun_proxy::FailoverConfig>(&bytes).ok());
  let Some(policy) = policy else {
    return -1;
  };
  if let Some(session) = SESSION
    .lock()
    .ok()
    .as_deref()
    .and_then(|state| state.as_ref())
  {
    session.failover.update(policy);
    return 0;
  }
  -1
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn Java_com_cloudtun_app_CloudTunVpn_status(
  mut env: EnvUnowned,
  _: JClass,
) -> jstring {
  let status = SESSION.lock().ok().and_then(|state| {
    state
      .as_ref()
      .filter(|session| !session.cancel.is_cancelled())
      .map(|session| session.failover.snapshot())
  });
  let value = serde_json::to_string(&status).unwrap_or_else(|_| "null".into());
  env
    .with_env(|env| env.new_string(value).map(|value| value.into_raw()))
    .resolve::<ThrowRuntimeExAndDefault>()
}

fn java_string(env: &mut EnvUnowned, input: &JString) -> String {
  env
    .with_env(|_| -> Result<String, jni::errors::Error> { Ok(input.to_string()) })
    .resolve::<ThrowRuntimeExAndDefault>()
}
