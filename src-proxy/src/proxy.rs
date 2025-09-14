use axum::{
  body::Body,
  extract::Request,
  response::{IntoResponse, Response},
};
use hyper::{StatusCode, upgrade::Upgraded};
use hyper_util::rt::TokioIo;
use tokio::net::TcpStream;

use crate::route::{MatchType, RouteMatcher};

pub async fn proxy_request(req: Request, router: RouteMatcher) -> Result<Response, hyper::Error> {
  let Some(host) = req.uri().host() else {
    eprintln!("CONNECT host is not socket addr: {:?}", req.uri());
    return Ok(
      (
        StatusCode::BAD_REQUEST,
        "CONNECT must be to a socket address",
      )
        .into_response(),
    );
  };

  let host = host.to_string();
  let port = req.uri().port_u16().unwrap_or(80);
  tokio::task::spawn(async move {
    match hyper::upgrade::on(req).await {
      Ok(upgraded) => match router.match_domain(&host).await {
        Some(MatchType::Deny) => {
          // println!("Deny ==> {}", host);
          drop(upgraded);
        }
        Some(MatchType::Proxy) => {}
        _ => match proxy_direct(upgraded, &host, port).await {
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

async fn proxy_direct(upgraded: Upgraded, host: &str, port: u16) -> std::io::Result<()> {
  // println!("Direct ==> {}", host);
  let mut server = TcpStream::connect((host, port)).await?;
  let mut upgraded = TokioIo::new(upgraded);

  let _ = tokio::io::copy_bidirectional(&mut upgraded, &mut server).await?;

  // println!(
  //   "client wrote {} bytes and received {} bytes",
  //   from_client, from_server
  // );

  Ok(())
}
