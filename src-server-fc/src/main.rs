mod context;
mod proxy;
mod routes;

use std::sync::Arc;

use axum::{
  Router,
  routing::{any, get},
};
use cloudtun_common::{constant::REMOTE_PROXY_PORT, util::hex2str};

//allows to split the websocket stream into separate TX and RX branches

use crate::{context::Context, proxy::proxy_handler, routes::test_handler};

use clap::Parser;

/// CloudTun - 超轻量网络代理服务器
#[derive(Parser, Debug)]
#[command(version)]
struct Args {
  /// 代理服务监听 ip，默认 0.0.0.0
  #[arg(short, long)]
  ip: Option<String>,

  /// 代理服务监听端口，默认 24816
  #[arg(short, long, default_value_t = REMOTE_PROXY_PORT)]
  port: u16,
}

#[tokio::main]
async fn main() {
  let args = Args::parse();

  let (token, password) = get_password();
  let app = Router::new()
    .route("/test", get(test_handler))
    .route("/ws", any(proxy_handler));

  let ip = args.ip.unwrap_or("0.0.0.0".to_string());
  let port = args.port;
  let listener = tokio::net::TcpListener::bind((ip.clone(), port))
    .await
    .unwrap();

  println!(
    "CloudTun Server Listening at {ip}:{port}
  Auth Token: {}
  Data Password: {}",
    token,
    hex2str(&password)
  );

  let context = Arc::new(Context::new(token, password));

  let serve_handle = axum::serve(
    listener,
    app.with_state(context.clone()).into_make_service(),
  );

  let _ = serve_handle.await;
}

fn get_password() -> (String, Vec<u8>) {
  let token = "1234567812345678".to_string();
  let token_bytes = token.as_bytes();
  let len = token_bytes.len();
  let mut buf = Vec::with_capacity(16);
  for i in 0..16 {
    buf.push(token_bytes[i % len]);
  }
  (token, buf)
}
