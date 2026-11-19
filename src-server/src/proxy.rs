use crate::context::Context;
use axum::{
  extract::{
    State,
    ws::{Message, WebSocket, WebSocketUpgrade},
  },
  http::{HeaderMap, HeaderValue, StatusCode},
  response::{IntoResponse, Response},
};
use cloudtun_common::constant::{X_CONNECT_HOST_KEY, X_CONNECT_PORT_KEY, X_TOKEN_KEY};
use futures_util::{SinkExt, StreamExt};
use std::{sync::Arc, time::Duration};
use tokio::{
  io::{AsyncReadExt, AsyncWriteExt},
  net::{TcpStream, UdpSocket, lookup_host},
  time::{Instant, interval, timeout},
};
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const IO_TIMEOUT: Duration = Duration::from_secs(60);
const EOF: &str = "cloudtun:eof";

pub async fn proxy_handler(
  ws: WebSocketUpgrade,
  headers: HeaderMap,
  State(context): State<Arc<Context>>,
) -> Result<Response, StatusCode> {
  upgrade(ws, headers, context, false)
}
pub async fn udp_proxy_handler(
  ws: WebSocketUpgrade,
  headers: HeaderMap,
  State(context): State<Arc<Context>>,
) -> Result<Response, StatusCode> {
  upgrade(ws, headers, context, true)
}
fn upgrade(
  ws: WebSocketUpgrade,
  headers: HeaderMap,
  context: Arc<Context>,
  udp: bool,
) -> Result<Response, StatusCode> {
  if !headers
    .get(X_TOKEN_KEY)
    .is_some_and(|v| v == context.token.as_str())
  {
    return Err(StatusCode::UNAUTHORIZED);
  }
  let host = headers
    .get(X_CONNECT_HOST_KEY)
    .and_then(|v| v.to_str().ok())
    .filter(|s| !s.is_empty())
    .ok_or(StatusCode::BAD_REQUEST)?
    .to_owned();
  let port = headers
    .get(X_CONNECT_PORT_KEY)
    .and_then(|v| v.to_str().ok())
    .and_then(|s| s.parse::<u16>().ok())
    .filter(|p| *p != 0)
    .ok_or(StatusCode::BAD_REQUEST)?;
  if !headers.get("x-tunnel-version").is_some_and(|v| v == "3") {
    return Err(StatusCode::BAD_REQUEST);
  }
  let half_close = true;
  let mut response = ws
    .max_message_size(1024 * 1024)
    .on_upgrade(move |socket| async move {
      let result = if udp {
        handle_udp(socket, host, port, &context).await
      } else {
        handle_tcp(socket, host, port, &context, half_close).await
      };
      if let Err(e) = result {
        eprintln!("proxy tunnel closed: {e}");
      }
    })
    .into_response();
  response
    .headers_mut()
    .insert("x-tunnel-version", HeaderValue::from_static("3"));
  Ok(response)
}
async fn handle_tcp(
  socket: WebSocket,
  host: String,
  port: u16,
  ctx: &Context,
  half_close: bool,
) -> anyhow::Result<()> {
  let remote = timeout(CONNECT_TIMEOUT, TcpStream::connect((host.as_str(), port))).await??;
  remote.set_nodelay(true)?;
  let (mut reader, mut writer) = remote.into_split();
  let (mut sink, mut source) = socket.split();
  let mut buf = [0u8; 16384];
  let mut heartbeat = interval(Duration::from_secs(20));
  let mut last_received = Instant::now();
  let mut write_open = true;
  loop {
    tokio::select! {
      msg = source.next() => {
        last_received = Instant::now();
        match msg {
          Some(Ok(Message::Binary(data))) if write_open => {
            timeout(IO_TIMEOUT, writer.write_all(&data)).await??;
            ctx.touch_ping_ts().await;
          }
          Some(Ok(Message::Text(text))) if half_close && text == EOF && write_open => {
            timeout(CONNECT_TIMEOUT, writer.shutdown()).await??;
            write_open = false;
          }
          Some(Ok(Message::Ping(data))) => {
            timeout(IO_TIMEOUT, sink.send(Message::Pong(data))).await??;
          }
          Some(Ok(Message::Close(_))) | None => return Ok(()),
          Some(Err(e)) => return Err(e.into()),
          _ => {}
        }
      }
      n = reader.read(&mut buf) => {
        let n = n?;
        if n == 0 {
          // Complete the WebSocket close handshake before dropping TCP. Dropping
          // with unread client frames can reset the connection and lose the response.
          let _ = timeout(Duration::from_secs(3), async {
            sink.send(Message::Close(None)).await?;
            while let Some(message) = source.next().await {
              if matches!(message?, Message::Close(_)) { break; }
            }
            Ok::<(), axum::Error>(())
          }).await;
          return Ok(());
        }
        timeout(IO_TIMEOUT, sink.send(Message::Binary(buf[..n].to_vec().into()))).await??;
        ctx.touch_ping_ts().await;
      }
      _ = heartbeat.tick() => {
        anyhow::ensure!(last_received.elapsed() < IO_TIMEOUT, "client heartbeat timed out");
        timeout(IO_TIMEOUT, sink.send(Message::Ping(Vec::new().into()))).await??;
      }
    }
  }
}
async fn handle_udp(
  socket: WebSocket,
  host: String,
  port: u16,
  ctx: &Context,
) -> anyhow::Result<()> {
  let addr = timeout(CONNECT_TIMEOUT, lookup_host((host.as_str(), port)))
    .await??
    .next()
    .ok_or_else(|| anyhow::anyhow!("no UDP target address"))?;
  let udp = UdpSocket::bind(if addr.is_ipv4() {
    "0.0.0.0:0"
  } else {
    "[::]:0"
  })
  .await?;
  udp.connect(addr).await?;
  let (mut sink, mut source) = socket.split();
  let mut buf = vec![0; 65535];
  let mut last_received = Instant::now();
  let mut last_datagram = Instant::now();
  let mut heartbeat = interval(Duration::from_secs(20));
  loop {
    tokio::select! {
      msg = source.next() => {
        last_received = Instant::now();
        match msg {
          Some(Ok(Message::Binary(data))) => {
            last_datagram = Instant::now();
            anyhow::ensure!(data.len() <= 65507, "UDP datagram too large");
            let n = timeout(IO_TIMEOUT, udp.send(&data)).await??;
            anyhow::ensure!(n == data.len(), "partial UDP send");
            ctx.touch_ping_ts().await;
          }
          Some(Ok(Message::Ping(data))) => {
            timeout(IO_TIMEOUT, sink.send(Message::Pong(data))).await??;
          }
          Some(Ok(Message::Close(_))) | None => return Ok(()),
          Some(Err(e)) => return Err(e.into()),
          _ => {}
        }
      }
      n = udp.recv(&mut buf) => {
        let n = n?;
        last_datagram = Instant::now();
        timeout(IO_TIMEOUT, sink.send(Message::Binary(buf[..n].to_vec().into()))).await??;
        ctx.touch_ping_ts().await;
      }
      _ = heartbeat.tick() => {
        if last_datagram.elapsed() >= IO_TIMEOUT { return Ok(()); }
        anyhow::ensure!(last_received.elapsed() < IO_TIMEOUT, "client heartbeat timed out");
        timeout(IO_TIMEOUT, sink.send(Message::Ping(Vec::new().into()))).await??;
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::{
    pin::Pin,
    task::{Context as TaskContext, Poll},
  };
  use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::TcpListener,
  };

  async fn start_server() -> (
    cloudtun_common::tls::CloudEndpoint,
    tokio::task::JoinHandle<()>,
    tempfile::TempDir,
  ) {
    let directory = tempfile::tempdir().unwrap();
    let identity = crate::tls::Identity::load_or_create(directory.path(), None).unwrap();
    let context = Arc::new(Context::new(identity.token.clone(), "unused".into(), None));
    let app = crate::router(context);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let endpoint = cloudtun_common::tls::CloudEndpoint::new(
      "127.0.0.1".into(),
      port,
      "test-instance".into(),
      identity.token.clone(),
      directory.path().join("trust"),
    )
    .unwrap();
    let config = identity.config().unwrap();
    let task = tokio::spawn(async move {
      crate::serve(
        listener,
        app,
        config,
        tokio_util::sync::CancellationToken::new(),
      )
      .await
      .unwrap()
    });
    (endpoint, task, directory)
  }
  #[tokio::test]
  async fn tcp_half_close_preserves_delayed_response() {
    let (server, serve, _directory) = start_server().await;
    let target = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target_addr = target.local_addr().unwrap();
    let peer = tokio::spawn(async move {
      let (mut tcp, _) = target.accept().await.unwrap();
      let mut data = Vec::new();
      tcp.read_to_end(&mut data).await.unwrap();
      assert_eq!(data, b"request with a nonaligned tail!");
      tcp.write_all(b"complete response after EOF").await.unwrap();
    });
    let (local, mut app) = tokio::io::duplex(4096);
    let tunnel = tokio::spawn(cloudtun_common::proxy::proxy_to_cloudtun_server(
      local,
      Arc::new(server),
      "127.0.0.1".into(),
      target_addr.port(),
      Arc::new(|_: &str, _: &str| {}),
    ));
    app
      .write_all(b"request with a nonaligned tail!")
      .await
      .unwrap();
    app.shutdown().await.unwrap();
    let mut response = Vec::new();
    timeout(Duration::from_secs(3), app.read_to_end(&mut response))
      .await
      .unwrap()
      .unwrap();
    assert_eq!(response, b"complete response after EOF");
    timeout(Duration::from_secs(3), tunnel)
      .await
      .unwrap()
      .unwrap()
      .unwrap();
    peer.await.unwrap();
    serve.abort();
  }

  struct Datagrams {
    incoming: tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>,
    outgoing: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
  }
  impl AsyncRead for Datagrams {
    fn poll_read(
      mut self: Pin<&mut Self>,
      cx: &mut TaskContext<'_>,
      buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
      match self.incoming.poll_recv(cx) {
        Poll::Ready(Some(data)) => {
          buf.put_slice(&data);
          Poll::Ready(Ok(()))
        }
        Poll::Ready(None) => Poll::Ready(Err(std::io::ErrorKind::BrokenPipe.into())),
        Poll::Pending => Poll::Pending,
      }
    }
  }
  impl AsyncWrite for Datagrams {
    fn poll_write(
      self: Pin<&mut Self>,
      _: &mut TaskContext<'_>,
      buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
      Poll::Ready(
        self
          .outgoing
          .send(buf.to_vec())
          .map(|_| buf.len())
          .map_err(|_| std::io::ErrorKind::BrokenPipe.into()),
      )
    }
    fn poll_flush(self: Pin<&mut Self>, _: &mut TaskContext<'_>) -> Poll<std::io::Result<()>> {
      Poll::Ready(Ok(()))
    }
    fn poll_shutdown(self: Pin<&mut Self>, _: &mut TaskContext<'_>) -> Poll<std::io::Result<()>> {
      Poll::Ready(Ok(()))
    }
  }
  #[tokio::test]
  async fn udp_preserves_datagrams_and_drops_oversize_responses() {
    let (server, serve, _directory) = start_server().await;
    let target = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let target_addr = target.local_addr().unwrap();
    let peer = tokio::spawn(async move {
      let mut buf = [0; 65535];
      for _ in 0..4 {
        let (n, addr) = target.recv_from(&mut buf).await.unwrap();
        target.send_to(&buf[..n], addr).await.unwrap();
      }
    });
    let (send, incoming) = tokio::sync::mpsc::unbounded_channel();
    let (outgoing, mut recv) = tokio::sync::mpsc::unbounded_channel();
    let local = Datagrams { incoming, outgoing };
    let tunnel = tokio::spawn(cloudtun_common::proxy::proxy_udp_to_cloudtun_server(
      local,
      Arc::new(server),
      "127.0.0.1".into(),
      target_addr.port(),
      Arc::new(|_: &str, _: &str| {}),
      1472,
    ));
    for data in [vec![], vec![9; 1400]] {
      send.send(data.clone()).unwrap();
      assert_eq!(
        timeout(Duration::from_secs(3), recv.recv())
          .await
          .unwrap()
          .unwrap(),
        data
      );
    }
    send.send(vec![8; 2000]).unwrap();
    send.send(b"after oversize".to_vec()).unwrap();
    assert_eq!(
      timeout(Duration::from_secs(3), recv.recv())
        .await
        .unwrap()
        .unwrap(),
      b"after oversize"
    );
    assert!(recv.try_recv().is_err());
    peer.await.unwrap();
    tunnel.abort();
    serve.abort();
  }
  #[tokio::test]
  async fn tcp_transfers_large_payloads_with_backpressure() {
    let (server, serve, _directory) = start_server().await;
    let target = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target_addr = target.local_addr().unwrap();
    let payload: Vec<u8> = (0..1024 * 1024 + 13).map(|n| (n % 251) as u8).collect();
    let expected = payload.clone();
    let peer = tokio::spawn(async move {
      let (mut tcp, _) = target.accept().await.unwrap();
      let mut received = Vec::new();
      tcp.read_to_end(&mut received).await.unwrap();
      assert_eq!(received, expected);
      tcp.write_all(&received).await.unwrap();
    });
    let (local, application) = tokio::io::duplex(1024);
    let tunnel = tokio::spawn(cloudtun_common::proxy::proxy_to_cloudtun_server(
      local,
      Arc::new(server),
      "127.0.0.1".into(),
      target_addr.port(),
      Arc::new(|_: &str, _: &str| {}),
    ));
    let (mut read, mut write) = tokio::io::split(application);
    let upload = async {
      write.write_all(&payload).await.unwrap();
      write.shutdown().await.unwrap();
    };
    let download = async {
      let mut data = Vec::new();
      read.read_to_end(&mut data).await.unwrap();
      data
    };
    let (_, response) = timeout(Duration::from_secs(5), async {
      tokio::join!(upload, download)
    })
    .await
    .unwrap();
    assert_eq!(response, payload);
    timeout(Duration::from_secs(3), tunnel)
      .await
      .unwrap()
      .unwrap()
      .unwrap();
    peer.await.unwrap();
    serve.abort();
  }
}
