use std::thread;

use clap::Parser;
use cloudtun_common::{LOCAL_HTTP_PROXY_PORT, REMOTE_PROXY_PORT};
use cloudtun_proxy::{MatchType, StartProxyArgs, start_proxy, stop_proxy};

/// CloudTun - 超轻量网络代理命令行客户端
#[derive(Parser, Debug)]
#[command(version)]
struct Args {
  /// CloudTun 代理服务端 IP
  #[arg(short, long)]
  server: String,

  /// 代理规则文件路径
  #[arg(short, long)]
  config: Option<String>,
}

fn main() {
  let args = Args::parse();
  let proxy_args = StartProxyArgs {
    server_ip: args.server,
    server_port: REMOTE_PROXY_PORT,
    local_ip: "0.0.0.0".to_string(),
    local_port: LOCAL_HTTP_PROXY_PORT,
    default_rule: MatchType::Proxy,
    rules_config_file: args.config,
  };

  let proxy_handle = thread::spawn(move || {
    start_proxy(proxy_args);
  });

  ctrlc::set_handler(move || {
    stop_proxy();
  })
  .unwrap();

  proxy_handle.join().unwrap();

  println!("Bye!");
}
