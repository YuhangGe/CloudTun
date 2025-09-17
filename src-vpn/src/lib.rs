use std::{collections::VecDeque, io::ErrorKind, net::{IpAddr, SocketAddr}, sync::Arc};

use ipstack::{IpStackStream, IpStackTcpStream, IpStackUdpStream};
use tokio::{io::{AsyncRead, AsyncWrite}, sync::Mutex};
use tokio_util::sync::CancellationToken;
use tokio::{
    io::{ AsyncReadExt,  AsyncWriteExt},
    net::{TcpSocket, TcpStream, UdpSocket},
    sync::{ mpsc::Receiver},
};
 
  mod error;
mod dns;
mod virtual_dns;
mod args;
mod session_info;
mod directions;
mod proxy_handler;
mod proxy_manager;
mod no_proxy;

pub use error::Result;
use tproxy_config::is_private_ip;
use udp_stream::UdpStream;

use crate::{args::{ArgDns, Args}, no_proxy::NoProxyManager, proxy_handler::{ProxyHandler, ProxyHandlerManager}, session_info::{IpProtocol, SessionInfo}, virtual_dns::VirtualDns};

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

async fn create_tcp_stream(socket_queue: &Option<Arc<SocketQueue>>, peer: SocketAddr) -> std::io::Result<TcpStream> {
    match &socket_queue {
        None => TcpStream::connect(peer).await,
        Some(queue) => queue.recv_tcp(peer.ip().into()).await?.connect(peer).await,
    }
}

async fn create_udp_stream(socket_queue: &Option<Arc<SocketQueue>>, peer: SocketAddr) -> std::io::Result<UdpStream> {
    match &socket_queue {
        None => UdpStream::connect(peer).await,
        Some(queue) => {
            let socket = queue.recv_udp(peer.ip().into()).await?;
            socket.connect(peer).await?;
            UdpStream::from_tokio(socket, peer).await
        }
    }
}

async fn handle_tcp_session(
    mut tcp_stack: IpStackTcpStream,
    proxy_handler: Arc<Mutex<dyn ProxyHandler>>,
    socket_queue: Option<Arc<SocketQueue
    >>,
) -> crate::Result<()> {
    let (session_info, server_addr) = {
        let handler = proxy_handler.lock().await;

        (handler.get_session_info(), handler.get_server_addr())
    };

    let mut server = create_tcp_stream(&socket_queue, server_addr).await?;

    println!("Beginning {session_info}");

    if let Err(e) = handle_proxy_session(&mut server, proxy_handler).await {
        tcp_stack.shutdown().await?;
        return Err(e);
    }

    let (mut t_rx, mut t_tx) = tokio::io::split(tcp_stack);
    let (mut s_rx, mut s_tx) = tokio::io::split(server);

    let res = tokio::join!(
        async move {
            let r = copy_and_record_traffic(&mut t_rx, &mut s_tx, true).await;
            if let Err(err) = s_tx.shutdown().await {
                eprintln!("{session_info} s_tx shutdown error {err}");
            }
            r
        },
        async move {
            let r = copy_and_record_traffic(&mut s_rx, &mut t_tx, false).await;
            if let Err(err) = t_tx.shutdown().await {
               eprintln!("{session_info} t_tx shutdown error {err}");
            }
            r
        },
    );
    println!("Ending {session_info} with {res:?}");

    Ok(())
}


 async fn handle_proxy_session(server: &mut TcpStream, proxy_handler: Arc<Mutex<dyn ProxyHandler>>) -> crate::Result<Option<SocketAddr>> {
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

                // crate::traffic_status::traffic_status_update(len, 0)?;

                // if let ProxyType::Socks4 | ProxyType::Socks5 = proxy_type {
                //     let s5addr = if let Some(domain_name) = &domain_name {
                //         Address::DomainAddress(domain_name.clone(), session_info.dst.port())
                //     } else {
                //         session_info.dst.into()
                //     };

                //     // Add SOCKS5 UDP header to the incoming data
                //     let mut s5_udp_data = Vec::<u8>::new();
                //     UdpHeader::new(0, s5addr).write_to_stream(&mut s5_udp_data)?;
                //     s5_udp_data.extend_from_slice(buf1);

                //     udp_server.write_all(&s5_udp_data).await?;
                // } else {
                //    udp_server.write_all(buf1).await?;
                // }
                udp_server.write_all(buf1).await?;

            }
            len = udp_server.read(&mut buf2) => {
                let len = len?;
                if len == 0 {
                    break;
                }
                let buf2 = &buf2[..len];

                // crate::traffic_status::traffic_status_update(0, len)?;

                // if let ProxyType::Socks4 | ProxyType::Socks5 = proxy_type {
                //     // Remove SOCKS5 UDP header from the server data
                //     let header = UdpHeader::retrieve_from_stream(&mut &buf2[..])?;
                //     let data = &buf2[header.len()..];

                //     let buf = if session_info.dst.port() == DNS_PORT {
                //         let mut message = dns::parse_data_to_dns_message(data, false)?;
                //         if !ipv6_enabled {
                //             dns::remove_ipv6_entries(&mut message);
                //         }
                //         message.to_vec()?
                //     } else {
                //         data.to_vec()
                //     };

                //     udp_stack.write_all(&buf).await?;
                // } else {
                //     udp_stack.write_all(buf2).await?;
                // }
                udp_stack.write_all(buf2).await?;
            }
        }
    }

    println!("Ending {session_info}");

    Ok(())
}


async fn handle_virtual_dns_session(mut udp: IpStackUdpStream, dns: Arc<Mutex<VirtualDns>>) -> crate::Result<()> {
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

async fn copy_and_record_traffic<R, W>(reader: &mut R, writer: &mut W, is_tx: bool) -> tokio::io::Result<u64>
where
    R: tokio::io::AsyncRead + Unpin + ?Sized,
    W: tokio::io::AsyncWrite + Unpin + ?Sized,
{
    let mut buf = vec![0; 8192];
    let mut total = 0;
    loop {
        match reader.read(&mut buf).await? {
            0 => break, // EOF
            n => {
                total += n as u64;
                // let (tx, rx) = if is_tx { (n, 0) } else { (0, n) };
                // if let Err(e) = crate::traffic_status::traffic_status_update(tx, rx) {
                //     log::debug!("Record traffic status error: {e}");
                // }
                writer.write_all(&buf[..n]).await?;
            }
        }
    }
    Ok(total)
}

async fn handle_dns_over_tcp_session(
    mut udp_stack: IpStackUdpStream,
    proxy_handler: Arc<Mutex<dyn ProxyHandler>>,
    socket_queue: Option<Arc<SocketQueue>>,
    ipv6_enabled: bool,
) -> crate::Result<()> {
    let (session_info, server_addr) = {
        let handler = proxy_handler.lock().await;

        (handler.get_session_info(), handler.get_server_addr())
    };

    let mut server = create_tcp_stream(&socket_queue, server_addr).await?;

    println!("Beginning {session_info}");

    let _ = handle_proxy_session(&mut server, proxy_handler).await?;

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

                _ = dns::parse_data_to_dns_message(buf1, false)?;

                // Insert the DNS message length in front of the payload
                let len = u16::try_from(buf1.len())?;
                let mut buf = Vec::with_capacity(std::mem::size_of::<u16>() + usize::from(len));
                buf.extend_from_slice(&len.to_be_bytes());
                buf.extend_from_slice(buf1);

                server.write_all(&buf).await?;

                // crate::traffic_status::traffic_status_update(buf.len(), 0)?;
            }
            len = server.read(&mut buf2) => {
                let len = len?;
                if len == 0 {
                    break;
                }
                let mut buf = buf2[..len].to_vec();

                // crate::traffic_status::traffic_status_update(0, len)?;

                let mut to_send: VecDeque<Vec<u8>> = VecDeque::new();
                loop {
                    if buf.len() < 2 {
                        break;
                    }
                    let len = u16::from_be_bytes([buf[0], buf[1]]) as usize;
                    if buf.len() < len + 2 {
                        break;
                    }

                    // remove the length field
                    let data = buf[2..len + 2].to_vec();

                    let mut message = dns::parse_data_to_dns_message(&data, false)?;

                    let name = dns::extract_domain_from_dns_message(&message)?;
                    let ip = dns::extract_ipaddr_from_dns_message(&message);
                    println!("DNS over TCP query result: {name} -> {ip:?}");

                    if !ipv6_enabled {
                        dns::remove_ipv6_entries(&mut message);
                    }

                    to_send.push_back(message.to_vec()?);
                    if len + 2 == buf.len() {
                        break;
                    }
                    buf = buf[len + 2..].to_vec();
                }

                while let Some(packet) = to_send.pop_front() {
                    udp_stack.write_all(&packet).await?;
                }
            }
        }
    }

    println!("Ending {session_info}");

    Ok(())
}

/// Run the proxy server
/// # Arguments
/// * `device` - The network device to use
/// * `mtu` - The MTU of the network device
/// * `args` - The arguments to use
/// * `shutdown_token` - The token to exit the server
/// # Returns
/// * The number of sessions while exiting
pub async fn run<D>(device: D, mtu: u16, args: Args, shutdown_token: CancellationToken) -> Result<usize>
where
    D: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
 
    // let server_addr = args.proxy.addr;
    // let key = args.proxy.credentials.clone();
    let dns_addr = args.dns_addr;
    let ipv6_enabled = args.ipv6_enabled;
    let virtual_dns = if args.dns == ArgDns::Virtual {
        Some(Arc::new(Mutex::new(VirtualDns::new(args.virtual_dns_pool))))
    } else {
        None
    };
 

    #[cfg(not(target_os = "linux"))]
    let socket_queue = None;

    // use socks5_impl::protocol::Version::{V4, V5};
    // let mgr: Arc<dyn ProxyHandlerManager> = match args.proxy.proxy_type {
    //     ProxyType::Socks5 => Arc::new(SocksProxyManager::new(server_addr, V5, key)),
    //     ProxyType::Socks4 => Arc::new(SocksProxyManager::new(server_addr, V4, key)),
    //     ProxyType::Http => Arc::new(HttpManager::new(server_addr, key)),
    //     ProxyType::None => Arc::new(NoProxyManager::new()),
    // };

    let mgr:  Arc<dyn ProxyHandlerManager> = Arc::new(NoProxyManager::new());

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
                    if args.exit_on_fatal_error {
                        println!("Too many sessions that over {max_sessions}, exiting...");
                        break;
                    }
                   println!("Too many sessions that over {max_sessions}, dropping new session");
                    continue;
                }
                // log::trace!("Session count {}", task_count.fetch_add(1, Relaxed).saturating_add(1));
                let info = SessionInfo::new(tcp.local_addr(), tcp.peer_addr(), IpProtocol::Tcp);
                let domain_name = if let Some(virtual_dns) = &virtual_dns {
                    let mut virtual_dns = virtual_dns.lock().await;
                    virtual_dns.touch_ip(&tcp.peer_addr().ip());
                    virtual_dns.resolve_ip(&tcp.peer_addr().ip()).cloned()
                } else {
                    None
                };
                let proxy_handler = mgr.new_proxy_handler(info, domain_name, false).await?;
                let socket_queue = socket_queue.clone();
                tokio::spawn(async move {
                    if let Err(err) = handle_tcp_session(tcp, proxy_handler, socket_queue).await {
                        eprintln!("{info} error \"{err}\"");
                    }
                    println!("Session count {}", task_count.fetch_sub(1, Relaxed).saturating_sub(1));
                });
            }
            IpStackStream::Udp(udp) => {
                if task_count.load(Relaxed) >= max_sessions {
                    if args.exit_on_fatal_error {
                       println!("Too many sessions that over {max_sessions}, exiting...");
                        break;
                    }
                    println!("Too many sessions that over {max_sessions}, dropping new session");
                    continue;
                }
                println!("Session count {}", task_count.fetch_add(1, Relaxed).saturating_add(1));
                let mut info = SessionInfo::new(udp.local_addr(), udp.peer_addr(), IpProtocol::Udp);
                if info.dst.port() == DNS_PORT {
                    if is_private_ip(info.dst.ip()) {
                        info.dst.set_ip(dns_addr); // !!! Here we change the destination address to remote DNS server!!!
                    }
                    if args.dns == ArgDns::OverTcp {
                        info.protocol = IpProtocol::Tcp;
                        let proxy_handler = mgr.new_proxy_handler(info, None, false).await?;
                        let socket_queue = socket_queue.clone();
                        tokio::spawn(async move {
                            if let Err(err) = handle_dns_over_tcp_session(udp, proxy_handler, socket_queue, ipv6_enabled).await {
                                eprintln!("{info} error \"{err}\"");
                            }
                            println!("Session count {}", task_count.fetch_sub(1, Relaxed).saturating_sub(1));
                        });
                        continue;
                    }
                    if args.dns == ArgDns::Virtual {
                        tokio::spawn(async move {
                            if let Some(virtual_dns) = virtual_dns {
                                if let Err(err) = handle_virtual_dns_session(udp, virtual_dns).await {
                                    eprintln!("{info} error \"{err}\"");
                                }
                            }
                            println!("Session count {}", task_count.fetch_sub(1, Relaxed).saturating_sub(1));
                        });
                        continue;
                    }
                    assert_eq!(args.dns, ArgDns::Direct);
                }
                let domain_name = if let Some(virtual_dns) = &virtual_dns {
                    let mut virtual_dns = virtual_dns.lock().await;
                    virtual_dns.touch_ip(&udp.peer_addr().ip());
                    virtual_dns.resolve_ip(&udp.peer_addr().ip()).cloned()
                } else {
                    None
                };
                 
                match mgr.new_proxy_handler(info, domain_name, true).await {
                    Ok(proxy_handler) => {
                        let socket_queue = socket_queue.clone();
                        tokio::spawn(async move {
                            // let ty = args.proxy.proxy_type;
                            if let Err(err) = handle_udp_associate_session(udp, proxy_handler, socket_queue).await {
                                eprintln!("Ending {info} with \"{err}\"");
                            }
                            println!("Session count {}", task_count.fetch_sub(1, Relaxed).saturating_sub(1));
                        });
                    }
                    Err(e) => {
                        eprintln!("Failed to create UDP connection: {e}");
                    }
                }
            }
            IpStackStream::UnknownTransport(u) => {
                let len = u.payload().len();
                println!("#0 unhandled transport - Ip Protocol {:?}, length {}", u.ip_protocol(), len);
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
