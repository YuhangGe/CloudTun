use std::{
  collections::VecDeque, io::ErrorKind, net::{IpAddr, SocketAddr}, os::fd::AsRawFd, process, sync::Arc
};

 
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

mod args;
mod directions;
mod dns;
mod error;
mod no_proxy;
mod proxy_handler;
mod proxy_manager;
mod session_info;
mod virtual_dns;

pub use error::Result;
pub use args::Args;
use tproxy_config::is_private_ip;
use udp_stream::UdpStream;

use crate::{
  directions::{IncomingDataEvent, IncomingDirection, OutgoingDirection},
  no_proxy::NoProxyManager,
  proxy_handler::{ProxyHandler, ProxyHandlerManager},
  session_info::{IpProtocol, SessionInfo},
  virtual_dns::VirtualDns,
};

const DNS_PORT: u16 = 53;

#[allow(unused)]
#[derive(Hash, Copy, Clone, Eq, PartialEq, Debug)]
#[cfg_attr(
  target_os = "linux",
  derive(bincode::Encode, bincode::Decode, serde::Serialize, serde::Deserialize)
)]
pub enum SocketProtocol {
  Tcp,
  Udp,
}

#[allow(unused)]
#[derive(Hash, Copy, Clone, Eq, PartialEq, Debug)]
#[cfg_attr(
  target_os = "linux",
  derive(bincode::Encode, bincode::Decode, serde::Serialize, serde::Deserialize)
)]
pub enum SocketDomain {
  IpV4,
  IpV6,
}

impl From<IpAddr> for SocketDomain {
  fn from(value: IpAddr) -> Self {
    match value {
      IpAddr::V4(_) => Self::IpV4,
      IpAddr::V6(_) => Self::IpV6,
    }
  }
}

struct SocketQueue {
  tcp_v4: Mutex<Receiver<TcpSocket>>,
  tcp_v6: Mutex<Receiver<TcpSocket>>,
  udp_v4: Mutex<Receiver<UdpSocket>>,
  udp_v6: Mutex<Receiver<UdpSocket>>,
}

impl SocketQueue {
  async fn recv_tcp(&self, domain: SocketDomain) -> Result<TcpSocket, std::io::Error> {
    match domain {
      SocketDomain::IpV4 => &self.tcp_v4,
      SocketDomain::IpV6 => &self.tcp_v6,
    }
    .lock()
    .await
    .recv()
    .await
    .ok_or(ErrorKind::Other.into())
  }
  async fn recv_udp(&self, domain: SocketDomain) -> Result<UdpSocket, std::io::Error> {
    match domain {
      SocketDomain::IpV4 => &self.udp_v4,
      SocketDomain::IpV6 => &self.udp_v6,
    }
    .lock()
    .await
    .recv()
    .await
    .ok_or(ErrorKind::Other.into())
  }
}

async fn create_tcp_stream(
  socket_queue: &Option<Arc<SocketQueue>>,
  peer: SocketAddr,
) -> std::io::Result<TcpStream> {
  match &socket_queue {
    None => TcpStream::connect(peer).await,
    Some(queue) => queue.recv_tcp(peer.ip().into()).await?.connect(peer).await,
  }
}

async fn create_udp_stream(
  socket_queue: &Option<Arc<SocketQueue>>,
  peer: SocketAddr,
) -> std::io::Result<UdpStream> {
  match &socket_queue {
    None => UdpStream::connect(peer).await,
    Some(queue) => {
      let socket = queue.recv_udp(peer.ip().into()).await?;
      socket.connect(peer).await?;
      UdpStream::from_tokio(socket, peer).await
    }
  }
}



fn bind_to_iface(sock: &Socket, iface_name: &str) -> std::io::Result<()> {
    // 获取接口索引
    let ifindex = unsafe { libc::if_nametoindex(iface_name.as_ptr() as *const i8) };
    if ifindex == 0 {
        return Err(std::io::Error::last_os_error());
    }

    // 设置 socket 选项 IP_BOUND_IF
    let ret = unsafe {
        libc::setsockopt(
            sock.as_raw_fd(),
            libc::IPPROTO_IP,
            libc::IP_BOUND_IF,
            &ifindex as *const _ as *const libc::c_void,
            std::mem::size_of_val(&ifindex) as libc::socklen_t,
        )
    };
    if ret != 0 {
        return Err(std::io::Error::last_os_error());
    }

    Ok(())
}

async fn handle_tcp_session(
  mut tcp_stack: IpStackTcpStream,
  proxy_handler: Arc<Mutex<dyn ProxyHandler>>,
  socket_queue: Option<Arc<SocketQueue>>,
) -> crate::Result<()> {
  let (session_info, server_addr) = {
    let handler = proxy_handler.lock().await;

    (handler.get_session_info(), handler.get_server_addr())
  };

  // let mut server = create_tcp_stream(&socket_queue, server_addr).await?;

// 创建原始 socket
    let sock = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP))?;

    // ⚠️ 把接口名字传进来，例如 "en0"
    bind_to_iface(&sock, "en0\0")?; // 注意这里要带一个 '\0'，C 风格字符串

    sock.set_nonblocking(true)?;
    let std_stream: std::net::TcpStream = sock.into();
    let mut server = TcpStream::from_std(std_stream)?;

  // println!("Beginning 111 {session_info}");

  if let Err(e) = handle_proxy_session(&mut server, proxy_handler).await {
    tcp_stack.shutdown().await?;
    return Err(e);
  }

  // println!("XXXXXXXXX 111");

  let (mut t_rx, mut t_tx) = tokio::io::split(tcp_stack);
  let (mut s_rx, mut s_tx) = tokio::io::split(server);

  let res = tokio::join!(
    async move {
      // println!("XXXXXXX 222");
      let r = copy_and_record_traffic(&mut t_rx, &mut s_tx, true).await;
      // println!("XXXXXXX 222 2222");
      if let Err(err) = s_tx.shutdown().await {
        eprintln!("{session_info} s_tx shutdown error {err}");
      }
      r
    },
    async move {
      // println!("XXXXXXX 333");
      let r = copy_and_record_traffic(&mut s_rx, &mut t_tx, false).await;
      // println!("XXXXXXX 333 3333");
      if let Err(err) = t_tx.shutdown().await {
        eprintln!("{session_info} t_tx shutdown error {err}");
      }
      r
    },
  );
  println!("Ending {session_info} with {res:?}");

  Ok(())
}

async fn handle_proxy_session(
  server: &mut TcpStream,
  proxy_handler: Arc<Mutex<dyn ProxyHandler>>,
) -> crate::Result<Option<SocketAddr>> {
  let mut launched = false;
  let mut proxy_handler = proxy_handler.lock().await;
  let dir = OutgoingDirection::ToServer;
  let (mut tx, mut rx) = (0, 0);

  loop {
    if proxy_handler.connection_established() {
      break;
    }

    if !launched {
      let data = proxy_handler.peek_data(dir).buffer;
      let len = data.len();
      if len == 0 {
        return Err("proxy_handler launched went wrong".into());
      }
      server.write_all(data).await?;
      proxy_handler.consume_data(dir, len);
      tx += len;

      launched = true;
    }

    let mut buf = [0_u8; 4096];
    let len = server.read(&mut buf).await?;
    if len == 0 {
      return Err("server closed accidentially".into());
    }
    rx += len;
    let event = IncomingDataEvent {
      direction: IncomingDirection::FromServer,
      buffer: &buf[..len],
    };
    proxy_handler.push_data(event).await?;

    let data = proxy_handler.peek_data(dir).buffer;
    let len = data.len();
    if len > 0 {
      server.write_all(data).await?;
      proxy_handler.consume_data(dir, len);
      tx += len;
    }
  }
  // crate::traffic_status::traffic_status_update(tx, rx)?;
  Ok(proxy_handler.get_udp_associate())
}

async fn handle_udp_associate_session(
  mut udp_stack: IpStackUdpStream,
  proxy_handler: Arc<Mutex<dyn ProxyHandler>>,
  socket_queue: Option<Arc<SocketQueue>>,
) -> crate::Result<()> {
  let (session_info, server_addr, domain_name, udp_addr) = {
    let handler = proxy_handler.lock().await;
    (
      handler.get_session_info(),
      handler.get_server_addr(),
      handler.get_domain_name(),
      handler.get_udp_associate(),
    )
  };

  println!("Beginning {session_info}");

  // `_server` is meaningful here, it must be alive all the time
  // to ensure that UDP transmission will not be interrupted accidentally.
  let (_server, udp_addr) = match udp_addr {
    Some(udp_addr) => (None, udp_addr),
    None => {
      let mut server = create_tcp_stream(&socket_queue, server_addr).await?;
      let udp_addr = handle_proxy_session(&mut server, proxy_handler).await?;
      (Some(server), udp_addr.ok_or("udp associate failed")?)
    }
  };

  let mut udp_server = create_udp_stream(&socket_queue, udp_addr).await?;

  let mut buf1 = [0_u8; 4096];
  let mut buf2 = [0_u8; 4096];
  loop {
    tokio::select! {
        len = udp_stack.read(&mut buf1) => {
            let len = len?;
            if len == 0 {
                break;
            }
            let buf1 = &buf1[..len];


            udp_server.write_all(buf1).await?;

        }
        len = udp_server.read(&mut buf2) => {
            let len = len?;
            if len == 0 {
                break;
            }
            let buf2 = &buf2[..len];

            udp_stack.write_all(buf2).await?;
        }
    }
  }

  println!("Ending {session_info}");

  Ok(())
}

async fn handle_virtual_dns_session(
  mut udp: IpStackUdpStream,
  dns: Arc<Mutex<VirtualDns>>,
) -> crate::Result<()> {
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

async fn copy_and_record_traffic<R, W>(
  reader: &mut R,
  writer: &mut W,
  is_tx: bool,
) -> tokio::io::Result<u64>
where
  R: tokio::io::AsyncRead + Unpin + ?Sized,
  W: tokio::io::AsyncWrite + Unpin + ?Sized,
{
  let mut buf = vec![0; 8192];
  let mut total = 0;
  loop {
    let x = reader.read(&mut buf).await;
    let xx = match x {
        Ok(xx) => xx,
        Err(e) => {
          println!("{is_tx}: XEEEE {e}");
          return Err(e);
        }
    };
    // let xxx = buf[..xx].to_vec();
    // unsafe {
    //   println!("XXPPPPP {xx}, {}|{}|{}, {}", buf[0], buf[1], buf[2], String::from_utf8_unchecked(xxx));
    // }
    match  xx {
      0 => break, // EOF
      n => {
        total += n as u64;
        // let (tx, rx) = if is_tx { (n, 0) } else { (0, n) };
        // if let Err(e) = crate::traffic_status::traffic_status_update(tx, rx) {
        //     log::debug!("Record traffic status error: {e}");
        // }
        match writer.write_all(&buf[..n]).await {
          Ok(()) => {
            println!("{is_tx}: XEEE 1 ok {n}");
          },
          Err(e) => {
          println!("{is_tx}: XEEEE 2 {e}");
            return Err(e);
          }
        }
      }
    }
  }
  println!("{is_tx}: COPPPPPP {total}");
  Ok(total)
}

/// Run the proxy server
/// # Arguments
/// * `device` - The network device to use
/// * `mtu` - The MTU of the network device
/// * `args` - The arguments to use
/// * `shutdown_token` - The token to exit the server
/// # Returns
/// * The number of sessions while exiting
pub async fn run<D>(
  device: D,
  mtu: u16,
  args: Args,
  shutdown_token: CancellationToken,
) -> Result<usize>
where
  D: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
  // let server_addr = args.proxy.addr;
  // let key = args.proxy.credentials.clone();
//   let dns_addr = args.dns_addr;
//   let ipv6_enabled = args.ipv6_enabled;
  let virtual_dns = Arc::new(Mutex::new(VirtualDns::new(args.virtual_dns_pool)));

  #[cfg(not(target_os = "linux"))]
  let socket_queue = None;

  // use socks5_impl::protocol::Version::{V4, V5};
  // let mgr: Arc<dyn ProxyHandlerManager> = match args.proxy.proxy_type {
  //     ProxyType::Socks5 => Arc::new(SocksProxyManager::new(server_addr, V5, key)),
  //     ProxyType::Socks4 => Arc::new(SocksProxyManager::new(server_addr, V4, key)),
  //     ProxyType::Http => Arc::new(HttpManager::new(server_addr, key)),
  //     ProxyType::None => Arc::new(NoProxyManager::new()),
  // };

  let mgr: Arc<dyn ProxyHandlerManager> = Arc::new(NoProxyManager::new());

  let mut ipstack_config = ipstack::IpStackConfig::default();
  ipstack_config.mtu(mtu);
  ipstack_config.tcp_timeout(std::time::Duration::from_secs(args.tcp_timeout));
  ipstack_config.udp_timeout(std::time::Duration::from_secs(args.udp_timeout));

  let mut ip_stack = ipstack::IpStack::new(ipstack_config, device);

  let task_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
  use std::sync::atomic::Ordering::Relaxed;

  loop {
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
    let max_sessions = args.max_sessions;
    match ip_stack_stream {
      IpStackStream::Tcp(tcp) => {
        if task_count.load(Relaxed) >= max_sessions {
        //   if args.exit_on_fatal_error {
            println!("Too many sessions that over {max_sessions}, exiting...");
        //     break;
        //   }
          // println!("Too many sessions that over {max_sessions}, dropping new session");
          break;
        }
       println!("Session count {}", task_count.fetch_add(1, Relaxed).saturating_add(1));
        let info = SessionInfo::new(tcp.local_addr(), tcp.peer_addr(), IpProtocol::Tcp);
        let domain_name = {
          let mut virtual_dns = virtual_dns.lock().await;
          virtual_dns.touch_ip(&tcp.peer_addr().ip());
          virtual_dns.resolve_ip(&tcp.peer_addr().ip()).cloned()
        };
        // println!("DDDomain {:#?}", domain_name);
        let proxy_handler = mgr.new_proxy_handler(info, domain_name, false).await?;
        let socket_queue = socket_queue.clone();
        tokio::spawn(async move {
          if let Err(err) = handle_tcp_session(tcp, proxy_handler, socket_queue).await {
            eprintln!("{info} error \"{err}\"");
          }
          println!(
            "Session count {}",
            task_count.fetch_sub(1, Relaxed).saturating_sub(1)
          );
        });
      }
      IpStackStream::Udp(udp) => {
        if task_count.load(Relaxed) >= max_sessions {
        //   if args.exit_on_fatal_error {
        //     println!("Too many sessions that over {max_sessions}, exiting...");
        //     break;
        //   }
          println!("Too many sessions that over {max_sessions}, dropping new session");
          continue;
        }
        println!(
          "Session count {}",
          task_count.fetch_add(1, Relaxed).saturating_add(1)
        );
        let  info = SessionInfo::new(udp.local_addr(), udp.peer_addr(), IpProtocol::Udp);
        if info.dst.port() == DNS_PORT {
        //   if is_private_ip(info.dst.ip()) {
        //     info.dst.set_ip(dns_addr); // !!! Here we change the destination address to remote DNS server!!!
        //   }

          tokio::spawn(async move {
            if let Err(err) = handle_virtual_dns_session(udp, virtual_dns).await {
              eprintln!("{info} error \"{err}\"");
            }
            println!(
              "Session count {}",
              task_count.fetch_sub(1, Relaxed).saturating_sub(1)
            );
          });
          continue;
        }
        let domain_name = {
          let mut virtual_dns = virtual_dns.lock().await;
          virtual_dns.touch_ip(&udp.peer_addr().ip());
          virtual_dns.resolve_ip(&udp.peer_addr().ip()).cloned()
        };

        match mgr.new_proxy_handler(info, domain_name, true).await {
          Ok(proxy_handler) => {
            let socket_queue = socket_queue.clone();
            tokio::spawn(async move {
              // let ty = args.proxy.proxy_type;
              if let Err(err) = handle_udp_associate_session(udp, proxy_handler, socket_queue).await
              {
                eprintln!("Ending {info} with \"{err}\"");
              }
              println!(
                "Session count {}",
                task_count.fetch_sub(1, Relaxed).saturating_sub(1)
              );
            });
          }
          Err(e) => {
            eprintln!("Failed to create UDP connection: {e}");
          }
        }
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
