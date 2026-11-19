use crate::{
  constant::{X_CONNECT_HOST_KEY, X_CONNECT_PORT_KEY, X_TOKEN_KEY},
  encode::xor_inplace_simd,
};
use futures_util::{SinkExt, StreamExt};
use http::HeaderValue;
use std::{
  io::{Error, ErrorKind},
  sync::Arc,
  time::Duration,
};
use tokio::{
  io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
  time::{Instant, interval, timeout},
};
use tokio_tungstenite::{
  connect_async_with_config,
  tungstenite::{
    client::IntoClientRequest,
    protocol::{Message, WebSocketConfig},
  },
};

pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
pub const IO_TIMEOUT: Duration = Duration::from_secs(60);
pub const TUNNEL_EOF: &str = "cloudtun:eof";

pub async fn proxy_to_cloudtun_server<S, F>(
  local_stream: S,
  server: Arc<(String, u16, String)>,
  target_host: String,
  target_port: u16,
  secret: Arc<Vec<u8>>,
  log_fn: Arc<F>,
) -> std::io::Result<()>
where
  S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
  F: Fn(&str, &str) + Send + Sync + 'static,
{
  tunnel(
    local_stream,
    server,
    target_host,
    target_port,
    secret,
    log_fn,
    None,
  )
  .await
}

/// Each UDP read and binary WebSocket message represents exactly one datagram.
pub async fn proxy_udp_to_cloudtun_server<S, F>(
  local_stream: S,
  server: Arc<(String, u16, String)>,
  target_host: String,
  target_port: u16,
  secret: Arc<Vec<u8>>,
  log_fn: Arc<F>,
  max_response: usize,
) -> std::io::Result<()>
where
  S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
  F: Fn(&str, &str) + Send + Sync + 'static,
{
  tunnel(
    local_stream,
    server,
    target_host,
    target_port,
    secret,
    log_fn,
    Some(max_response),
  )
  .await
}

async fn tunnel<S, F>(
  local_stream: S,
  server: Arc<(String, u16, String)>,
  target_host: String,
  target_port: u16,
  secret: Arc<Vec<u8>>,
  log_fn: Arc<F>,
  max_datagram: Option<usize>,
) -> std::io::Result<()>
where
  S: AsyncRead + AsyncWrite + Unpin,
  F: Fn(&str, &str),
{
  if secret.len() != 16 {
    return Err(Error::new(
      ErrorKind::InvalidInput,
      "proxy key must contain 16 bytes",
    ));
  }
  let host = if server.0.contains(':') && !server.0.starts_with('[') {
    format!("[{}]", server.0)
  } else {
    server.0.clone()
  };
  let path = if max_datagram.is_some() { "udp" } else { "ws" };
  let mut request = format!("ws://{host}:{}/{path}", server.1)
    .into_client_request()
    .map_err(Error::other)?;
  let headers = request.headers_mut();
  headers.insert(
    X_TOKEN_KEY,
    HeaderValue::from_str(&server.2).map_err(Error::other)?,
  );
  headers.insert(
    X_CONNECT_HOST_KEY,
    HeaderValue::from_str(&target_host).map_err(Error::other)?,
  );
  headers.insert(
    X_CONNECT_PORT_KEY,
    HeaderValue::from_str(&target_port.to_string()).map_err(Error::other)?,
  );
  headers.insert("x-tunnel-version", HeaderValue::from_static("2"));
  let (ws, response) = timeout(
    CONNECT_TIMEOUT,
    connect_async_with_config(
      request,
      Some(WebSocketConfig::default().max_message_size(Some(1024 * 1024))),
      true,
    ),
  )
  .await?
  .map_err(Error::other)?;
  let half_close = response
    .headers()
    .get("x-tunnel-version")
    .is_some_and(|v| v == "2");
  let (mut sink, mut source) = ws.split();
  let (mut reader, mut writer) = tokio::io::split(local_stream);
  log_fn(
    "proxy::info",
    &format!("Proxy {path} ==> {target_host}:{target_port}"),
  );
  let mut buf = vec![0; if max_datagram.is_some() { 65535 } else { 16384 }];
  let mut reading = true;
  let mut heartbeat = interval(Duration::from_secs(20));
  let mut last_received = Instant::now();
  let mut last_datagram = Instant::now();
  loop {
    tokio::select! {
      data = reader.read(&mut buf), if reading => {
        let n = data?;
        last_datagram = Instant::now();
        if n == 0 && max_datagram.is_none() {
          if half_close {
            timeout(IO_TIMEOUT, sink.send(Message::Text(TUNNEL_EOF.into())))
              .await?.map_err(Error::other)?;
            reading = false;
          } else {
            let _ = timeout(CONNECT_TIMEOUT, sink.close()).await;
            return Ok(());
          }
        } else {
          xor_inplace_simd(&mut buf[..n], &secret);
          timeout(IO_TIMEOUT, sink.send(Message::Binary(buf[..n].to_vec().into())))
            .await?.map_err(Error::other)?;
        }
      }
      msg = source.next() => {
        last_received = Instant::now();
        match msg {
          Some(Ok(Message::Binary(data))) => {
            last_datagram = Instant::now();
            if let Some(limit) = max_datagram {
              // IpStackUdpStream truncates at MTU. Never split a datagram with write_all.
              if data.len() > limit {
                log_fn("proxy::error", "dropping UDP response larger than TUN MTU");
                continue;
              }
              let n = timeout(IO_TIMEOUT, writer.write(&data)).await??;
              if n != data.len() {
                return Err(Error::new(ErrorKind::WriteZero, "partial UDP datagram"));
              }
            } else {
              timeout(IO_TIMEOUT, writer.write_all(&data)).await??;
            }
          }
          Some(Ok(Message::Ping(data))) => {
            timeout(IO_TIMEOUT, sink.send(Message::Pong(data))).await?.map_err(Error::other)?;
          }
          Some(Ok(Message::Close(_))) => {
            // Reading Close queues tungstenite's reply; flush it before dropping TCP.
            let _ = timeout(CONNECT_TIMEOUT, sink.flush()).await;
            timeout(CONNECT_TIMEOUT, writer.shutdown()).await??;
            return Ok(());
          }
          None => {
            timeout(CONNECT_TIMEOUT, writer.shutdown()).await??;
            return Ok(());
          }
          Some(Err(e)) => return Err(Error::other(e)),
          _ => {}
        }
      }
      _ = heartbeat.tick() => {
        if max_datagram.is_some() && last_datagram.elapsed() >= IO_TIMEOUT {
          return Ok(());
        }
        if last_received.elapsed() > IO_TIMEOUT {
          return Err(Error::new(ErrorKind::TimedOut, "proxy tunnel heartbeat timed out"));
        }
        timeout(IO_TIMEOUT, sink.send(Message::Ping(Vec::new().into())))
          .await?.map_err(Error::other)?;
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use tokio::net::TcpListener;
  use tokio_tungstenite::accept_async;
  #[tokio::test]
  async fn remote_close_delivers_eof_and_completes_without_local_input() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let peer = tokio::spawn(async move {
      let (tcp, _) = listener.accept().await.unwrap();
      let mut ws = accept_async(tcp).await.unwrap();
      ws.send(Message::Binary(b"response".to_vec().into()))
        .await
        .unwrap();
      ws.send(Message::Close(None)).await.unwrap();
    });
    let (stream, mut application) = tokio::io::duplex(4096);
    let tunnel = tokio::spawn(proxy_to_cloudtun_server(
      stream,
      Arc::new(("127.0.0.1".into(), addr.port(), "token".into())),
      "example.com".into(),
      443,
      Arc::new(vec![0; 16]),
      Arc::new(|_: &str, _: &str| {}),
    ));
    let mut data = Vec::new();
    timeout(Duration::from_secs(2), application.read_to_end(&mut data))
      .await
      .unwrap()
      .unwrap();
    assert_eq!(data, b"response");
    timeout(Duration::from_secs(2), tunnel)
      .await
      .unwrap()
      .unwrap()
      .unwrap();
    peer.await.unwrap();
  }
  #[tokio::test]
  async fn invalid_key_returns_error_instead_of_panicking() {
    let (stream, _) = tokio::io::duplex(16);
    let error = proxy_to_cloudtun_server(
      stream,
      Arc::new(("127.0.0.1".into(), 1, "token".into())),
      "example.com".into(),
      443,
      Arc::new(vec![]),
      Arc::new(|_: &str, _: &str| {}),
    )
    .await
    .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidInput);
  }
  #[tokio::test]
  async fn unresponsive_peer_times_out_even_when_local_application_is_idle() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let peer = tokio::spawn(async move {
      let (tcp, _) = listener.accept().await.unwrap();
      let _ws = accept_async(tcp).await.unwrap();
      std::future::pending::<()>().await;
    });
    let started = Arc::new(tokio::sync::Notify::new());
    let signal = started.clone();
    let (stream, _application) = tokio::io::duplex(4096);
    let tunnel = tokio::spawn(proxy_to_cloudtun_server(
      stream,
      Arc::new(("127.0.0.1".into(), addr.port(), "token".into())),
      "example.com".into(),
      443,
      Arc::new(vec![0; 16]),
      Arc::new(move |_: &str, _: &str| signal.notify_one()),
    ));
    started.notified().await;
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(81)).await;
    let error = tunnel.await.unwrap().unwrap_err();
    assert_eq!(error.kind(), ErrorKind::TimedOut);
    peer.abort();
  }
}
