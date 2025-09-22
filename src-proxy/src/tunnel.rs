use cloudtun_common::proxy::proxy_to_cloudtun_server;
use hyper::upgrade::Upgraded;
use hyper_util::rt::TokioIo;
use std::sync::Arc;

#[inline]
pub async fn proxy_tunnel<F: Fn(&str, &str) + Send + Sync + 'static>(
  upgraded: Upgraded,
  server: Arc<(String, u16, String)>,
  target_host: String,
  target_port: u16,
  secret: Arc<(Vec<u8>, String)>,
  log_fn: Arc<F>,
) -> std::io::Result<()> {
  let upgraded = TokioIo::new(upgraded);
  proxy_to_cloudtun_server(upgraded, server, target_host, target_port, secret, log_fn).await
}
