mod context;
mod proxy;
mod routes;
mod tencent;

use std::{process, sync::Arc, time::Duration};

use axum::{
  Router,
  routing::{any, get},
};
use cloudtun_common::constant::REMOTE_PROXY_PORT;

//allows to split the websocket stream into separate TX and RX branches

use crate::{context::Context, proxy::proxy_handler, routes::ping_handler, tencent::TencentSDK};

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

  /// 客户端连接时的鉴权 Token
  #[arg(short, long)]
  token: String,
}

#[tokio::main]
async fn main() {
  let args = Args::parse();

  let app = Router::new()
    .route("/ping", get(ping_handler))
    .route("/ws", any(proxy_handler));

  let ip = args.ip.unwrap_or("0.0.0.0".to_string());
  let port = args.port;
  let listener = tokio::net::TcpListener::bind((ip.clone(), port))
    .await
    .unwrap();

  println!(
    "CloudTun Server Listening at {ip}:{port}
  Auth Token: {}",
    args.token
  );

  let context = Arc::new(Context::new(args.token));
  let context2 = context.clone();
  let serve_handle = axum::serve(listener, app.with_state(context).into_make_service());

  let tencent_client = TencentSDK::new("".into(), "".into());
  let timer_handle = tokio::spawn(async move {
    let mut int = tokio::time::interval(Duration::from_secs(60));
    int.tick().await;
    loop {
      int.tick().await;
      if context2.is_ping_expired().await {
        let _ = tencent_client.describe_instances("ap-chengdu").await;
        println!("ping expired, bye!");
        process::exit(0);
      }
    }
  });
  let _ = tokio::join!(serve_handle, timer_handle);
}
