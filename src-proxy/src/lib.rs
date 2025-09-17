mod handler;
mod proxy;
mod route;
mod tunnel;

use std::{
  sync::{Arc, RwLock},
  thread,
};

use crate::handler::ProxyHandler;

pub use route::MatchType;

lazy_static::lazy_static! {
  static ref PROXY_HANDLER: Arc<RwLock<ProxyHandler>> = Arc::new(RwLock::new(ProxyHandler::new()));
}

pub struct StartProxyArgs {
  pub server_ip: String,
  pub server_port: u16,
  pub local_ip: String,
  pub local_port: u16,
  pub default_rule: MatchType,
  pub rules_config_file: Option<String>,
}

pub fn start_proxy(args: StartProxyArgs) {
  let handle = thread::spawn(move || {
    {
      PROXY_HANDLER.write().unwrap().init_rt(args);
    }
    PROXY_HANDLER.read().unwrap().start_loop();
  });
  handle.join().unwrap();
  PROXY_HANDLER.write().unwrap().deinit_rt();
}

pub fn stop_proxy() {
  PROXY_HANDLER.read().unwrap().stop_loop();
}
