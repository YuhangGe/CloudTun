#![cfg(target_os = "android")]

use jni::{
  EnvUnowned,
  errors::ThrowRuntimeExAndDefault,
  objects::{JClass, JString},
  sys::jint,
};

use crate::{start_ping_interval, start_run_vpn};
use std::os::fd::{BorrowedFd, IntoRawFd};

struct RunningGuard;
impl Drop for RunningGuard {
  fn drop(&mut self) {
    if let Ok(mut state) = TUN_QUIT.lock() {
      *state = None;
    }
  }
}

static TUN_QUIT: std::sync::Mutex<Option<tokio_util::sync::CancellationToken>> =
  std::sync::Mutex::new(None);

/// Start cloudtun
#[unsafe(no_mangle)]
pub unsafe extern "C" fn Java_com_cloudtun_app_CloudTunVpn_run(
  mut env: EnvUnowned,
  _clazz: JClass,
  tun_fd: jint,
  mtu: jint,
  server_ip: JString,
  cvm_id: JString,
  access_token: JString,
  trust_dir: JString,
) -> jint {
  android_logger::init_once(
    android_logger::Config::default()
      .with_tag("cloudtun")
      .with_max_level(log::LevelFilter::Trace), // .with_filter(filter),
  );

  let Ok(server_ip) = get_java_string(&mut env, &server_ip) else {
    log::error!("failed get jstring");
    return -1;
  };
  // let Ok(token) = get_java_string(&mut env, &token) else {
  //   log::error!("failed get jstring");
  //   return -1;
  // };
  let Ok(cvm_id) = get_java_string(&mut env, &cvm_id) else {
    log::error!("failed get jstring");
    return -1;
  };

  if cvm_id.is_empty() || server_ip.is_empty() || tun_fd < 0 || !(576..=65535).contains(&mtu) {
    return -1;
  }
  let mtu = mtu as u16;
  // Kotlin retains ownership of its ParcelFileDescriptor. Duplicate it before
  // starting asynchronous work so either side can close its own descriptor safely.
  let Ok(owned_fd) = (unsafe { BorrowedFd::borrow_raw(tun_fd) }).try_clone_to_owned() else {
    return -1;
  };
  let Ok(access_token) = get_java_string(&mut env, &access_token) else {
    return -1;
  };
  let Ok(trust_dir) = get_java_string(&mut env, &trust_dir) else {
    return -1;
  };
  let Ok(server) = cloudtun_common::tls::CloudEndpoint::new(
    server_ip.clone(),
    24816,
    cvm_id.clone(),
    access_token,
    trust_dir.into(),
  ) else {
    return -1;
  };

  let shutdown_token = tokio_util::sync::CancellationToken::new();
  if let Ok(mut lock) = TUN_QUIT.lock() {
    if lock.is_some() {
      log::error!("cloudtun_vpn already started");
      return -1;
    }
    *lock = Some(shutdown_token.clone());
  } else {
    log::error!("failed to lock cloudtun_vpn quit token");
    return -2;
  }

  let _running_guard = RunningGuard;
  let Ok(rt) = tokio::runtime::Runtime::new() else {
    log::error!("failed to create tokio runtime with");
    return -3;
  };

  log::info!(
    "XXX will start tokio vpn with ip {}, token: {}",
    server_ip,
    cvm_id
  );
  let res = rt.block_on(async move {
    let ping_server = server.clone();
    let ping_cancel_token = shutdown_token.clone();
    let h2 = tokio::spawn(async move {
      start_ping_interval(&ping_server, &ping_cancel_token).await;
      0
    });
    let h1 = tokio::spawn(async move {
      let mut config = tun::Configuration::default();
      config.raw_fd(owned_fd.into_raw_fd());
      // println!("XXX mtu {mtu} {tun_fd}");
      let shutdown_token2 = shutdown_token.clone();
      let Ok(device) = tun::create_as_async(&config) else {
        log::error!("failed create tun device");
        shutdown_token2.cancel();
        return -1;
      };
      let log_fn = |log_type: &str, log_message: &str| {
        log::info!("[{log_type}] {log_message}");
      };

      match start_run_vpn(device, mtu, server, shutdown_token, log_fn).await {
        Ok(_) => 0,
        Err(e) => {
          log::error!("failed start_run_vpn: {e}");
          shutdown_token2.cancel();
          -1
        }
      }
    });
    tokio::select! {
      a = h1 => match a {
        Err(_) => -1,
        Ok(n) => n
      },
      b = h2 => match b {
        Err(_) => -1,
        Ok(n) => n,
      }
    }
  });
  res
}

/// Shutdown cloudtun
#[unsafe(no_mangle)]
pub unsafe extern "C" fn Java_com_cloudtun_app_CloudTunVpn_stop(
  _env: EnvUnowned,
  _: JClass,
) -> jint {
  if let Ok(lock) = TUN_QUIT.lock() {
    if let Some(shutdown_token) = lock.as_ref() {
      shutdown_token.cancel();
      return 0;
    }
  }
  -1
}

fn get_java_string(env: &mut EnvUnowned, input: &JString) -> anyhow::Result<String> {
  let v = env.with_env(|_| -> Result<String, jni::errors::Error> { Ok(input.to_string()) });
  let v = v.resolve::<ThrowRuntimeExAndDefault>();
  Ok(v)
}
