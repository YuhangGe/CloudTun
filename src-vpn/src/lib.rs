use std::{
  collections::VecDeque,
  io::ErrorKind,
  net::{IpAddr, SocketAddr},
  os::fd::AsRawFd,
  process,
  sync::Arc,
};

use cloudtun_common::proxy::{generate_proxy_secret, proxy_to_cloudtun_server};
use ipstack::{IpStackStream, IpStackTcpStream, IpStackUdpStream};
use tokio::{
  io::{AsyncRead, AsyncWrite},
  sync::Mutex,
};
use tokio::{
  io::{AsyncReadExt, AsyncWriteExt},
  net::{TcpSocket, TcpStream, UdpSocket},
  sync::mpsc::Receiver,
};
use tokio_util::sync::CancellationToken;

use libc;
use socket2::{Domain, Protocol, Socket, Type};
use std::str::FromStr;
use tproxy_config::IpCidr;

mod dns;
mod virtual_dns;

use tproxy_config::is_private_ip;
use udp_stream::UdpStream;

use crate::virtual_dns::VirtualDns;

const DNS_PORT: u16 = 53;

// #[inline]
// async fn create_tcp_stream(
//   // socket_queue: &Option<Arc<SocketQueue>>,
//   peer: SocketAddr,
// ) -> std::io::Result<TcpStream> {
//   // match &socket_queue {
//   //   None => TcpStream::connect(peer).await,
//   //   Some(queue) => queue.recv_tcp(peer.ip().into()).await?.connect(peer).await,
//   // }
//   TcpStream::connect(peer).await
// }

// async fn create_udp_stream(
//   // socket_queue: &Option<Arc<SocketQueue>>,
//   peer: SocketAddr,
// ) -> std::io::Result<UdpStream> {
//   UdpStream::connect(peer).await
// }

// async fn handle_udp_associate_session(
//   mut udp_stack: IpStackUdpStream,
//   proxy_handler: Arc<Mutex<dyn ProxyHandler>>,
//   // socket_queue: Option<Arc<SocketQueue>>,
// ) -> crate::Result<()> {
//   let (session_info, server_addr, domain_name, udp_addr) = {
//     let handler = proxy_handler.lock().await;
//     (
//       handler.get_session_info(),
//       handler.get_server_addr(),
//       handler.get_domain_name(),
//       handler.get_udp_associate(),
//     )
//   };

//   println!("Beginning {session_info}");

//   // `_server` is meaningful here, it must be alive all the time
//   // to ensure that UDP transmission will not be interrupted accidentally.
//   let (_server, udp_addr) = match udp_addr {
//     Some(udp_addr) => (None, udp_addr),
//     None => {
//       let mut server = create_tcp_stream(server_addr).await?;
//       let udp_addr = handle_proxy_session(&mut server, proxy_handler).await?;
//       (Some(server), udp_addr.ok_or("udp associate failed")?)
//     }
//   };

//   let mut udp_server = create_udp_stream(udp_addr).await?;

//   let mut buf1 = [0_u8; 4096];
//   let mut buf2 = [0_u8; 4096];
//   loop {
//     tokio::select! {
//         len = udp_stack.read(&mut buf1) => {
//             let len = len?;
//             if len == 0 {
//                 break;
//             }
//             let buf1 = &buf1[..len];

//             udp_server.write_all(buf1).await?;

//         }
//         len = udp_server.read(&mut buf2) => {
//             let len = len?;
//             if len == 0 {
//                 break;
//             }
//             let buf2 = &buf2[..len];

//             udp_stack.write_all(buf2).await?;
//         }
//     }
//   }

//   println!("Ending {session_info}");

//   Ok(())
// }

async fn handle_virtual_dns_session(
  mut udp: IpStackUdpStream,
  dns: Arc<Mutex<VirtualDns>>,
) -> anyhow::Result<()> {
  let mut buf = [0_u8; 4096];
  loop {
    let len = match udp.read(&mut buf).await {
      Err(e) => {
        // indicate UDP read fails not an error.
        println!("Virtual DNS session error: {e}");
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
) -> anyhow::Result<usize>
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

  let task_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
  use std::sync::atomic::Ordering::Relaxed;

  let secret = Arc::new(generate_proxy_secret());
  let server_addr = Arc::new(server_addr);

  loop {
    let secret = secret.clone();
    let log_fn = log_fn.clone();
    let server_addr = server_addr.clone();
    let task_count = task_count.clone();
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
    println!("got ip stack");
    match ip_stack_stream {
      IpStackStream::Tcp(tcp) => {
        if task_count.load(Relaxed) >= 200 {
          println!("Too many sessions, exiting...");
          break;
        }
        println!(
          "Session count {}",
          task_count.fetch_add(1, Relaxed).saturating_add(1)
        );
        // let info = SessionInfo::new(tcp.local_addr(), tcp.peer_addr(), IpProtocol::Tcp);
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
          println!(
            "Session count {}",
            task_count.fetch_sub(1, Relaxed).saturating_sub(1)
          );
        });
      }
      IpStackStream::Udp(udp) => {
        if task_count.load(Relaxed) >= 200 {
          println!("Too many udp sessions");
          break;
        }
        println!(
          "Session count {}",
          task_count.fetch_add(1, Relaxed).saturating_add(1)
        );
        // let info = SessionInfo::new(udp.local_addr(), udp.peer_addr(), IpProtocol::Udp);
        if udp.peer_addr().port() == DNS_PORT {
          //   if is_private_ip(info.dst.ip()) {
          //     info.dst.set_ip(dns_addr); // !!! Here we change the destination address to remote DNS server!!!
          //   }

          tokio::spawn(async move {
            if let Err(err) = handle_virtual_dns_session(udp, virtual_dns).await {
              eprintln!("failed handler virtual dns: \"{err}\"");
            }
            println!(
              "Session count {}",
              task_count.fetch_sub(1, Relaxed).saturating_sub(1)
            );
          });
          continue;
        }

        // let domain_name = {
        //   let mut virtual_dns = virtual_dns.lock().await;
        //   virtual_dns.touch_ip(&udp.peer_addr().ip());
        //   virtual_dns.resolve_ip(&udp.peer_addr().ip()).cloned()
        // };

        // match mgr.new_proxy_handler(info, domain_name, true).await {
        //   Ok(proxy_handler) => {
        //     // let socket_queue = socket_queue.clone();
        //     tokio::spawn(async move {
        //       // let ty = args.proxy.proxy_type;
        //       if let Err(err) = handle_udp_associate_session(udp, proxy_handler).await {
        //         eprintln!("Ending {info} with \"{err}\"");
        //       }
        //       println!(
        //         "Session count {}",
        //         task_count.fetch_sub(1, Relaxed).saturating_sub(1)
        //       );
        //     });
        //   }
        //   Err(e) => {
        //     eprintln!("Failed to create UDP connection: {e}");
        //   }
        // }
      }
      IpStackStream::UnknownTransport(u) => {
        let len = u.payload().len();
        println!(
          "#0 unhandled transport - Ip Protocol {:?}, length {}",
          u.ip_protocol(),
          len
        );
        continue;
      }
      IpStackStream::UnknownNetwork(pkt) => {
        println!("#0 unknown transport - {} bytes", pkt.len());
        continue;
      }
    }
  }
  Ok(task_count.load(Relaxed))
}
