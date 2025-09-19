use clap::Parser;
use cloudtun_common::constant::{LOCAL_HTTP_PROXY_PORT, REMOTE_PROXY_PORT};
use cloudtun_proxy::{MatchType, ProxyArgs, run_proxy_loop};
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

  /// 本地客户端监听 PORT，默认 7892
  #[arg(long, default_value_t = LOCAL_HTTP_PROXY_PORT)]
  local_port: u16,

  /// 代理规则文件路径
  #[arg(short, long)]
  config: Option<String>,

  /// 和服务端通信的鉴权 Token
  #[arg(short, long)]
  token: String,
}

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
  let args = Args::parse();
  let proxy_args = ProxyArgs {
    server_addr: (args.server_ip, args.server_port, args.token),
    local_addr: (
      args.local_ip.unwrap_or("0.0.0.0".to_string()),
      args.local_port,
    ),
    default_rule: MatchType::Proxy,
    rules_config_file: args.config,
  };

  let shutdown_token = CancellationToken::new();
  let shutdown_token2 = shutdown_token.clone();
  let log_fn = |log_type: &str, log_message: &str| {
    println!("{log_type} ==> {log_message}");
  };
  let proxy_handle = tokio::spawn(async move {
    if let Err(e) = run_proxy_loop(proxy_args, shutdown_token2, log_fn).await {
      eprintln!("error occur: {e}");
    }
  });
  let ctrlc_handle = ctrlc2::AsyncCtrlC::new(move || {
    shutdown_token.cancel();
    true
  })?;
  let _ = tokio::join!(proxy_handle, ctrlc_handle);

  println!("Bye!");

  Ok(())
}
