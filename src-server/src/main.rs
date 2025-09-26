mod context;
mod proxy;
mod routes;

use std::{sync::Arc, time::Duration};

use axum::{
  Router,
  routing::{any, get},
};
use cloudtun_common::{constant::REMOTE_PROXY_PORT, tencent::TencentCloudClient, util::hex2str};

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
  #[arg(long)]
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

  let tencent_client = TencentCloudClient::new(args.secret_id, args.secret_key, args.region);
  let password = get_password(&tencent_client, &args.cvm_name).await;
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
  Auth Token: {}
  Data Password: {}",
    args.token,
    hex2str(&password)
  );

  let context = Arc::new(Context::new(
    args.token,
    password,
    args.cvm_name,
    tencent_client,
  ));

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
        if let Err(e) = destroy_cvm(&context).await {
          eprintln!("failed destroy cvm due to: {e}");
        }
      }
    }
  });
  let _ = tokio::join!(serve_handle, timer_handle);
}

async fn get_password(tx: &TencentCloudClient, cvm_name: &str) -> Vec<u8> {
  let cvm_id = match tx.get_instance_by_name(cvm_name).await {
    Err(_) => {
      eprintln!("cvm not found, use empty password");
      return vec![0; 16];
    }
    Ok(v) => v.id,
  };
  let cvm_id_bytes = cvm_id.as_bytes();
  let len = cvm_id_bytes.len();
  let mut buf = Vec::with_capacity(16);
  for i in 0..16 {
    buf.push(cvm_id_bytes[i % len]);
  }
  buf
}

async fn destroy_cvm(ctx: &Context) -> anyhow::Result<bool> {
  let inst = ctx.tx.get_instance_by_name(&ctx.cvm_name).await?;
  ctx.tx.desroy_instance(&inst.id).await
}
