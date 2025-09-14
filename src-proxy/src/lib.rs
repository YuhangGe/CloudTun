mod handler;
mod proxy;
mod router;

use std::sync::{Arc, Mutex};

use crate::handler::ProxyHandler;

lazy_static::lazy_static! {
  static ref PROXY_HANDLER: Arc<Mutex<ProxyHandler>> = Arc::new(Mutex::new(ProxyHandler::new()));
}

pub fn start_proxy() {
  PROXY_HANDLER.lock().unwrap().start_loop();
}

pub fn stop_proxy() {
  PROXY_HANDLER.lock().unwrap().stop_loop();
}
