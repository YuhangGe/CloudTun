use crate::{
  route::{MatchType, RouteMatcher},
  tunnel::proxy_tunnel,
};
use axum::{
  body::Body,
  response::{IntoResponse, Response},
};
use hyper::{Request, StatusCode, body::Incoming, upgrade::Upgraded};
use hyper_util::rt::TokioIo;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio_util::{sync::CancellationToken, task::TaskTracker};

pub async fn proxy_request<F: Fn(&str, &str) + Send + Sync + 'static>(
  req: Request<Incoming>,
  server: Arc<(String, u16, String)>,
  router: RouteMatcher,
  secret: Arc<Vec<u8>>,
  log: Arc<F>,
  cancel: CancellationToken,
  tracker: TaskTracker,
) -> Result<Response, hyper::Error> {
  let Some(authority) = req.uri().authority() else {
    return Ok((StatusCode::BAD_REQUEST, "CONNECT requires a host and port").into_response());
  };
  let host = authority
    .host()
    .trim_start_matches('[')
    .trim_end_matches(']')
    .to_owned();
  let Some(port) = authority.port_u16().filter(|port| *port != 0) else {
    return Ok((StatusCode::BAD_REQUEST, "CONNECT requires a valid port").into_response());
  };
  let route = router.match_domain(&host).await;
  if matches!(route, MatchType::Deny) {
    return Ok(StatusCode::FORBIDDEN.into_response());
  }
  tracker.spawn(async move {
    tokio::select! {
      biased;
      _ = cancel.cancelled() => {},
      result = async {
        let upgraded = hyper::upgrade::on(req).await.map_err(std::io::Error::other)?;
        match route {
          MatchType::Proxy => proxy_tunnel(upgraded, server, host, port, secret, log.clone()).await,
          MatchType::Direct => proxy_direct(upgraded, host, port, log.clone()).await,
          MatchType::Deny => Ok(()),
        }
      } => {
        if let Err(error) = result { log("proxy::error", &format!("tunnel closed: {error}")); }
      }
    }
  });
  Ok(Response::new(Body::empty()))
}

async fn proxy_direct<F: Fn(&str, &str) + Send + Sync + 'static>(
  upgraded: Upgraded,
  host: String,
  port: u16,
  log: Arc<F>,
) -> std::io::Result<()> {
  log("proxy::info", &format!("Direct ==> {host}:{port}"));
  let mut remote = tokio::time::timeout(
    cloudtun_common::proxy::CONNECT_TIMEOUT,
    TcpStream::connect((host, port)),
  )
  .await??;
  remote.set_nodelay(true)?;
  let mut local = TokioIo::new(upgraded);
  tokio::io::copy_bidirectional(&mut local, &mut remote).await?;
  Ok(())
}
