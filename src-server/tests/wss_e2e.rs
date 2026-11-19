use cloudtun_common::{ping::ping_cloudtun_proxy_server, tls::CloudEndpoint};
use cloudtun_proxy::{MatchType, PreparedProxy, ProxyArgs};
use cloudtun_server::{context::Context, tls::Identity};
use futures_util::{SinkExt, StreamExt};
use std::{sync::Arc, time::Duration};
use tokio::{
  io::{AsyncReadExt, AsyncWriteExt},
  net::{TcpListener, TcpStream},
  time::timeout,
};
use tokio_util::sync::CancellationToken;
struct Server {
  endpoint: CloudEndpoint,
  cancel: CancellationToken,
  task: tokio::task::JoinHandle<()>,
  directory: tempfile::TempDir,
}
impl Server {
  async fn start() -> Self {
    Self::at(0, None).await
  }
  async fn at(port: u16, token: Option<String>) -> Self {
    let directory = tempfile::tempdir().unwrap();
    let identity = Identity::load_or_create(directory.path(), token).unwrap();
    let context = Arc::new(Context::new(identity.token.clone(), "local".into(), None));
    let listener = TcpListener::bind(("127.0.0.1", port)).await.unwrap();
    let endpoint = CloudEndpoint::new(
      "127.0.0.1".into(),
      listener.local_addr().unwrap().port(),
      "local-instance".into(),
      identity.token.clone(),
      directory.path().join("client-trust"),
    )
    .unwrap();
    let cancel = CancellationToken::new();
    let token = cancel.clone();
    let tls = identity.config().unwrap();
    let task = tokio::spawn(async move {
      cloudtun_server::serve(listener, cloudtun_server::router(context), tls, token)
        .await
        .unwrap()
    });
    Self {
      endpoint,
      cancel,
      task,
      directory,
    }
  }
  async fn stop(self) {
    self.cancel.cancel();
    self.task.await.unwrap();
  }
}
async fn proxy(
  server: CloudEndpoint,
) -> (
  std::net::SocketAddr,
  CancellationToken,
  tokio::task::JoinHandle<()>,
) {
  // Force the remote hop even for loopback destinations, rather than accidentally testing a direct route.
  let proxy = PreparedProxy::bind(ProxyArgs {
    server,
    local_addr: ("127.0.0.1".into(), 0),
    default_rule: MatchType::Proxy,
    proxy_rules: None,
  })
  .await
  .unwrap();
  let addr = proxy.local_addr().unwrap();
  let cancel = CancellationToken::new();
  let token = cancel.clone();
  let task = tokio::spawn(async move { proxy.run(token, |_, _| {}).await.unwrap() });
  (addr, cancel, task)
}
async fn headers(client: &mut TcpStream) -> Vec<u8> {
  timeout(Duration::from_secs(5), async {
    let mut bytes = Vec::new();
    while !bytes.ends_with(b"\r\n\r\n") {
      bytes.push(client.read_u8().await.unwrap());
    }
    bytes
  })
  .await
  .unwrap()
}
#[tokio::test]
async fn first_trust_persists_restart_matches_and_changed_certificate_is_rejected() {
  let server = Server::start().await;
  assert!(ping_cloudtun_proxy_server(&server.endpoint).await.unwrap());
  assert!(
    ping_cloudtun_proxy_server(&server.endpoint.clone())
      .await
      .unwrap()
  );
  let pin_files: Vec<_> = std::fs::read_dir(&server.endpoint.trust_dir)
    .unwrap()
    .map(|e| e.unwrap().path())
    .collect();
  assert_eq!(pin_files.len(), 1);
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
      std::fs::metadata(&pin_files[0])
        .unwrap()
        .permissions()
        .mode()
        & 0o077,
      0
    );
  }
  let one = Identity::load_or_create(server.directory.path(), None).unwrap();
  let two = Identity::load_or_create(server.directory.path(), None).unwrap();
  assert_eq!(one.token, two.token);
  assert!(Identity::load_or_create(server.directory.path(), Some("aa".repeat(32))).is_err());
  let replacement = Server::at(0, Some(server.endpoint.token.clone())).await;
  let mut endpoint = server.endpoint.clone();
  endpoint.port = replacement.endpoint.port;
  let error = endpoint.connect().await.unwrap_err();
  assert!(error.to_string().contains("证书"));
  endpoint.identity = "another-instance".into();
  assert!(ping_cloudtun_proxy_server(&endpoint).await.unwrap());
  std::fs::write(&pin_files[0], b"corrupt").unwrap();
  assert!(server.endpoint.connect().await.is_err());
  assert_eq!(std::fs::read(&pin_files[0]).unwrap(), b"corrupt");
  replacement.stop().await;
  server.stop().await;
}
#[tokio::test]
async fn wrong_access_token_is_rejected_over_tls() {
  let server = Server::start().await;
  let mut wrong = server.endpoint.clone();
  wrong.token = "ff".repeat(32);
  assert!(!ping_cloudtun_proxy_server(&wrong).await.unwrap());
  let (stream, _application) = tokio::io::duplex(1024);
  let result = cloudtun_common::proxy::proxy_to_cloudtun_server(
    stream,
    Arc::new(wrong),
    "127.0.0.1".into(),
    1,
    Arc::new(|_: &str, _: &str| {}),
  )
  .await;
  assert!(result.is_err());
  assert!(ping_cloudtun_proxy_server(&server.endpoint).await.unwrap());
  server.stop().await;
}
#[tokio::test]
async fn concurrent_first_trust_accepts_only_one_certificate() {
  let first = Server::start().await;
  let second = Server::start().await;
  let mut other = second.endpoint.clone();
  other.trust_dir = first.endpoint.trust_dir.clone();
  let (a, b) = tokio::join!(first.endpoint.connect(), other.connect());
  assert_ne!(a.is_ok(), b.is_ok());
  first.stop().await;
  second.stop().await;
}
#[tokio::test]
async fn full_http_proxy_wss_sse_streams_before_origin_finishes() {
  let server = Server::start().await;
  let origin = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let origin_addr = origin.local_addr().unwrap();
  let (release, wait) = tokio::sync::oneshot::channel();
  let origin_task = tokio::spawn(async move {
    let (mut socket, _) = origin.accept().await.unwrap();
    let request = headers(&mut socket).await;
    assert!(request.starts_with(b"GET /events HTTP/1.1"));
    assert!(
      !String::from_utf8_lossy(&request)
        .to_ascii_lowercase()
        .contains("proxy-authorization")
    );
    socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\nd\r\ndata: first\n\n\r\n").await.unwrap();
    wait.await.unwrap();
    socket
      .write_all(b"e\r\ndata: second\n\n\r\n0\r\n\r\n")
      .await
      .unwrap();
  });
  let (addr, cancel, task) = proxy(server.endpoint.clone()).await;
  let mut client = TcpStream::connect(addr).await.unwrap();
  client.write_all(format!("GET http://{origin_addr}/events HTTP/1.1\r\nHost: {origin_addr}\r\nProxy-Authorization: Basic must-not-leak\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
  let mut first = Vec::new();
  timeout(Duration::from_secs(5), async {
    while !first.ends_with(b"data: first\n\n") {
      first.push(client.read_u8().await.unwrap());
    }
  })
  .await
  .unwrap();
  assert!(server.endpoint.trust_dir.exists());
  release.send(()).unwrap();
  let mut rest = Vec::new();
  timeout(Duration::from_secs(5), client.read_to_end(&mut rest))
    .await
    .unwrap()
    .unwrap();
  assert!(String::from_utf8_lossy(&rest).contains("data: second"));
  origin_task.await.unwrap();
  cancel.cancel();
  task.await.unwrap();
  server.stop().await;
}
#[tokio::test]
async fn full_http_proxy_wss_websocket_echoes_binary_ping_and_close() {
  let server = Server::start().await;
  let origin = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let origin_addr = origin.local_addr().unwrap();
  let origin_task = tokio::spawn(async move {
    let (tcp, _) = origin.accept().await.unwrap();
    let mut ws = tokio_tungstenite::accept_async(tcp).await.unwrap();
    while let Some(message) = ws.next().await {
      match message.unwrap() {
        message @ (tokio_tungstenite::tungstenite::Message::Text(_)
        | tokio_tungstenite::tungstenite::Message::Binary(_)) => ws.send(message).await.unwrap(),
        tokio_tungstenite::tungstenite::Message::Ping(_) => ws.flush().await.unwrap(),
        tokio_tungstenite::tungstenite::Message::Close(_) => {
          ws.flush().await.unwrap();
          break;
        }
        _ => {}
      }
    }
  });
  let (addr, cancel, task) = proxy(server.endpoint.clone()).await;
  let mut tcp = TcpStream::connect(addr).await.unwrap();
  tcp.write_all(format!("GET ws://{origin_addr}/socket HTTP/1.1\r\nHost: {origin_addr}\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n").as_bytes()).await.unwrap();
  assert!(headers(&mut tcp).await.starts_with(b"HTTP/1.1 101"));
  use tokio_tungstenite::tungstenite::{Message, protocol::Role};
  let mut ws = tokio_tungstenite::WebSocketStream::from_raw_socket(tcp, Role::Client, None).await;
  for message in [
    Message::Text("hello".into()),
    Message::Binary(vec![7; 128 * 1024].into()),
    Message::Ping(b"alive".to_vec().into()),
  ] {
    ws.send(message.clone()).await.unwrap();
    let response = timeout(Duration::from_secs(5), ws.next())
      .await
      .unwrap()
      .unwrap()
      .unwrap();
    if let Message::Ping(bytes) = message {
      assert_eq!(response, Message::Pong(bytes));
    } else {
      assert_eq!(response, message);
    }
  }
  ws.close(None).await.unwrap();
  assert!(matches!(
    ws.next().await.unwrap().unwrap(),
    Message::Close(_)
  ));
  origin_task.await.unwrap();
  cancel.cancel();
  task.await.unwrap();
  server.stop().await;
}
