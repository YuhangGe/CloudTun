mod context;
mod proxy;
mod routes;

use std::{sync::Arc, time::Duration};

use axum::{
  Router,
  routing::{any, get},
};
use cloudtun_common::{constant::REMOTE_PROXY_PORT, tencent::TencentCloudClient};

//allows to split the websocket stream into separate TX and RX branches

use crate::{
  context::Context,
  proxy::proxy_handler,
  routes::{ping_handler, test_handler},
};

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

  /// 腾讯云 SecretId
  #[arg(long)]
  secret_id: String,

  /// 腾讯云 SecretKey
  #[arg(long)]
  secret_key: String,

  /// 腾讯云 Region
  #[arg(short, long)]
  region: String,

  /// 代理服务主机的名称
  #[arg(long)]
  cvm_name: String,
}

#[tokio::main]
async fn main() {
  let args = Args::parse();

  let app = Router::new()
    .route("/ping", get(ping_handler))
    .route("/test", get(test_handler))
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

  let tencent_client = TencentCloudClient::new(args.secret_id, args.secret_key, args.region);
  let context = Arc::new(Context::new(args.token, args.cvm_name, tencent_client));

  let serve_handle = axum::serve(
    listener,
    app.with_state(context.clone()).into_make_service(),
  );

  let timer_handle = tokio::spawn(async move {
    let mut int = tokio::time::interval(Duration::from_secs(60));
    int.tick().await;
    loop {
      int.tick().await;
      if context.is_ping_expired().await {
        println!("ping expired, bye!");
        destroy_cvm(&context).await;
      }
    }
  });
  let _ = tokio::join!(serve_handle, timer_handle);
}

async fn destroy_cvm(ctx: &Context) {
  let ret = match ctx.tx.describe_instances().await {
    Err(e) => {
      eprintln!("failed call describe_instances: {e}");
      return;
    }
    Ok(ret) => ret,
  };
  let Some(inst) = ret.iter().find(|inst| inst.name.eq(&ctx.cvm_name)) else {
    eprintln!("{} not found.", &ctx.cvm_name);
    return;
  };
  if let Err(e) = ctx.tx.desroy_instance(&inst.id).await {
    eprintln!("failed call destroy_instance: {e}");
  }
}
