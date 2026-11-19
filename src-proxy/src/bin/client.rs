use clap::Parser;
use cloudtun_common::constant::{LOCAL_HTTP_PROXY_PORT, REMOTE_PROXY_PORT};
use cloudtun_proxy::{MatchType, ProxyArgs, run_proxy_loop};
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio_util::sync::CancellationToken;
/// CloudTun - 超轻量网络代理命令行客户端
#[derive(Parser, Debug)]
#[command(version)]
struct Args {
  /// 代理服务端 IP
  #[arg(long)]
  server_ip: String,

  /// 代理服务端 PORT
  #[arg(long, default_value_t = REMOTE_PROXY_PORT)]
  server_port: u16,

  /// 本地客户端监听 IP，默认 0.0.0.0
  #[arg(long)]
  local_ip: Option<String>,

  /// 本地客户端监听 PORT，默认 7893
  #[arg(long, default_value_t = LOCAL_HTTP_PROXY_PORT)]
  local_port: u16,

  /// 代理规则文件路径
  #[arg(short, long)]
  config: Option<String>,
  /// 独立随机访问凭据（64 位十六进制）
  #[arg(long, env = "CLOUDTUN_ACCESS_TOKEN", hide_env_values = true)]
  token: String,
  #[arg(long)]
  instance_id: String,
  #[arg(long, default_value = ".cloudtun-trust")]
  trust_dir: std::path::PathBuf,
}

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
  let args = Args::parse();

  let token = args.token;
  if token.is_empty() {
    return Err("token is required".into());
  }
  let server = cloudtun_common::tls::CloudEndpoint::new(
    args.server_ip.clone(),
    args.server_port,
    args.instance_id,
    token,
    args.trust_dir,
  )?;
  let ping_server = server.clone();

  let proxy_rules = if let Some(rules_config_file) = &args.config {
    let mut f = File::open(rules_config_file).await?;
    let mut rules = String::new();
    f.read_to_string(&mut rules).await?;
    Some(rules)
  } else {
    None
  };

  let proxy_args = ProxyArgs {
    server,
    local_addr: (
      args.local_ip.unwrap_or("0.0.0.0".to_string()),
      args.local_port,
    ),
    default_rule: MatchType::Proxy,
    proxy_rules,
  };

  let shutdown_token = CancellationToken::new();
  let shutdown_token2 = shutdown_token.clone();
  let ping_cancel = shutdown_token.clone();
  let ping_handle = tokio::spawn(async move {
    let mut ticks = tokio::time::interval(std::time::Duration::from_secs(30));
    loop {
      tokio::select! { biased; _ = ping_cancel.cancelled() => break, _ = ticks.tick() => {} }
      tokio::select! { biased; _ = ping_cancel.cancelled() => break,
      _ = cloudtun_common::ping::ping_cloudtun_proxy_server(&ping_server) => {} }
    }
  });
  let log_fn = |log_type: &str, log_message: &str| {
    println!("{log_type} ==> {log_message}");
  };
  let proxy_handle = tokio::spawn(async move {
    if let Err(e) = run_proxy_loop(proxy_args, shutdown_token2, log_fn).await {
      eprintln!("error occur: {e}");
    }
  });
  let ctrlc_cancel = shutdown_token.clone();
  let ctrlc_handle = ctrlc2::AsyncCtrlC::new(move || {
    ctrlc_cancel.cancel();
    true
  })?;
  let mut proxy_handle = proxy_handle;
  tokio::select! {
    _ = &mut proxy_handle => {},
    _ = ctrlc_handle => { shutdown_token.cancel(); let _ = proxy_handle.await; }
  }
  shutdown_token.cancel();
  let _ = ping_handle.await;

  println!("Bye!");

  Ok(())
}
