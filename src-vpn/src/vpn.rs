use std::net::SocketAddr;
use std::sync::Arc;

use cloudtun_common::proxy::{generate_proxy_secret, proxy_to_cloudtun_server};
use ipstack::{IpStackStream, IpStackUdpStream};
use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
use tokio::{
  io::{AsyncRead, AsyncWrite},
  sync::Mutex,
};
use tokio_util::sync::CancellationToken;

use std::str::FromStr;
use tproxy_config::IpCidr;

// use tproxy_config::is_private_ip;
use udp_stream::UdpStream;

use crate::virtual_dns::VirtualDns;

const DNS_PORT: u16 = 53;

async fn handle_virtual_dns_session(
  mut udp: IpStackUdpStream,
  dns: Arc<Mutex<VirtualDns>>,
) -> anyhow::Result<()> {
  let mut buf = [0_u8; 4096];
  loop {
    let len = match udp.read(&mut buf).await {
      Err(e) => {
        // indicate UDP read fails not an error.
        log::error!("Virtual DNS session error: {e}");
        break;
      }
      Ok(len) => len,
    };
    if len == 0 {
      break;
    }
    let (msg, qname, ip) = dns.lock().await.generate_query(&buf[..len])?;
    udp.write_all(&msg).await?;
    println!("Virtual DNS query: {qname} -> {ip}");
  }
  Ok(())
}

pub async fn start_run_vpn<D, F: Fn(&str, &str) + Send + Sync + 'static>(
  device: D,
  mtu: u16,
  server_addr: (String, u16, String),
  shutdown_token: CancellationToken,
  log_fn: F,
) -> anyhow::Result<()>
where
  D: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
  let log_fn = Arc::new(log_fn);
  let virtual_dns = Arc::new(Mutex::new(VirtualDns::new(
    IpCidr::from_str("198.18.0.0/15").unwrap(),
  )));

  let mut ipstack_config = ipstack::IpStackConfig::default();
  ipstack_config.mtu(mtu);
  ipstack_config.tcp_timeout(std::time::Duration::from_secs(600));
  ipstack_config.udp_timeout(std::time::Duration::from_secs(10));

  let mut ip_stack = ipstack::IpStack::new(ipstack_config, device);

  let secret = Arc::new(generate_proxy_secret());
  let server_addr = Arc::new(server_addr);

  loop {
    let secret = secret.clone();
    let log_fn = log_fn.clone();
    let server_addr = server_addr.clone();
    let virtual_dns = virtual_dns.clone();
    let ip_stack_stream = tokio::select! {
        _ = shutdown_token.cancelled() => {
            println!("Shutdown received");
            break;
        }
        ip_stack_stream = ip_stack.accept() => {
            ip_stack_stream?
        }
    };

    match ip_stack_stream {
      IpStackStream::Tcp(tcp) => {
        let target_ip = tcp.peer_addr().ip();
        let target_port = tcp.peer_addr().port();
        let domain_name = {
          let mut virtual_dns = virtual_dns.lock().await;
          virtual_dns.touch_ip(&target_ip);
          virtual_dns.resolve_ip(&target_ip).cloned()
        };

        tokio::spawn(async move {
          if let Err(err) = proxy_to_cloudtun_server(
            tcp,
            server_addr,
            domain_name.unwrap_or(target_ip.to_string()),
            target_port,
            secret,
            log_fn,
          )
          .await
          {
            eprintln!("failed proxy: \"{err}\"");
          }
        });
      }
      IpStackStream::Udp(mut udp) => {
        if udp.peer_addr().port() == DNS_PORT {
          tokio::spawn(async move {
            if let Err(err) = handle_virtual_dns_session(udp, virtual_dns).await {
              eprintln!("failed handler virtual dns: \"{err}\"");
            }
          });
        } else {
          tokio::spawn(async move {
            let target_ip = udp.peer_addr().ip();
            let target_port = udp.peer_addr().port();
            let target = format!("{}:{}", target_ip, target_port);
            let mut target_stream =
              match UdpStream::connect(SocketAddr::from_str(&target).unwrap()).await {
                Err(e) => {
                  eprintln!("failed create udp stream");
                  return;
                }
                Ok(x) => x,
              };

            let mut buf1 = [0_u8; 4096];
            let mut buf2 = [0_u8; 4096];
            loop {
              tokio::select! {
                len = udp.read(&mut buf1) => {
                  let len = match len {
                    Ok(n) => n,
                    Err(e) => {
                      eprintln!("{e}");
                      break;
                    }
                  };
                  if len == 0 {
                      break;
                  }
                  let buf1 = &buf1[..len];
                  if let Err(e) = target_stream.write_all(buf1).await {
                    eprintln!("{e}");
                    break;
                  }
                },
                len = target_stream.read(&mut buf2) => {
                  let len = match len {
                    Ok(n) => n,
                    Err(e) => {
                      eprintln!("{e}");
                      break;
                    }
                  };
                  if len == 0 {
                      break;
                  }
                  let buf2 = &buf2[..len];
                  if let Err(e) = udp.write_all(buf2).await {
                    eprintln!("{e}");
                    break;
                  }
                }
              }
            }
          });
        }
      }
      IpStackStream::UnknownTransport(u) => {
        let len = u.payload().len();
        println!(
          "#0 unhandled transport - Ip Protocol {:?}, length {}",
          u.ip_protocol(),
          len
        );
      }
      IpStackStream::UnknownNetwork(pkt) => {
        println!("#0 unknown transport - {} bytes", pkt.len());
      }
    }
  }
  Ok(())
}
