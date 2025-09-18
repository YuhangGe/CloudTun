use std::process::{self, Command};

use cloudtun_vpn::{Args, run};
use tokio::spawn;
use tokio_util::sync::CancellationToken;
use tun::AbstractDevice;

const MTU: u16 = 1500;

#[tokio::main]
pub async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
  let mut config = tun::Configuration::default();
  config
    .mtu(MTU)
    // .raw_fd(fd)
    // .tun_name("utun4")
    .address((10, 0, 0, 1))
    .netmask((255, 255, 255, 0))
    // .destination((10, 0, 0, 1))
    .up();

  #[cfg(target_os = "linux")]
  config.platform_config(|config| {
    // requiring root privilege to acquire complete functions
    config.ensure_root_privileges(true);
  });
  println!("will create");
  let device = tun::create_as_async(&config)?;
  println!("after create");

  let args = Args::default();
  let shutdown_token = CancellationToken::new();
  let shutdown_token2 = shutdown_token.clone();
  let run_handle = spawn(async move { run(device, MTU, args, shutdown_token2).await });
  let ctrlc_handle = ctrlc2::AsyncCtrlC::new(move || {
    shutdown_token.cancel();
    true
  })?;

  let _ = tokio::join!(run_handle, ctrlc_handle);
  println!("Bye!");
  
  process::exit(0);
  Ok(())
}
