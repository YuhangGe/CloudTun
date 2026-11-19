use std::{io, net::SocketAddr, time::Duration};
use tokio::{net::TcpStream, task::JoinSet};

// Race resolved addresses: a stale or unreachable first DNS answer must not hide
// a healthy second address until the entire connect timeout expires.
pub(crate) async fn connect_addresses(addresses: &[SocketAddr]) -> io::Result<TcpStream> {
  let mut attempts = JoinSet::new();
  for (index, address) in addresses.iter().copied().take(16).enumerate() {
    attempts.spawn(async move {
      if index > 0 {
        tokio::time::sleep(Duration::from_millis(index as u64 * 40)).await;
      }
      TcpStream::connect(address).await
    });
  }
  let mut last = io::Error::other("DNS returned no addresses");
  while let Some(result) = attempts.join_next().await {
    match result {
      Ok(Ok(stream)) => return Ok(stream),
      Ok(Err(error)) => last = error,
      Err(error) => last = io::Error::other(error),
    }
  }
  Err(last)
}
pub(crate) async fn connect(host: &str, port: u16) -> io::Result<TcpStream> {
  let addresses: Vec<_> = tokio::time::timeout(
    Duration::from_secs(3),
    tokio::net::lookup_host((host, port)),
  )
  .await??
  .collect();
  connect_addresses(&addresses).await
}
#[cfg(test)]
mod tests {
  use super::*;
  #[tokio::test]
  async fn tries_other_dns_addresses_when_first_fails() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let closed = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let closed_address = closed.local_addr().unwrap();
    drop(closed);
    let stream = tokio::time::timeout(
      Duration::from_millis(500),
      connect_addresses(&[closed_address, address]),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(stream.peer_addr().unwrap(), address);
  }
}
