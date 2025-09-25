#![cfg(target_os = "android")]

use jni::{
  JNIEnv,
  objects::{JClass, JString},
  sys::{jchar, jint},
};

use crate::{start_ping_interval, start_run_vpn};

static TUN_QUIT: std::sync::Mutex<Option<tokio_util::sync::CancellationToken>> =
  std::sync::Mutex::new(None);

/// Start cloudtun
#[unsafe(no_mangle)]
pub unsafe extern "C" fn Java_com_cloudtun_app_CloudTunVpn_run(
  mut env: JNIEnv,
  _clazz: JClass,
  tun_fd: jint,
  mtu: jchar,
  server_ip: JString,
  token: JString,
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
  let Ok(token) = get_java_string(&mut env, &token) else {
    log::error!("failed get jstring");
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

  let Ok(rt) = tokio::runtime::Runtime::new() else {
    log::error!("failed to create tokio runtime with");
    return -3;
  };

  log::info!("XXX will start tokio vpn");
  let res = rt.block_on(async move {
    let ping_ip = server_ip.clone();
    let ping_token = token.clone();
    let ping_cancel_token = shutdown_token.clone();
    let h2 = tokio::spawn(async move {
      start_ping_interval(&ping_ip, &ping_token, &ping_cancel_token).await;
      0
    });
    let h1 = tokio::spawn(async move {
      let mut config = tun::Configuration::default();
      config.raw_fd(tun_fd);
      // println!("XXX mtu {mtu} {tun_fd}");
      let shutdown_token2 = shutdown_token.clone();
      let Ok(device) = tun::create_as_async(&config) else {
        log::error!("failed create tun device");
        shutdown_token2.cancel();
        return -1;
      };
      let server_addr = (server_ip, 24816, token);
      let log_fn = |log_type: &str, log_message: &str| {
        log::info!("[{log_type}] {log_message}");
      };

      match start_run_vpn(device, mtu, server_addr, shutdown_token, log_fn).await {
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
pub unsafe extern "C" fn Java_com_cloudtun_app_CloudTunVpn_stop(_env: JNIEnv, _: JClass) -> jint {
  if let Ok(mut lock) = TUN_QUIT.lock() {
    if let Some(shutdown_token) = lock.take() {
      shutdown_token.cancel();
      return 0;
    }
  }
  -1
}

fn get_java_string(env: &mut JNIEnv, string: &JString) -> anyhow::Result<String> {
  Ok(env.get_string(string)?.into())
}
