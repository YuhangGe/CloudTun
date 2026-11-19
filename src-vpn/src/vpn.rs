use std::sync::Arc;

use cloudtun_common::proxy::{proxy_to_cloudtun_server, proxy_udp_to_cloudtun_server};
use ipstack::{IpStackStream, IpStackUdpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::{
  io::{AsyncRead, AsyncWrite},
  sync::Mutex,
};
use tokio_util::sync::CancellationToken;

use std::str::FromStr;
use tproxy_config::IpCidr;

// use tproxy_config::is_private_ip;

use crate::virtual_dns::VirtualDns;

const DNS_PORT: u16 = 53;

async fn handle_virtual_dns_session(
  mut udp: IpStackUdpStream,
  dns: Arc<Mutex<VirtualDns>>,
) -> anyhow::Result<()> {
  let mut buf = [0_u8; 65535];
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
    let (msg, qname, ip) = match dns.lock().await.generate_query(&buf[..len]) {
      Ok(response) => response,
      Err(e) => {
        log::debug!("ignoring malformed DNS query: {e}");
        continue;
      }
    };
    anyhow::ensure!(
      udp.write(&msg).await? == msg.len(),
      "DNS response exceeds MTU"
    );
    println!("Virtual DNS query: {qname} -> {ip:?}");
  }
  Ok(())
}

pub async fn start_run_vpn<D, F: Fn(&str, &str) + Send + Sync + 'static>(
  device: D,
  mtu: u16,
  server_addr: (String, u16, String),
  password: Vec<u8>,
  shutdown_token: CancellationToken,
  log_fn: F,
) -> anyhow::Result<()>
where
  D: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
  anyhow::ensure!(password.len() == 16 && mtu >= 576, "invalid VPN key or MTU");
  let log_fn = Arc::new(log_fn);
  let virtual_dns = Arc::new(Mutex::new(VirtualDns::new(
    IpCidr::from_str("198.18.0.0/15").unwrap(),
  )));

  let mut ipstack_config = ipstack::IpStackConfig::default();
  let _ = ipstack_config.mtu(mtu);
  // ipstack_config.tcp_timeout(std::time::Duration::from_secs(600));
  ipstack_config.udp_timeout(std::time::Duration::from_secs(60));

  let mut ip_stack = ipstack::IpStack::new(ipstack_config, device);

  let password = Arc::new(password);
  let server_addr = Arc::new(server_addr);

  let mut sessions = tokio::task::JoinSet::new();
  loop {
    let password = password.clone();
    let log_fn = log_fn.clone();
    let server_addr = server_addr.clone();
    let virtual_dns = virtual_dns.clone();
    let ip_stack_stream = tokio::select! {
        _ = shutdown_token.cancelled() => {
            println!("Shutdown received");
            break;
        }
        _ = sessions.join_next(), if !sessions.is_empty() => continue,
        ip_stack_stream = ip_stack.accept() => {
            match ip_stack_stream { Ok(s) => s, Err(e) => {
              sessions.abort_all();
              while sessions.join_next().await.is_some() {}
              return Err(e.into());
            }}
        }
    };

    match ip_stack_stream {
      IpStackStream::Tcp(mut tcp) => {
        if tcp.peer_addr().port() == DNS_PORT {
          let dns = virtual_dns.clone();
          sessions.spawn(async move {
            let result: anyhow::Result<()> = async {
              loop {
                let len = tcp.read_u16().await? as usize;
                let mut data = vec![0; len];
                tcp.read_exact(&mut data).await?;
                let (response, _, _) = dns.lock().await.generate_query(&data)?;
                tcp.write_u16(u16::try_from(response.len())?).await?;
                tcp.write_all(&response).await?;
              }
            }
            .await;
            if let Err(e) = result {
              log::debug!("TCP DNS session closed: {e}");
            }
          });
          continue;
        }
        let target_ip = tcp.peer_addr().ip();
        let target_port = tcp.peer_addr().port();
        let domain_name = {
          let virtual_dns = virtual_dns.lock().await;
          virtual_dns.resolve_ip(&target_ip).cloned()
        };

        if domain_name.is_none() && VirtualDns::is_fake_ip(&target_ip) {
          log::warn!("rejecting unknown fake DNS address {target_ip}");
          continue;
        }
        sessions.spawn(async move {
          if let Err(err) = proxy_to_cloudtun_server(
            tcp,
            server_addr,
            domain_name.unwrap_or(target_ip.to_string()),
            target_port,
            password,
            log_fn,
          )
          .await
          {
            eprintln!("failed proxy: \"{err}\"");
          }
        });
      }
      IpStackStream::Udp(udp) => {
        if udp.peer_addr().port() == DNS_PORT {
          sessions.spawn(async move {
            if let Err(err) = handle_virtual_dns_session(udp, virtual_dns).await {
              eprintln!("failed handler virtual dns: \"{err}\"");
            }
          });
        } else {
          let target_ip = udp.peer_addr().ip();
          let target_port = udp.peer_addr().port();
          let domain = virtual_dns.lock().await.resolve_ip(&target_ip).cloned();
          if domain.is_none() && VirtualDns::is_fake_ip(&target_ip) {
            continue;
          }
          let max_response = mtu.saturating_sub(if target_ip.is_ipv4() { 28 } else { 48 }) as usize;
          sessions.spawn(async move {
            if let Err(e) = proxy_udp_to_cloudtun_server(
              udp,
              server_addr,
              domain.unwrap_or_else(|| target_ip.to_string()),
              target_port,
              password,
              log_fn,
              max_response,
            )
            .await
            {
              log::debug!("UDP tunnel closed: {e}");
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
  sessions.abort_all();
  while sessions.join_next().await.is_some() {}
  Ok(())
}
