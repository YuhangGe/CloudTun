use crate::subscription::SubscriptionNode;
use leaf::{
  proxy::OutboundStreamHandler,
  session::{Session, SocksAddr},
};
use std::{collections::HashMap, io, sync::Arc};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

pub trait ProxyStream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> ProxyStream for T {}
pub type AnyStream = Box<dyn ProxyStream>;

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
    let roots = tokio_rustls::rustls::RootCertStore::from_iter(
      webpki_roots::TLS_SERVER_ROOTS.iter().cloned(),
    );
    let provider = Arc::new(tokio_rustls::rustls::crypto::ring::default_provider());
    let tls = tokio_rustls::rustls::ClientConfig::builder_with_provider(provider)
      .with_safe_default_protocol_versions()
      .map_err(io::Error::other)?
      .with_root_certificates(roots)
      .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(Arc::new(tls));
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
  Ok(Box::new(vmess.handle(&session, None, Some(stream)).await?))
}

/// Validate VMess authentication and a real HTTP response, beyond TCP ping.
pub async fn check_node(node: &SubscriptionNode) -> anyhow::Result<()> {
  use tokio::io::{AsyncReadExt, AsyncWriteExt};
  tokio::time::timeout(std::time::Duration::from_secs(15), async {
    let mut stream = connect_vmess(node, "example.com", 80).await?;
    stream
      .write_all(b"GET / HTTP/1.1\r\nHost: example.com\r\nConnection: close\r\n\r\n")
      .await?;
    stream.flush().await?;
    let mut prefix = [0; 12];
    stream.read_exact(&mut prefix).await?;
    anyhow::ensure!(prefix.starts_with(b"HTTP/1."), "节点未返回有效 HTTP 响应");
    Ok::<_, anyhow::Error>(())
  })
  .await
  .map_err(|_| anyhow::anyhow!("VMess 连通测试超时"))?
}
