use crate::subscription::SubscriptionNode;
use leaf::{
  proxy::OutboundStreamHandler,
  session::{Session, SocksAddr},
};
use std::{
  collections::HashMap,
  io,
  pin::Pin,
  sync::Arc,
  task::{Context, Poll},
};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

pub trait ProxyStream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> ProxyStream for T {}
pub type AnyStream = Box<dyn ProxyStream>;

fn tls_config() -> io::Result<tokio_rustls::rustls::ClientConfig> {
  let roots =
    tokio_rustls::rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
  let provider = Arc::new(tokio_rustls::rustls::crypto::ring::default_provider());
  let config = tokio_rustls::rustls::ClientConfig::builder_with_provider(provider)
    .with_safe_default_protocol_versions()
    .map_err(io::Error::other)?
    .with_root_certificates(roots)
    .with_no_client_auth();
  Ok(config)
}

fn tls_connector() -> io::Result<tokio_rustls::TlsConnector> {
  Ok(tokio_rustls::TlsConnector::from(Arc::new(tls_config()?)))
}

/// Android has no platform-verifier JVM component in this Rust-only HTTP path.
/// Use the same Mozilla root store as VMess TLS, keeping certificate/name validation.
pub fn http_client_builder() -> io::Result<reqwest::ClientBuilder> {
  let builder = reqwest::Client::builder();
  #[cfg(target_os = "android")]
  let builder = builder.tls_backend_preconfigured(tls_config()?);
  Ok(builder)
}

struct VmessStream(leaf::proxy::AnyStream);

fn vmess_io_error(error: io::Error) -> io::Error {
  // Leaf's WebSocket adapter reports a closed pipe as Interrupted.
  // Tokio's copy loop retries Interrupted immediately, even on a closed socket.
  if error.kind() == io::ErrorKind::Interrupted {
    io::Error::new(io::ErrorKind::ConnectionAborted, error)
  } else {
    error
  }
}

impl AsyncRead for VmessStream {
  fn poll_read(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
    buf: &mut ReadBuf<'_>,
  ) -> Poll<io::Result<()>> {
    Pin::new(&mut self.0)
      .poll_read(cx, buf)
      .map_err(vmess_io_error)
  }
}

impl AsyncWrite for VmessStream {
  fn poll_write(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
    buf: &[u8],
  ) -> Poll<io::Result<usize>> {
    Pin::new(&mut self.0)
      .poll_write(cx, buf)
      .map_err(vmess_io_error)
  }

  fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
    Pin::new(&mut self.0).poll_flush(cx).map_err(vmess_io_error)
  }

  fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
    Pin::new(&mut self.0)
      .poll_shutdown(cx)
      .map_err(vmess_io_error)
  }
}

#[derive(Clone)]
pub(crate) enum Outbound {
  Cloud {
    server: Arc<cloudtun_common::tls::CloudEndpoint>,
  },
  Vmess(SubscriptionNode),
}
#[derive(Clone)]
pub(crate) struct DynamicOutbound(Arc<std::sync::RwLock<(Outbound, CancellationToken)>>);
impl DynamicOutbound {
  pub fn new(outbound: Outbound) -> Self {
    Self(Arc::new(std::sync::RwLock::new((
      outbound,
      CancellationToken::new(),
    ))))
  }
  pub fn snapshot(&self) -> (Outbound, CancellationToken) {
    self.0.read().unwrap_or_else(|e| e.into_inner()).clone()
  }
  pub fn set_node(&self, node: SubscriptionNode) -> io::Result<()> {
    if node.unsupported_reason.is_some() || node.vmess.is_none() {
      return Err(io::Error::other("unsupported node"));
    }
    let mut slot = self.0.write().unwrap_or_else(|e| e.into_inner());
    slot.1.cancel();
    *slot = (Outbound::Vmess(node), CancellationToken::new());
    Ok(())
  }
}

impl Outbound {
  pub(crate) fn diagnostic_label(&self) -> String {
    match self {
      Self::Cloud { server } => format!("cloud upstream={}:{}", server.host, server.port),
      Self::Vmess(node) => format!(
        "vmess upstream={}:{} transport={} tls={}",
        node.address,
        node.port,
        node
          .vmess
          .as_ref()
          .map(|v| v.network.as_str())
          .unwrap_or("unknown"),
        node.vmess.as_ref().is_some_and(|v| v.tls)
      ),
    }
  }
  pub async fn connect<F: Fn(&str, &str) + Send + Sync + 'static>(
    &self,
    host: &str,
    port: u16,
    cancel: CancellationToken,
    tracker: TaskTracker,
    log: Arc<F>,
  ) -> io::Result<AnyStream> {
    match self {
      Self::Cloud { server } => {
        let (application, transport) = tokio::io::duplex(65536);
        let server = server.clone();
        let host = host.to_owned();
        tracker.spawn(async move {
          tokio::select! {
            biased;
            _ = cancel.cancelled() => {},
            result = cloudtun_common::proxy::proxy_to_cloudtun_server(transport, server, host.clone(), port, log.clone()) => {
              if let Err(e) = result { log("proxy::error", &format!("stage=cloud.transport target={host}:{port} Cloud tunnel closed: {e:?}")); }
            }
          }
        });
        Ok(Box::new(application))
      }
      Self::Vmess(node) => connect_vmess(node, host, port).await,
    }
  }
}

pub async fn connect_vmess(
  node: &SubscriptionNode,
  host: &str,
  port: u16,
) -> io::Result<AnyStream> {
  if node.unsupported_reason.is_some() {
    return Err(io::Error::other("unsupported node"));
  }
  let config = node
    .vmess
    .as_ref()
    .ok_or_else(|| io::Error::other("missing VMess configuration"))?;
  let stream = vmess_transport(node).await?;
  let mut session = Session::default();
  session.destination = SocksAddr::try_from((host, port))?;
  let vmess = leaf::proxy::vmess::outbound::StreamHandler {
    address: node.address.clone(),
    port: node.port,
    uuid: config.uuid.clone(),
    security: config.cipher.clone(),
  };
  let stream = vmess.handle(&session, None, Some(stream)).await?;
  Ok(Box::new(VmessStream(stream)))
}

async fn vmess_transport(node: &SubscriptionNode) -> io::Result<leaf::proxy::AnyStream> {
  let config = node
    .vmess
    .as_ref()
    .ok_or_else(|| io::Error::other("missing VMess configuration"))?;
  let tcp = crate::tcp::connect(&node.address, node.port).await?;
  tcp.set_nodelay(true)?;
  let mut stream: leaf::proxy::AnyStream = Box::new(tcp);
  if config.tls {
    let connector = tls_connector()?;
    let name = tokio_rustls::rustls::pki_types::ServerName::try_from(config.server_name.clone())
      .map_err(io::Error::other)?;
    stream = Box::new(connector.connect(name, stream).await?);
  }
  if config.network == "ws" {
    let mut transport = Session::default();
    transport.destination = SocksAddr::try_from((node.address.as_str(), node.port))?;
    let mut headers = HashMap::new();
    headers.insert(
      "Host".into(),
      if config.host.is_empty() {
        node.address.clone()
      } else {
        config.host.clone()
      },
    );
    let ws = leaf::proxy::ws::outbound::StreamHandler {
      path: config.path.clone(),
      headers,
    };
    stream = ws.handle(&transport, None, Some(stream)).await?;
  }
  Ok(stream)
}

pub(crate) async fn vmess_datagram(
  node: &SubscriptionNode,
  host: &str,
  port: u16,
) -> io::Result<leaf::proxy::AnyOutboundDatagram> {
  use leaf::proxy::{OutboundDatagramHandler, OutboundTransport};
  let config = node
    .vmess
    .as_ref()
    .ok_or_else(|| io::Error::other("missing VMess configuration"))?;
  let stream = vmess_transport(node).await?;
  let mut session = Session::default();
  session.destination = SocksAddr::try_from((host, port))?;
  leaf::proxy::vmess::outbound::DatagramHandler {
    address: node.address.clone(),
    port: node.port,
    uuid: config.uuid.clone(),
    security: config.cipher.clone(),
  }
  .handle(&session, Some(OutboundTransport::Stream(stream)))
  .await
}

/// Match v2rayN real ping: two full GETs, 100 ms gaps, one shared five-second budget.
pub async fn real_ping(node: &SubscriptionNode) -> anyhow::Result<u64> {
  let proxy =
    crate::PreparedProxy::bind_node_at(node.clone(), crate::RoutingMode::Global, "127.0.0.1", 0)
      .await?;
  let address = proxy.local_addr()?;
  let cancel = CancellationToken::new();
  struct StopProbe(CancellationToken);
  impl Drop for StopProbe {
    fn drop(&mut self) {
      self.0.cancel();
    }
  }
  let _stop = StopProbe(cancel.clone());
  tokio::spawn(async move {
    let _ = proxy.run(cancel, |_, _| {}).await;
  });
  let client = http_client_builder()?
    .no_proxy()
    .proxy(reqwest::Proxy::all(format!("http://{address}"))?)
    .connect_timeout(std::time::Duration::from_secs(5))
    .build()?;
  measure_http_latency(
    &client,
    "https://www.google.com/generate_204",
    std::time::Duration::from_secs(5),
  )
  .await
}

async fn measure_http_latency(
  client: &reqwest::Client,
  url: &str,
  budget: std::time::Duration,
) -> anyhow::Result<u64> {
  tokio::time::timeout(budget, async {
    let mut samples = Vec::with_capacity(2);
    for _ in 0..2 {
      let started = std::time::Instant::now();
      // Like HttpClient.GetAsync, consume the response body without requiring HTTP 204.
      client.get(url).send().await?.bytes().await?;
      let elapsed = started.elapsed().as_millis() as u64;
      if elapsed > 0 {
        samples.push(elapsed);
      }
      tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    Ok::<_, anyhow::Error>(samples.into_iter().min().unwrap_or(0))
  })
  .await
  .map_err(|_| anyhow::anyhow!("真连接延迟测试超时（两次请求共用 5 秒）"))?
}

pub async fn check_node(node: &SubscriptionNode) -> anyhow::Result<()> {
  anyhow::ensure!(real_ping(node).await? > 0, "真连接延迟测试失败");
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  async fn probe_server(fail_second: bool, delay_ms: u64) -> (String, tokio::task::JoinHandle<()>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
      for index in 0..2 {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut header = Vec::new();
        while !header.ends_with(b"\r\n\r\n") {
          header.push(socket.read_u8().await.unwrap());
        }
        socket
          .write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 4\r\nConnection: close\r\n\r\n")
          .await
          .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
        if index == 1 && fail_second {
          socket.write_all(b"x").await.unwrap();
        } else {
          socket.write_all(b"body").await.unwrap();
        }
      }
    });
    (format!("http://{address}"), task)
  }
  #[tokio::test]
  async fn real_ping_reads_both_bodies_and_accepts_non_204_status() {
    let (url, task) = probe_server(false, 20).await;
    let result = measure_http_latency(
      &reqwest::Client::builder().no_proxy().build().unwrap(),
      &url,
      std::time::Duration::from_secs(2),
    )
    .await;
    task.await.unwrap();
    assert!(result.unwrap() >= 15);
  }
  #[tokio::test]
  async fn second_response_failure_discards_first_success() {
    let (url, task) = probe_server(true, 20).await;
    let result = measure_http_latency(
      &reqwest::Client::builder().no_proxy().build().unwrap(),
      &url,
      std::time::Duration::from_secs(2),
    )
    .await;
    task.await.unwrap();
    assert!(result.is_err());
  }
  #[tokio::test]
  async fn requests_share_one_overall_timeout() {
    let (url, task) = probe_server(false, 20).await;
    let result = measure_http_latency(
      &reqwest::Client::builder().no_proxy().build().unwrap(),
      &url,
      std::time::Duration::from_millis(80),
    )
    .await;
    task.abort();
    assert!(result.unwrap_err().to_string().contains("超时"));
  }

  #[tokio::test]
  async fn real_ping_accepts_latency_above_the_old_800_ms_limit() {
    let (url, task) = probe_server(false, 900).await;
    let result = measure_http_latency(
      &reqwest::Client::builder().no_proxy().build().unwrap(),
      &url,
      std::time::Duration::from_secs(5),
    )
    .await;
    task.await.unwrap();
    assert!(result.unwrap() >= 800);
  }
  #[tokio::test]
  async fn tcp_reachable_without_vmess_forwarding_is_not_a_successful_ping() {
    use base64::Engine;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let fixture = serde_json::json!({"v":"2","ps":"tcp-only","add":"127.0.0.1","port":address.port(),"id":"00000000-0000-4000-8000-000000000001","net":"tcp","tls":""});
    let link = format!(
      "vmess://{}",
      base64::engine::general_purpose::STANDARD.encode(fixture.to_string())
    );
    let node = crate::subscription::parse_subscription(&link)
      .unwrap()
      .remove(0);
    let task = tokio::spawn(async move {
      let (socket, _) = listener.accept().await.unwrap();
      drop(socket);
    });
    let result = crate::subscription::ping_node(&node).await;
    task.await.unwrap();
    assert!(result.latency_ms.is_none());
  }

  #[test]
  fn websocket_disconnect_is_not_retried_as_interrupted_io() {
    let error = io::Error::new(io::ErrorKind::Interrupted, "broken pipe");
    assert_eq!(
      vmess_io_error(error).kind(),
      io::ErrorKind::ConnectionAborted
    );
    assert_eq!(
      vmess_io_error(io::ErrorKind::TimedOut.into()).kind(),
      io::ErrorKind::TimedOut
    );
  }
}
