use axum::{
  body::Body,
  extract::Request,
  response::{IntoResponse, Response},
};
use hyper::{StatusCode, upgrade::Upgraded};
use hyper_util::rt::TokioIo;
use tokio::net::TcpStream;

pub async fn proxy_request(req: Request) -> Result<Response, hyper::Error> {
  if let Some(host_addr) = req.uri().authority().map(|auth| auth.to_string()) {
    tokio::task::spawn(async move {
      match hyper::upgrade::on(req).await {
        Ok(upgraded) => {
          if let Err(e) = proxy_tunnel(upgraded, host_addr).await {
            eprintln!("server io error: {}", e);
          };
        }
        Err(e) => eprintln!("upgrade error: {}", e),
      }
    });

    Ok(Response::new(Body::empty()))
  } else {
    eprintln!("CONNECT host is not socket addr: {:?}", req.uri());
    Ok(
      (
        StatusCode::BAD_REQUEST,
        "CONNECT must be to a socket address",
      )
        .into_response(),
    )
  }
}

async fn proxy_tunnel(upgraded: Upgraded, addr: String) -> std::io::Result<()> {
  let mut server = TcpStream::connect(addr).await?;
  let mut upgraded = TokioIo::new(upgraded);

  let (from_client, from_server) =
    tokio::io::copy_bidirectional(&mut upgraded, &mut server).await?;

  println!(
    "client wrote {} bytes and received {} bytes",
    from_client, from_server
  );

  Ok(())
}
