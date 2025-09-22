#![cfg(target_os = "android")]

use jni::{
  JNIEnv,
  objects::{JClass, JString},
  sys::{jboolean, jchar, jint},
};

use crate::start_run_vpn;

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
  // let dns = dns_strategy.try_into().unwrap();
  // let verbosity = verbosity.try_into().unwrap();
  // let filter_str = &format!("off,tun2proxy={verbosity}");
  // let filter = android_logger::FilterBuilder::new()
  //   .parse(filter_str)
  //   .build();
  android_logger::init_once(
    android_logger::Config::default()
      .with_tag("cloudtun")
      .with_max_level(log::LevelFilter::Trace), // .with_filter(filter),
  );

  let Some(server_ip) = get_java_string(&mut env, &server_ip) else {
    log::error!("failed get jstring");
    return -1;
  };
  let Some(token) = get_java_string(&mut env, &token) else {
    log::error!("failed get jstring");
    return -1;
  };

  let mut config = tun::Configuration::default();
  config.raw_fd(tun_fd);
  let Some(device) = tun::create_as_async(&config) else {
    log::error!("failed create tun device");
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

  let Ok(rt) = tokio::runtime::Builder::new_multi_thread()
    .enable_all()
    .build()
  else {
    log::error!("failed to create tokio runtime with");
    return -3;
  };
  let res = rt.block_on(async move {
    let server_addr = (server_ip, 24816, token);
    let log_fn = |log_type: &str, log_message: &str| {
      log::info!("[{log_type}] {log_message}");
    };

    let ret = start_run_vpn(device, mtu, server_addr, shutdown_token, log_fn).await;

    ret
  });
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
