use std::sync::Arc;

use axum::{extract::Request, response::IntoResponse};
use hyper::{Method, body::Incoming, server::conn::http1};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use crate::{
  proxy::proxy_request,
  route::{MatchType, RouteMatcher},
};

pub struct ProxyArgs {
  pub server_addr: (String, u16),
  pub local_addr: (String, u16),
  pub default_rule: MatchType,
  pub rules_config_file: Option<String>,
}

pub async fn run_proxy_loop(
  args: ProxyArgs,
  shutdown_token: CancellationToken,
) -> Result<(), Box<dyn std::error::Error>> {
  let router = RouteMatcher::load(args.default_rule, args.rules_config_file.clone()).await?;

  let server_addr = Arc::new(args.server_addr.clone());
  let hyper_service = hyper::service::service_fn(move |req: Request<Incoming>| {
    let server_addr = server_addr.clone();
    let router = router.clone();
    async move {
      if req.method() == Method::CONNECT {
        proxy_request(req, server_addr, router).await
      } else {
        Ok("to be implemented".into_response())
      }
    }
  });

  let listener = TcpListener::bind(&args.local_addr).await?;

  println!(
    "CloudTun Client Listening at {}:{}\n  Proxy to ==> {}:{}\n  Default Rule: {}",
    args.local_addr.0, args.local_addr.1, args.server_addr.0, args.server_addr.1, args.default_rule
  );
  args.rules_config_file.map(|f| {
    println!("  Rules File: {f}");
  });

  loop {
    let accept_future = listener.accept();
    let cancel_future = shutdown_token.cancelled();

    tokio::select! {
      result = accept_future => {
        match result {
          Ok((stream, _)) => {
            let io = TokioIo::new(stream);
            let hyper_service = hyper_service.clone();
            tokio::task::spawn(async move {
              if let Err(err) = http1::Builder::new()
                .preserve_header_case(true)
                .title_case_headers(true)
                .serve_connection(io, hyper_service)
                .with_upgrades()
                .await
              {
                println!("Failed to serve connection: {err:?}");
              }
            });

          },
          Err(err) => {
            eprintln!("failed accept tcp: {}", err);
          }
        };
      },
      _ = cancel_future => {
        // println!("Got cancel notify");
        break Ok(());
      }
    }
  }
}
