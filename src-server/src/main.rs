use clap::Parser;
use cloudtun_common::{constant::REMOTE_PROXY_PORT, tencent::TencentCloudClient};
use cloudtun_server::{context::Context, tls::Identity};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
#[derive(Parser)]
#[command(version, about = "CloudTun WSS proxy server (TOFU protocol v3)")]
struct Args {
  #[arg(short, long, default_value = "0.0.0.0")]
  ip: String,
  #[arg(short, long, default_value_t = REMOTE_PROXY_PORT)]
  port: u16,
  #[arg(long, default_value = ".cloudtun-server")]
  data_dir: PathBuf,
  #[arg(long, env = "CLOUDTUN_ACCESS_TOKEN", hide_env_values = true)]
  access_token: Option<String>,
  /// Run locally without Tencent credentials or automatic cloud reclamation.
  #[arg(long)]
  standalone: bool,
  #[arg(long, env = "CLOUDTUN_SECRET_ID", hide_env_values = true)]
  secret_id: Option<String>,
  #[arg(long, env = "CLOUDTUN_SECRET_KEY", hide_env_values = true)]
  secret_key: Option<String>,
  #[arg(short, long)]
  region: Option<String>,
  #[arg(long)]
  cvm_name: Option<String>,
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
  let args = Args::parse();
  let tx = if args.standalone {
    None
  } else {
    Some(TencentCloudClient::new(
      args
        .secret_id
        .ok_or_else(|| anyhow::anyhow!("missing --secret-id"))?,
      args
        .secret_key
        .ok_or_else(|| anyhow::anyhow!("missing --secret-key"))?,
      args
        .region
        .ok_or_else(|| anyhow::anyhow!("missing --region"))?,
    ))
  };
  anyhow::ensure!(
    args.standalone || args.cvm_name.is_some(),
    "missing --cvm-name"
  );
  let identity = Identity::load_or_create(&args.data_dir, args.access_token)?;
  let config = identity.config()?;
  let context = Arc::new(Context::new(
    identity.token,
    args.cvm_name.unwrap_or_default(),
    tx,
  ));
  let listener = tokio::net::TcpListener::bind((args.ip.as_str(), args.port)).await?;
  println!("CloudTun WSS v3 listening at {}", listener.local_addr()?);
  if !args.standalone {
    let context = context.clone();
    tokio::spawn(async move {
      let mut ticks = tokio::time::interval(Duration::from_secs(60));
      ticks.tick().await;
      loop {
        ticks.tick().await;
        if context.is_ping_expired().await {
          if let Some(tx) = &context.tx {
            let result = async {
              let inst = tx.get_instance_by_name(&context.cvm_name).await?;
              tx.desroy_instance(&inst.id).await
            }
            .await;
            if let Err(error) = result {
              eprintln!("cloud reclamation failed: {error}");
            }
          }
        }
      }
    });
  }
  cloudtun_server::serve(
    listener,
    cloudtun_server::router(context),
    config,
    CancellationToken::new(),
  )
  .await?;
  Ok(())
}
