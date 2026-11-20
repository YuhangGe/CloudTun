use std::{
  io::{self, IoSlice},
  pin::Pin,
  sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
  },
  task::{Context, Poll},
  time::Instant,
};
use tokio::{
  io::{AsyncRead, AsyncWrite, ReadBuf},
  net::TcpStream,
};

static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
pub struct TrafficCounter(Arc<TrafficSession>);

struct TrafficSession {
  id: u64,
  started: Instant,
  bytes: AtomicU64,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrafficSnapshot {
  pub session_id: u64,
  pub elapsed_ms: u64,
  pub total_bytes: u64,
}

impl TrafficCounter {
  pub(crate) fn new() -> Self {
    Self(Arc::new(TrafficSession {
      id: NEXT_SESSION.fetch_add(1, Ordering::Relaxed),
      started: Instant::now(),
      bytes: AtomicU64::new(0),
    }))
  }

  pub fn snapshot(&self) -> TrafficSnapshot {
    TrafficSnapshot {
      session_id: self.0.id,
      elapsed_ms: self.0.started.elapsed().as_millis() as u64,
      total_bytes: self.0.bytes.load(Ordering::Relaxed),
    }
  }

  pub(crate) fn add(&self, bytes: usize) {
    if bytes != 0 {
      self.0.bytes.fetch_add(bytes as u64, Ordering::Relaxed);
    }
  }
}

// Count at the client socket so HTTP bodies and upgraded tunnels use the same counter.
pub(crate) struct MeteredStream<S = TcpStream>(pub S, pub TrafficCounter);

impl<S: AsyncRead + Unpin> AsyncRead for MeteredStream<S> {
  fn poll_read(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
    buf: &mut ReadBuf<'_>,
  ) -> Poll<io::Result<()>> {
    let before = buf.filled().len();
    let result = Pin::new(&mut self.0).poll_read(cx, buf);
    if let Poll::Ready(Ok(())) = result {
      self.1.add(buf.filled().len() - before);
    }
    result
  }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for MeteredStream<S> {
  fn poll_write(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
    buf: &[u8],
  ) -> Poll<io::Result<usize>> {
    let result = Pin::new(&mut self.0).poll_write(cx, buf);
    if let Poll::Ready(Ok(written)) = result {
      self.1.add(written);
    }
    result
  }

  fn is_write_vectored(&self) -> bool {
    self.0.is_write_vectored()
  }

  fn poll_write_vectored(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
    bufs: &[IoSlice<'_>],
  ) -> Poll<io::Result<usize>> {
    let result = Pin::new(&mut self.0).poll_write_vectored(cx, bufs);
    if let Poll::Ready(Ok(written)) = result {
      self.1.add(written);
    }
    result
  }

  fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
    Pin::new(&mut self.0).poll_flush(cx)
  }

  fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
    Pin::new(&mut self.0).poll_shutdown(cx)
  }
}
