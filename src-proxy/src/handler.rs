use crate::{
  outbound::{DynamicOutbound, Outbound},
  proxy::proxy_request,
  route::{MatchType, RouteMatcher},
  routing::{Routing, RoutingMode},
  subscription::SubscriptionNode,
  traffic::{MeteredStream, TrafficCounter},
};
use axum::extract::Request;
use hyper::{body::Incoming, server::conn::http1};
use hyper_util::rt::TokioIo;
use std::sync::Arc;
use tokio::{net::TcpListener, task::JoinSet};
use tokio_util::sync::CancellationToken;

// macOS can permit a specific IPv4 bind alongside an existing IPv6 wildcard
// listener with SO_REUSEADDR. Detect it before taking over another proxy's traffic.
async fn ensure_local_port_available(port: u16) -> std::io::Result<()> {
  if port != 0
    && matches!(
      tokio::time::timeout(
        std::time::Duration::from_millis(200),
        tokio::net::TcpStream::connect(("127.0.0.1", port))
      )
      .await,
      Ok(Ok(_))
    )
  {
    return Err(std::io::Error::new(
      std::io::ErrorKind::AddrInUse,
      format!("本地端口 {port} 已被占用，请先关闭其他代理程序"),
    ));
  }
  Ok(())
}

pub struct ProxyArgs {
  pub server: cloudtun_common::tls::CloudEndpoint,
  pub local_addr: (String, u16),
  pub default_rule: MatchType,
  pub proxy_rules: Option<String>,
}
pub struct PreparedProxy {
  traffic: TrafficCounter,
  outbound: DynamicOutbound,
  routing: Routing,
  listener: TcpListener,
  control: ProxyListenerControl,
  updates: tokio::sync::mpsc::Receiver<ListenerUpdate>,
}
struct ListenerUpdate {
  host: String,
  port: u16,
  reply: tokio::sync::oneshot::Sender<std::io::Result<()>>,
}
#[derive(Clone)]
pub struct ProxyListenerControl(
  tokio::sync::mpsc::Sender<ListenerUpdate>,
  Routing,
  DynamicOutbound,
);
impl ProxyListenerControl {
  pub fn set_node(&self, node: SubscriptionNode) -> std::io::Result<()> {
    self.2.set_node(node)
  }
  pub fn set_routing_mode(&self, mode: RoutingMode) {
    self.1.set_mode(mode);
  }
  pub async fn rebind(&self, host: &str, port: u16) -> std::io::Result<()> {
    let (reply, receive) = tokio::sync::oneshot::channel();
    self
      .0
      .send(ListenerUpdate {
        host: host.into(),
        port,
        reply,
      })
      .await
      .map_err(|_| std::io::Error::other("代理已停止"))?;
    receive
      .await
      .map_err(|_| std::io::Error::other("代理已停止"))?
  }
}
async fn rebind_listener(
  listener: &mut Option<TcpListener>,
  host: &str,
  port: u16,
) -> std::io::Result<()> {
  let old = listener.as_ref().unwrap().local_addr()?;
  let ip: std::net::IpAddr = host.parse().map_err(std::io::Error::other)?;
  if old == std::net::SocketAddr::new(ip, port) {
    return Ok(());
  }
  let same_port = old.port() == port;
  if same_port {
    drop(listener.take());
  }
  let result = async {
    ensure_local_port_available(port).await?;
    TcpListener::bind((host, port)).await
  }
  .await;
  match result {
    Ok(next) => {
      *listener = Some(next);
      Ok(())
    }
    Err(error) => {
      if same_port {
        *listener = Some(TcpListener::bind(old).await.map_err(|rollback| {
          std::io::Error::other(format!("监听修改失败：{error}；恢复监听失败：{rollback}"))
        })?);
      }
      Err(error)
    }
  }
}
impl PreparedProxy {
  fn prepared(outbound: Outbound, routing: Routing, listener: TcpListener) -> Self {
    let (send, updates) = tokio::sync::mpsc::channel(8);
    let outbound = DynamicOutbound::new(outbound);
    let control = ProxyListenerControl(send, routing.clone(), outbound.clone());
    Self {
      traffic: TrafficCounter::new(),
      outbound,
      routing,
      listener,
      control,
      updates,
    }
  }
  pub fn listener_control(&self) -> ProxyListenerControl {
    self.control.clone()
  }
  pub fn connector(&self) -> crate::ProxyConnector {
    crate::ProxyConnector::with_shared(
      self.outbound.clone(),
      self.routing.clone(),
      self.traffic.clone(),
    )
  }
  pub fn traffic_counter(&self) -> TrafficCounter {
    self.traffic.clone()
  }

  pub async fn bind(args: ProxyArgs) -> std::io::Result<Self> {
    let routing =
      Routing::custom(RouteMatcher::load(args.default_rule, args.proxy_rules.clone()).await?);
    Self::bind_routing(args, routing).await
  }
  pub async fn bind_with_rules(
    args: ProxyArgs,
    mode: RoutingMode,
    rules: Arc<crate::china::ChinaRulesStore>,
  ) -> std::io::Result<Self> {
    Self::bind_routing(args, Routing::with_rules(mode, rules)).await
  }
  async fn bind_routing(args: ProxyArgs, routing: Routing) -> std::io::Result<Self> {
    if matches!(
      args.local_addr.0.as_str(),
      "127.0.0.1" | "localhost" | "0.0.0.0"
    ) {
      ensure_local_port_available(args.local_addr.1).await?;
    }
    let listener = TcpListener::bind(&args.local_addr).await?;
    Ok(Self::prepared(
      Outbound::Cloud {
        server: Arc::new(args.server),
      },
      routing,
      listener,
    ))
  }
  pub async fn bind_node(
    node: SubscriptionNode,
    mode: RoutingMode,
    port: u16,
  ) -> std::io::Result<Self> {
    Self::bind_node_at(node, mode, "0.0.0.0", port).await
  }
  pub async fn bind_node_at(
    node: SubscriptionNode,
    mode: RoutingMode,
    host: &str,
    port: u16,
  ) -> std::io::Result<Self> {
    Self::bind_node_with_rules(node, mode, host, port, Arc::default()).await
  }
  pub async fn bind_node_with_rules(
    node: SubscriptionNode,
    mode: RoutingMode,
    host: &str,
    port: u16,
    rules: Arc<crate::china::ChinaRulesStore>,
  ) -> std::io::Result<Self> {
    if node.unsupported_reason.is_some() || node.vmess.is_none() {
      return Err(std::io::Error::other("unsupported node"));
    }
    ensure_local_port_available(port).await?;
    Ok(Self::prepared(
      Outbound::Vmess(node),
      Routing::with_rules(mode, rules),
      TcpListener::bind((host, port)).await?,
    ))
  }
  pub fn local_addr(&self) -> std::io::Result<std::net::SocketAddr> {
    self.listener.local_addr()
  }
  pub async fn run<F: Fn(&str, &str) + Send + Sync + 'static>(
    self,
    shutdown_token: CancellationToken,
    log_fn: F,
  ) -> Result<(), Box<dyn std::error::Error>> {
    let Self {
      traffic,
      outbound,
      routing,
      listener,
      control: _keep_control,
      mut updates,
    } = self;
    let connection_cancel = shutdown_token.child_token();
    let mut fatal = None;
    let mut listener = Some(listener);
    let log_fn = Arc::new(log_fn);
    let mut tasks = JoinSet::new();
    let tunnels = tokio_util::task::TaskTracker::new();
    log_fn(
      "proxy::info",
      &format!(
        "HTTP proxy listening at {}",
        listener.as_ref().unwrap().local_addr()?
      ),
    );
    loop {
      tokio::select! {
        biased;
        _ = shutdown_token.cancelled() => break,
        _ = tasks.join_next(), if !tasks.is_empty() => {},
        Some(update) = updates.recv() => {
          let result = rebind_listener(&mut listener, &update.host, update.port).await;
          if result.is_ok() { log_fn("proxy::info", &format!("HTTP proxy listening at {}", listener.as_ref().unwrap().local_addr()?)); }
          if listener.is_none() { fatal = Some(result.as_ref().unwrap_err().to_string()); }
          let _ = update.reply.send(result);
          if fatal.is_some() { break; }
        },
        accepted = listener.as_ref().unwrap().accept() => {
          let (stream, _) = match accepted {
            Ok(value) => value,
            Err(error) => {
              log_fn("proxy::error", &format!("accept failed: {error}"));
              tokio::select! { _ = shutdown_token.cancelled() => break, _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {} }
              continue;
            }
          };
          let _ = stream.set_nodelay(true);
          let stream = MeteredStream(stream, traffic.clone());
          let tracker = tunnels.clone(); let cancel = connection_cancel.clone();
          let outbound = outbound.clone(); let routing = routing.clone(); let log = log_fn.clone();
          tasks.spawn(async move {
            let service_cancel = cancel.clone(); let service_log = log.clone();
            let service = hyper::service::service_fn(move |req: Request<Incoming>| {
              proxy_request(req, outbound.snapshot().0, routing.clone(), service_log.clone(), service_cancel.clone(), tracker.clone())
            });
            tokio::select! {
              biased;
              _ = cancel.cancelled() => {},
              result = http1::Builder::new().serve_connection(TokioIo::new(stream), service).with_upgrades() => {
                if let Err(error) = result { log("proxy::error", &format!("HTTP connection closed: {error}")); }
              }
            }
          });
        }
      }
    }
    connection_cancel.cancel();
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    tunnels.close();
    tunnels.wait().await;
    if let Some(error) = fatal {
      return Err(std::io::Error::other(error).into());
    }
    Ok(())
  }
}
pub async fn run_proxy_loop<F: Fn(&str, &str) + Send + Sync + 'static>(
  args: ProxyArgs,
  shutdown_token: CancellationToken,
  log_fn: F,
) -> Result<(), Box<dyn std::error::Error>> {
  PreparedProxy::bind(args)
    .await?
    .run(shutdown_token, log_fn)
    .await
}

#[cfg(test)]
mod tests {
  use super::*;
  use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    time::{Duration, timeout},
  };
  fn args() -> ProxyArgs {
    ProxyArgs {
      server: cloudtun_common::tls::CloudEndpoint::new(
        "127.0.0.1".into(),
        1,
        "test".into(),
        "01".repeat(32),
        std::env::temp_dir().join("cloudtun-unused-trust"),
      )
      .unwrap(),
      local_addr: ("127.0.0.1".into(), 0),
      default_rule: MatchType::Direct,
      proxy_rules: None,
    }
  }

  #[tokio::test]
  async fn forwards_http_websocket_upgrade_and_bidirectional_frames() {
    for scheme in ["http", "ws"] {
      websocket_upgrade_roundtrip(scheme).await;
    }
  }
  async fn websocket_upgrade_roundtrip(scheme: &str) {
    let target = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target_addr = target.local_addr().unwrap();
    let origin = tokio::spawn(async move {
      let (mut socket, _) = target.accept().await.unwrap();
      let mut header = Vec::new();
      while !header.ends_with(b"\r\n\r\n") {
        header.push(socket.read_u8().await.unwrap());
      }
      let header = String::from_utf8(header).unwrap().to_ascii_lowercase();
      assert!(header.contains("upgrade: websocket"));
      assert!(header.contains("connection: upgrade"));
      assert!(!header.contains("proxy-authorization"));
      socket.write_all(b"HTTP/1.1 101 Switching Protocols\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=\r\n\r\n").await.unwrap();
      let mut frame = [0; 10];
      socket.read_exact(&mut frame).await.unwrap();
      assert_eq!(&frame[..6], &[0x81, 0x84, 1, 2, 3, 4]);
      socket
        .write_all(&[0x81, 4, b'p', b'o', b'n', b'g'])
        .await
        .unwrap();
      let mut byte = [0];
      assert_eq!(socket.read(&mut byte).await.unwrap(), 0);
    });
    let proxy = PreparedProxy::bind(args()).await.unwrap();
    let traffic = proxy.traffic_counter();
    let addr = proxy.local_addr().unwrap();
    let cancel = CancellationToken::new();
    let token = cancel.clone();
    let task = tokio::spawn(async move { proxy.run(token, |_, _| {}).await.unwrap() });
    let mut client = TcpStream::connect(addr).await.unwrap();
    client.write_all(format!("GET {scheme}://{target_addr}/ws HTTP/1.1\r\nHost: {target_addr}\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\nProxy-Authorization: Basic secret\r\n\r\n").as_bytes()).await.unwrap();
    let mut header = Vec::new();
    timeout(Duration::from_secs(3), async {
      while !header.ends_with(b"\r\n\r\n") {
        header.push(client.read_u8().await.unwrap());
      }
    })
    .await
    .unwrap();
    assert!(
      header.starts_with(b"HTTP/1.1 101"),
      "{}",
      String::from_utf8_lossy(&header)
    );
    let before_frames = traffic.snapshot().total_bytes;
    client
      .write_all(&[
        0x81,
        0x84,
        1,
        2,
        3,
        4,
        b't' ^ 1,
        b'e' ^ 2,
        b's' ^ 3,
        b't' ^ 4,
      ])
      .await
      .unwrap();
    let mut frame = [0; 6];
    timeout(Duration::from_secs(3), client.read_exact(&mut frame))
      .await
      .unwrap()
      .unwrap();
    assert_eq!(&frame, &[0x81, 4, b'p', b'o', b'n', b'g']);
    assert_eq!(traffic.snapshot().total_bytes - before_frames, 16);
    cancel.cancel();
    let mut byte = [0];
    assert_eq!(
      timeout(Duration::from_secs(3), client.read(&mut byte))
        .await
        .unwrap()
        .unwrap(),
      0
    );
    origin.await.unwrap();
    task.await.unwrap();
  }

  #[tokio::test]
  async fn forwards_sse_events_before_the_response_finishes() {
    let target = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target_addr = target.local_addr().unwrap();
    let (release, wait) = tokio::sync::oneshot::channel();
    let origin = tokio::spawn(async move {
      let (mut socket, _) = target.accept().await.unwrap();
      let mut header = Vec::new();
      while !header.ends_with(b"\r\n\r\n") {
        header.push(socket.read_u8().await.unwrap());
      }
      socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\nd\r\ndata: first\n\n\r\n").await.unwrap();
      wait.await.unwrap();
      socket
        .write_all(b"e\r\ndata: second\n\n\r\n0\r\n\r\n")
        .await
        .unwrap();
    });
    let proxy = PreparedProxy::bind(args()).await.unwrap();
    let traffic = proxy.traffic_counter();
    let initial = traffic.snapshot();
    assert_eq!(initial.total_bytes, 0);
    let addr = proxy.local_addr().unwrap();
    let cancel = CancellationToken::new();
    let token = cancel.clone();
    let task = tokio::spawn(async move { proxy.run(token, |_, _| {}).await.unwrap() });
    let mut client = TcpStream::connect(addr).await.unwrap();
    client.write_all(format!("GET http://{target_addr}/sse HTTP/1.1\r\nHost: {target_addr}\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
    let mut first = Vec::new();
    timeout(Duration::from_secs(3), async {
      while !first.ends_with(b"data: first\n\n") {
        first.push(client.read_u8().await.unwrap());
      }
    })
    .await
    .unwrap();
    let streaming = traffic.snapshot();
    assert!(streaming.total_bytes > first.len() as u64);
    release.send(()).unwrap();
    let mut rest = Vec::new();
    timeout(Duration::from_secs(3), client.read_to_end(&mut rest))
      .await
      .unwrap()
      .unwrap();
    assert!(String::from_utf8_lossy(&rest).contains("data: second"));
    assert!(traffic.snapshot().total_bytes > streaming.total_bytes);
    origin.await.unwrap();
    cancel.cancel();
    task.await.unwrap();
    let next = PreparedProxy::bind(args())
      .await
      .unwrap()
      .traffic_counter()
      .snapshot();
    assert_ne!(next.session_id, initial.session_id);
    assert_eq!(next.total_bytes, 0);
  }

  #[tokio::test]
  async fn cloud_uses_shared_geo_rules_and_changes_mode_for_existing_handlers() {
    use crate::china::{ChinaRules, ChinaRulesStore};
    let rules = Arc::new(ChinaRulesStore::default());
    let proxy = PreparedProxy::bind_with_rules(args(), RoutingMode::BypassChina, rules.clone())
      .await
      .unwrap();
    let existing_handler = proxy.routing.clone();
    // The legacy args default is Direct; the application's shared routing overrides it.
    assert!(matches!(
      existing_handler.decide("114.114.114.114", 80).await,
      MatchType::Proxy
    ));
    rules.replace(
      ChinaRules::parse("114.114.114.0/24", "240e::/16", "server=/baidu.com/1.1.1.1").unwrap(),
    );
    assert!(matches!(
      existing_handler.decide("114.114.114.114", 80).await,
      MatchType::Direct
    ));
    let control = proxy.listener_control();
    control.set_routing_mode(RoutingMode::Global);
    assert!(matches!(
      existing_handler.decide("114.114.114.114", 80).await,
      MatchType::Proxy
    ));
    assert!(matches!(
      existing_handler.decide("127.0.0.1", 80).await,
      MatchType::Direct
    ));
    control.set_routing_mode(RoutingMode::BypassChina);
    assert!(matches!(
      existing_handler.decide("baidu.com", 80).await,
      MatchType::Direct
    ));
  }

  #[tokio::test]
  async fn live_listener_changes_keep_tunnels_and_reject_occupied_ports() {
    let target = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target_addr = target.local_addr().unwrap();
    let proxy = PreparedProxy::bind(args()).await.unwrap();
    let old_addr = proxy.local_addr().unwrap();
    let control = proxy.listener_control();
    let cancel = CancellationToken::new();
    let token = cancel.clone();
    let task = tokio::spawn(async move { proxy.run(token, |_, _| {}).await.unwrap() });
    let mut client = TcpStream::connect(old_addr).await.unwrap();
    client
      .write_all(
        format!("CONNECT {target_addr} HTTP/1.1\r\nHost: {target_addr}\r\n\r\n").as_bytes(),
      )
      .await
      .unwrap();
    let mut header = Vec::new();
    while !header.ends_with(b"\r\n\r\n") {
      header.push(
        timeout(Duration::from_secs(2), client.read_u8())
          .await
          .unwrap()
          .unwrap(),
      );
    }
    assert!(header.starts_with(b"HTTP/1.1 200"));
    let (mut remote, _) = target.accept().await.unwrap();
    let reserve = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let new_port = reserve.local_addr().unwrap().port();
    drop(reserve);
    control.rebind("127.0.0.1", new_port).await.unwrap();
    assert!(TcpStream::connect(old_addr).await.is_err());
    TcpStream::connect(("127.0.0.1", new_port)).await.unwrap();
    control.rebind("0.0.0.0", new_port).await.unwrap();
    client.write_all(b"active").await.unwrap();
    let mut bytes = [0; 6];
    timeout(Duration::from_secs(2), remote.read_exact(&mut bytes))
      .await
      .unwrap()
      .unwrap();
    assert_eq!(&bytes, b"active");
    let occupied = TcpListener::bind("127.0.0.1:0").await.unwrap();
    assert!(
      control
        .rebind("127.0.0.1", occupied.local_addr().unwrap().port())
        .await
        .is_err()
    );
    TcpStream::connect(("127.0.0.1", new_port)).await.unwrap();
    cancel.cancel();
    timeout(Duration::from_secs(2), task)
      .await
      .unwrap()
      .unwrap();
  }
  #[tokio::test]
  async fn forwards_plain_http_path_and_body_without_proxy_credentials() {
    let target = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target_addr = target.local_addr().unwrap();
    let origin = tokio::spawn(async move {
      let (mut socket, _) = target.accept().await.unwrap();
      let mut request = Vec::new();
      while !request.ends_with(b"\r\n\r\n") {
        request.push(socket.read_u8().await.unwrap());
      }
      let header = String::from_utf8(request).unwrap().to_lowercase();
      assert!(header.starts_with("post /path?q=1 http/1.1"));
      assert!(!header.contains("proxy-authorization"));
      assert!(!header.contains("x-private"));
      assert!(!header.contains("proxy-connection"));
      let mut body = [0; 4];
      socket.read_exact(&mut body).await.unwrap();
      assert_eq!(&body, b"data");
      socket
        .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
        .await
        .unwrap();
    });
    let proxy = PreparedProxy::bind(args()).await.unwrap();
    let addr = proxy.local_addr().unwrap();
    let cancel = CancellationToken::new();
    let token = cancel.clone();
    let task = tokio::spawn(async move { proxy.run(token, |_, _| {}).await.unwrap() });
    let mut client = TcpStream::connect(addr).await.unwrap();
    client.write_all(format!("POST http://{target_addr}/path?q=1 HTTP/1.1\r\nHost: wrong.example\r\nProxy-Authorization: Basic secret\r\nProxy-Connection: keep-alive\r\nConnection: X-Private, close\r\nX-Private: secret\r\nContent-Length: 4\r\n\r\ndata").as_bytes()).await.unwrap();
    let mut response = Vec::new();
    timeout(Duration::from_secs(3), client.read_to_end(&mut response))
      .await
      .unwrap()
      .unwrap();
    assert!(response.starts_with(b"HTTP/1.1 200"));
    assert!(response.ends_with(b"ok"));
    origin.await.unwrap();
    cancel.cancel();
    task.await.unwrap();
  }
  #[tokio::test]
  async fn stop_closes_accepted_connections_and_releases_port() {
    let proxy = PreparedProxy::bind(args()).await.unwrap();
    let addr = proxy.listener.local_addr().unwrap();
    let cancel = CancellationToken::new();
    let token = cancel.clone();
    let task = tokio::spawn(async move { proxy.run(token, |_, _| {}).await.unwrap() });
    let mut client = TcpStream::connect(addr).await.unwrap();
    // A response confirms accept and request processing happened before cancellation.
    client
      .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n")
      .await
      .unwrap();
    let mut bytes = [0; 1024];
    timeout(Duration::from_secs(2), client.read(&mut bytes))
      .await
      .unwrap()
      .unwrap();
    cancel.cancel();
    task.await.unwrap();
    assert_eq!(
      timeout(Duration::from_secs(2), client.read(&mut bytes))
        .await
        .unwrap()
        .unwrap(),
      0
    );
    TcpListener::bind(addr).await.unwrap();
  }
  #[tokio::test]
  async fn bind_failure_is_reported_before_start() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut args = args();
    args.local_addr.1 = listener.local_addr().unwrap().port();
    assert!(PreparedProxy::bind(args).await.is_err());
  }
  #[tokio::test]
  async fn stop_waits_for_upgraded_tunnels_to_close() {
    let target = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target_addr = target.local_addr().unwrap();
    let proxy = PreparedProxy::bind(args()).await.unwrap();
    let traffic = proxy.traffic_counter();
    let addr = proxy.listener.local_addr().unwrap();
    let cancel = CancellationToken::new();
    let token = cancel.clone();
    let task = tokio::spawn(async move { proxy.run(token, |_, _| {}).await.unwrap() });
    let mut client = TcpStream::connect(addr).await.unwrap();
    client
      .write_all(
        format!("CONNECT {target_addr} HTTP/1.1\r\nHost: {target_addr}\r\n\r\n").as_bytes(),
      )
      .await
      .unwrap();
    let mut header = Vec::new();
    while !header.ends_with(b"\r\n\r\n") {
      header.push(
        timeout(Duration::from_secs(2), client.read_u8())
          .await
          .unwrap()
          .unwrap(),
      );
    }
    assert!(header.starts_with(b"HTTP/1.1 200"));
    let before_tunnel = traffic.snapshot().total_bytes;
    let (mut remote, _) = timeout(Duration::from_secs(2), target.accept())
      .await
      .unwrap()
      .unwrap();
    client.write_all(b"connected").await.unwrap();
    let mut data = [0; 9];
    timeout(Duration::from_secs(2), remote.read_exact(&mut data))
      .await
      .unwrap()
      .unwrap();
    remote.write_all(b"reply").await.unwrap();
    let mut reply = [0; 5];
    timeout(Duration::from_secs(2), client.read_exact(&mut reply))
      .await
      .unwrap()
      .unwrap();
    assert_eq!(&reply, b"reply");
    assert_eq!(traffic.snapshot().total_bytes - before_tunnel, 14);
    cancel.cancel();
    timeout(Duration::from_secs(2), task)
      .await
      .unwrap()
      .unwrap();
    let mut byte = [0];
    assert_eq!(
      timeout(Duration::from_secs(2), remote.read(&mut byte))
        .await
        .unwrap()
        .unwrap(),
      0
    );
    assert_eq!(
      timeout(Duration::from_secs(2), client.read(&mut byte))
        .await
        .unwrap()
        .unwrap(),
      0
    );
    TcpListener::bind(addr).await.unwrap();
  }
}
