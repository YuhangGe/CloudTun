use axum::{
  body::Body,
  extract::Request,
  response::{IntoResponse, Response},
};
use hyper::{StatusCode, upgrade::Upgraded};
use hyper_util::rt::TokioIo;
use tokio::net::TcpStream;

use crate::{
  route::{MatchType, RouteMatcher},
  tunnel::proxy_tunnel,
};

pub async fn proxy_request(req: Request, router: RouteMatcher) -> Result<Response, hyper::Error> {
  let Some(remote_auth) = req.uri().authority() else {
    eprintln!("CONNECT host is not socket addr: {:?}", req.uri());
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

  tokio::task::spawn(async move {
    match hyper::upgrade::on(req).await {
      Ok(upgraded) => match router.match_domain(&remote_host).await {
        Some(MatchType::Deny) => {
          // println!("Deny ==> {}", host);
          drop(upgraded);
        }
        Some(MatchType::Proxy) => {
          match proxy_tunnel(upgraded, "127.0.0.1", remote_host, remote_port).await {
            Ok(_) => (),
            Err(err) => {
              println!("proxy tunnel error: {}", err);
            }
          }
        }
        _ => match proxy_direct(upgraded, remote_host, remote_port).await {
          Ok(_) => (),
          Err(err) => {
            println!("proxy direct error: {}", err);
          }
        },
      },
      Err(e) => eprintln!("upgrade error: {}", e),
    }
  });

  Ok(Response::new(Body::empty()))
}

async fn proxy_direct(
  upgraded: Upgraded,
  remote_host: String,
  remote_port: u16,
) -> std::io::Result<()> {
  // println!("Direct ==> {}", host);
  let mut remote_server = TcpStream::connect((remote_host, remote_port)).await?;
  let mut upgraded = TokioIo::new(upgraded);

  let _ = tokio::io::copy_bidirectional(&mut upgraded, &mut remote_server).await?;

  // println!(
  //   "client wrote {} bytes and received {} bytes",
  //   from_client, from_server
  // );

  Ok(())
}
