use crate::{
  proxy::proxy_request,
  route::{MatchType, RouteMatcher},
};
use axum::{extract::Request, response::IntoResponse};
use hyper::{Method, StatusCode, body::Incoming, server::conn::http1};
use hyper_util::rt::TokioIo;
use std::sync::Arc;
use tokio::{net::TcpListener, task::JoinSet};
use tokio_util::sync::CancellationToken;

pub struct ProxyArgs {
  pub server_addr: (String, u16, String),
  pub password: Vec<u8>,
  pub local_addr: (String, u16),
  pub default_rule: MatchType,
  pub proxy_rules: Option<String>,
}
pub struct PreparedProxy {
  args: ProxyArgs,
  router: RouteMatcher,
  listener: TcpListener,
}
impl PreparedProxy {
  pub async fn bind(args: ProxyArgs) -> std::io::Result<Self> {
    if args.password.len() != 16 {
      return Err(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        "proxy key must contain 16 bytes",
      ));
    }
    let router = RouteMatcher::load(args.default_rule, args.proxy_rules.clone()).await?;
    let listener = TcpListener::bind(&args.local_addr).await?;
    Ok(Self {
      args,
      router,
      listener,
    })
  }
  pub async fn run<F: Fn(&str, &str) + Send + Sync + 'static>(
    self,
    shutdown_token: CancellationToken,
    log_fn: F,
  ) -> Result<(), Box<dyn std::error::Error>> {
    let Self {
      args,
      router,
      listener,
    } = self;
    let rule_count = router.get_count().await;
    let log_fn = Arc::new(log_fn);
    log_fn("proxy::info", &format!("Loaded {rule_count} proxy rules"));
    let server = Arc::new(args.server_addr);
    let password = Arc::new(args.password);
    let mut tasks = JoinSet::new();
    let tunnels = tokio_util::task::TaskTracker::new();
    log_fn(
      "proxy::info",
      &format!(
        "CloudTun listening at {}:{}",
        args.local_addr.0, args.local_addr.1
      ),
    );
    loop {
      tokio::select! {
        biased;
        _ = shutdown_token.cancelled() => break,
        _ = tasks.join_next(), if !tasks.is_empty() => {},
        accepted = listener.accept() => {
          let (stream, _) = match accepted {
            Ok(value) => value,
            Err(error) => {
              log_fn("proxy::error", &format!("accept failed: {error}"));
              tokio::select! {
                _ = shutdown_token.cancelled() => break,
                _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {}
              }
              continue;
            }
          };
          let _ = stream.set_nodelay(true);
          let tracker = tunnels.clone();
          let cancel = shutdown_token.clone();
          let server = server.clone();
          let password = password.clone();
          let router = router.clone();
          let log = log_fn.clone();
          tasks.spawn(async move {
            let service_cancel = cancel.clone();
            let service_log = log.clone();
            let service = hyper::service::service_fn(move |req: Request<Incoming>| {
              let server = server.clone();
              let password = password.clone();
              let router = router.clone();
              let log = service_log.clone();
              let cancel = service_cancel.clone();
              let tracker = tracker.clone();
              async move {
                if req.method() == Method::CONNECT {
                  proxy_request(req, server, router, password, log, cancel, tracker).await
                } else {
                  Ok((StatusCode::NOT_IMPLEMENTED, "Only CONNECT is supported").into_response())
                }
              }
            });
            tokio::select! {
              biased;
              _ = cancel.cancelled() => {},
              result = http1::Builder::new()
                .serve_connection(TokioIo::new(stream), service).with_upgrades() => {
                if let Err(error) = result {
                  log("proxy::error", &format!("HTTP connection failed: {error}"));
                }
              }
            }
          });
        }
      }
    }
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    tunnels.close();
    tunnels.wait().await;
    Ok(())
  }
}
pub async fn run_proxy_loop<F: Fn(&str, &str) + Send + Sync + 'static>(
  args: ProxyArgs,
  shutdown_token: CancellationToken,
  log_fn: F,
) -> Result<(), Box<dyn std::error::Error>> {
  PreparedProxy::bind(args)
    .await?
    .run(shutdown_token, log_fn)
    .await
}

#[cfg(test)]
mod tests {
  use super::*;
  use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    time::{Duration, timeout},
  };
  fn args() -> ProxyArgs {
    ProxyArgs {
      server_addr: ("127.0.0.1".into(), 1, "token".into()),
      password: vec![0; 16],
      local_addr: ("127.0.0.1".into(), 0),
      default_rule: MatchType::Direct,
      proxy_rules: None,
    }
  }
  #[tokio::test]
  async fn stop_closes_accepted_connections_and_releases_port() {
    let proxy = PreparedProxy::bind(args()).await.unwrap();
    let addr = proxy.listener.local_addr().unwrap();
    let cancel = CancellationToken::new();
    let token = cancel.clone();
    let task = tokio::spawn(async move { proxy.run(token, |_, _| {}).await.unwrap() });
    let mut client = TcpStream::connect(addr).await.unwrap();
    // A response confirms accept and request processing happened before cancellation.
    client
      .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n")
      .await
      .unwrap();
    let mut bytes = [0; 1024];
    timeout(Duration::from_secs(2), client.read(&mut bytes))
      .await
      .unwrap()
      .unwrap();
    cancel.cancel();
    task.await.unwrap();
    assert_eq!(
      timeout(Duration::from_secs(2), client.read(&mut bytes))
        .await
        .unwrap()
        .unwrap(),
      0
    );
    TcpListener::bind(addr).await.unwrap();
  }
  #[tokio::test]
  async fn bind_failure_is_reported_before_start() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut args = args();
    args.local_addr.1 = listener.local_addr().unwrap().port();
    assert!(PreparedProxy::bind(args).await.is_err());
  }
  #[tokio::test]
  async fn stop_waits_for_upgraded_tunnels_to_close() {
    let target = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target_addr = target.local_addr().unwrap();
    let proxy = PreparedProxy::bind(args()).await.unwrap();
    let addr = proxy.listener.local_addr().unwrap();
    let cancel = CancellationToken::new();
    let token = cancel.clone();
    let task = tokio::spawn(async move { proxy.run(token, |_, _| {}).await.unwrap() });
    let mut client = TcpStream::connect(addr).await.unwrap();
    client
      .write_all(
        format!("CONNECT {target_addr} HTTP/1.1\r\nHost: {target_addr}\r\n\r\n").as_bytes(),
      )
      .await
      .unwrap();
    let mut header = Vec::new();
    while !header.ends_with(b"\r\n\r\n") {
      header.push(
        timeout(Duration::from_secs(2), client.read_u8())
          .await
          .unwrap()
          .unwrap(),
      );
    }
    assert!(header.starts_with(b"HTTP/1.1 200"));
    let (mut remote, _) = timeout(Duration::from_secs(2), target.accept())
      .await
      .unwrap()
      .unwrap();
    client.write_all(b"connected").await.unwrap();
    let mut data = [0; 9];
    timeout(Duration::from_secs(2), remote.read_exact(&mut data))
      .await
      .unwrap()
      .unwrap();
    cancel.cancel();
    timeout(Duration::from_secs(2), task)
      .await
      .unwrap()
      .unwrap();
    let mut byte = [0];
    assert_eq!(
      timeout(Duration::from_secs(2), remote.read(&mut byte))
        .await
        .unwrap()
        .unwrap(),
      0
    );
    assert_eq!(
      timeout(Duration::from_secs(2), client.read(&mut byte))
        .await
        .unwrap()
        .unwrap(),
      0
    );
    TcpListener::bind(addr).await.unwrap();
  }
}
