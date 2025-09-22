use cloudtun_vpn::start_run_vpn;
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

  let server_addr = (
    "43.152.227.239".to_string(),
    24816,
    "8542623f-450a-40f5-93f2-5e40843b6f30".to_string(),
  );

  let log_fn = |log_type: &str, log_message: &str| {
    println!("{log_type} ==> {log_message}");
  };

  let shutdown_token = CancellationToken::new();
  let shutdown_token2 = shutdown_token.clone();
  let run_handle =
    spawn(async move { start_run_vpn(device, MTU, server_addr, shutdown_token2, log_fn).await });
  let ctrlc_handle = ctrlc2::AsyncCtrlC::new(move || {
    shutdown_token.cancel();
    true
  })?;

  let _ = tokio::join!(run_handle, ctrlc_handle);
  println!("Bye!");

  Ok(())
}
