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

fn tls_connector() -> io::Result<tokio_rustls::TlsConnector> {
  let roots =
    tokio_rustls::rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
  let provider = Arc::new(tokio_rustls::rustls::crypto::ring::default_provider());
  let config = tokio_rustls::rustls::ClientConfig::builder_with_provider(provider)
    .with_safe_default_protocol_versions()
    .map_err(io::Error::other)?
    .with_root_certificates(roots)
    .with_no_client_auth();
  Ok(tokio_rustls::TlsConnector::from(Arc::new(config)))
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
impl Outbound {
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
            result = cloudtun_common::proxy::proxy_to_cloudtun_server(transport, server, host, port, log.clone()) => {
              if let Err(e) = result { log("proxy::error", &format!("Cloud tunnel closed: {e}")); }
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

/// Validate VMess authentication and a real HTTP response, beyond TCP ping.
pub async fn check_node(node: &SubscriptionNode) -> anyhow::Result<()> {
  use tokio::io::{AsyncReadExt, AsyncWriteExt};
  const HOST: &str = "www.google.com";
  tokio::time::timeout(std::time::Duration::from_secs(15), async {
    let tunnel = connect_vmess(node, HOST, 443).await?;
    let name = tokio_rustls::rustls::pki_types::ServerName::try_from(HOST)?;
    let mut stream = tls_connector()?.connect(name, tunnel).await?;
    stream
      .write_all(b"GET /generate_204 HTTP/1.1\r\nHost: www.google.com\r\nConnection: close\r\n\r\n")
      .await?;
    stream.flush().await?;
    let mut status = [0; 13];
    stream.read_exact(&mut status).await?;
    anyhow::ensure!(
      status == *b"HTTP/1.1 204 " || status == *b"HTTP/1.0 204 ",
      "节点探测未返回 HTTP 204"
    );
    Ok::<_, anyhow::Error>(())
  })
  .await
  .map_err(|_| anyhow::anyhow!("VMess 连通测试超时"))?
}

#[cfg(test)]
mod tests {
  use super::*;

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
