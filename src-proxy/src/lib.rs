mod handler;
mod proxy;
mod route;
mod tunnel;

use std::{
  sync::{Arc, RwLock},
  thread,
};

use crate::handler::ProxyHandler;

lazy_static::lazy_static! {
    static ref PROXY_HANDLER: Arc<RwLock<ProxyHandler>> = Arc::new(RwLock::new(ProxyHandler::new()));
}

pub fn start_proxy(rules: Option<String>) {
  let handle = thread::spawn(move || {
    {
      PROXY_HANDLER.write().unwrap().init_rt(rules);
    }
    PROXY_HANDLER.read().unwrap().start_loop();
  });
  handle.join().unwrap();
  PROXY_HANDLER.write().unwrap().deinit_rt();
}

pub fn stop_proxy() {
  PROXY_HANDLER.read().unwrap().stop_loop();
}
