use crate::{proxy::CONNECT_TIMEOUT, tls::CloudEndpoint};
use tokio::{
  io::{AsyncReadExt, AsyncWriteExt},
  time::timeout,
};
pub async fn ping_cloudtun_proxy_server(server: &CloudEndpoint) -> std::io::Result<bool> {
  timeout(CONNECT_TIMEOUT, async {
    let mut stream = server.connect().await?;
    let request = format!(
      "GET /ping HTTP/1.1\r\nHost: {}:{}\r\nx-token: {}\r\nConnection: close\r\n\r\n",
      server.host, server.port, server.token
    );
    stream.write_all(request.as_bytes()).await?;
    let mut response = Vec::new();
    stream.take(16385).read_to_end(&mut response).await?;
    if response.len() > 16384 {
      return Err(std::io::Error::other("探测响应过大"));
    }
    let text = std::str::from_utf8(&response).map_err(std::io::Error::other)?;
    Ok(
      (text.starts_with("HTTP/1.1 200 ") || text.starts_with("HTTP/1.0 200 "))
        && text.ends_with("pong!"),
    )
  })
  .await
  .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "WSS 探测超时"))?
}
