use std::sync::Arc;

use axum::{
  body::Body,
  response::{IntoResponse, Response},
};
use hyper::{Request, StatusCode, body::Incoming, upgrade::Upgraded};
use hyper_util::rt::TokioIo;
use tokio::net::TcpStream;

use crate::{
  route::{MatchType, RouteMatcher},
  tunnel::proxy_tunnel,
};

pub async fn proxy_request<F: Fn(&str, &str) + Send + Sync + 'static>(
  req: Request<Incoming>,
  server_addr: Arc<(String, u16, String)>,
  router: RouteMatcher,
  secret: Arc<(Vec<u8>, String)>,
  log_fn: Arc<F>,
) -> Result<Response, hyper::Error> {
  let Some(remote_auth) = req.uri().authority() else {
    log_fn(
      "proxy::error",
      &format!("CONNECT host is not socket addr: {:?}", req.uri()),
    );
    return Ok(
      (
        StatusCode::BAD_REQUEST,
        "CONNECT must be to a socket address",
      )
        .into_response(),
    );
  };
  let remote_host = remote_auth.host().to_owned();
  let remote_port = remote_auth.port_u16().unwrap_or(80);

  tokio::spawn(async move {
    match hyper::upgrade::on(req).await {
      Ok(upgraded) => match router.match_domain(&remote_host).await {
        MatchType::Deny => {
          drop(upgraded);
        }
        MatchType::Proxy => {
          match proxy_tunnel(
            upgraded,
            server_addr,
            remote_host,
            remote_port,
            secret,
            log_fn.clone(),
          )
          .await
          {
            Ok(_) => (),
            Err(err) => {
              log_fn("proxy::error", &format!("proxy tunnel error: {}", err));
            }
          }
        }
        MatchType::Direct => {
          match proxy_direct(upgraded, remote_host, remote_port, log_fn.clone()).await {
            Ok(_) => (),
            Err(err) => {
              log_fn("proxy::error", &format!("proxy direct error: {}", err));
            }
          }
        }
      },
      Err(e) => log_fn("proxy::error", &format!("upgrade error: {}", e)),
    }
  });
  Ok(Response::new(Body::empty()))
}

async fn proxy_direct<F: Fn(&str, &str) + Send + Sync + 'static>(
  upgraded: Upgraded,
  remote_host: String,
  remote_port: u16,
  log_fn: Arc<F>,
) -> std::io::Result<()> {
  log_fn("proxy::info", &format!("Direct ==> {remote_host}"));
  let mut remote_server = TcpStream::connect((remote_host, remote_port)).await?;
  let mut upgraded = TokioIo::new(upgraded);

  let _ = tokio::io::copy_bidirectional(&mut upgraded, &mut remote_server).await?;

  // println!(
  //   "client wrote {} bytes and received {} bytes",
  //   from_client, from_server
  // );

  Ok(())
}
