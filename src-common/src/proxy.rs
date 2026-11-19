use crate::{
  constant::{X_CONNECT_HOST_KEY, X_CONNECT_PORT_KEY, X_TOKEN_KEY},
  tls::CloudEndpoint,
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
  client_async_with_config,
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
  server: Arc<CloudEndpoint>,
  target_host: String,
  target_port: u16,
  log_fn: Arc<F>,
) -> std::io::Result<()>
where
  S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
  F: Fn(&str, &str) + Send + Sync + 'static,
{
  tunnel(local_stream, server, target_host, target_port, log_fn, None).await
}

/// Each UDP read and binary WebSocket message represents exactly one datagram.
pub async fn proxy_udp_to_cloudtun_server<S, F>(
  local_stream: S,
  server: Arc<CloudEndpoint>,
  target_host: String,
  target_port: u16,
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
    log_fn,
    Some(max_response),
  )
  .await
}

async fn tunnel<S, F>(
  local_stream: S,
  server: Arc<CloudEndpoint>,
  target_host: String,
  target_port: u16,
  log_fn: Arc<F>,
  max_datagram: Option<usize>,
) -> std::io::Result<()>
where
  S: AsyncRead + AsyncWrite + Unpin,
  F: Fn(&str, &str),
{
  let host = if server.host.contains(':') {
    format!("[{}]", server.host)
  } else {
    server.host.clone()
  };
  let path = if max_datagram.is_some() { "udp" } else { "ws" };
  let mut request = format!("wss://{host}:{}/{path}", server.port)
    .into_client_request()
    .map_err(Error::other)?;
  let headers = request.headers_mut();
  headers.insert(
    X_TOKEN_KEY,
    HeaderValue::from_str(&server.token).map_err(Error::other)?,
  );
  headers.insert(
    X_CONNECT_HOST_KEY,
    HeaderValue::from_str(&target_host).map_err(Error::other)?,
  );
  headers.insert(
    X_CONNECT_PORT_KEY,
    HeaderValue::from_str(&target_port.to_string()).map_err(Error::other)?,
  );
  headers.insert("x-tunnel-version", HeaderValue::from_static("3"));
  let (ws, response) = timeout(
    CONNECT_TIMEOUT,
    client_async_with_config(
      request,
      server.connect().await?,
      Some(WebSocketConfig::default().max_message_size(Some(1024 * 1024))),
    ),
  )
  .await?
  .map_err(Error::other)?;
  if !response
    .headers()
    .get("x-tunnel-version")
    .is_some_and(|version| version == "3")
  {
    return Err(Error::new(
      ErrorKind::InvalidData,
      "服务端不是 WSS v3，请升级服务端",
    ));
  }
  let half_close = true;
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
