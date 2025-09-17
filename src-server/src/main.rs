mod proxy;
mod routes;

use axum::{
  Router,
  routing::{any, get},
};
use cloudtun_common::REMOTE_PROXY_PORT;

//allows to split the websocket stream into separate TX and RX branches

use crate::{proxy::proxy_handler, routes::home_handler};

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

  let app = Router::new()
    .route("/", get(home_handler))
    .route("/ws", any(proxy_handler));

  let ip = args.ip.unwrap_or("0.0.0.0".to_string());
  let port = args.port;
  let listener = tokio::net::TcpListener::bind((ip.clone(), port))
    .await
    .unwrap();

  println!("cloudtun server listening at {ip}:{port}");

  axum::serve(listener, app.into_make_service())
    .await
    .unwrap();
}
