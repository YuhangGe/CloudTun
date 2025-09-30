use std::sync::Arc;

use axum::{extract::Request, response::IntoResponse};
use cloudtun_common::util::hex2str;
use hyper::{Method, body::Incoming, server::conn::http1};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use crate::{
  proxy::proxy_request,
  route::{MatchType, RouteMatcher},
};

pub struct ProxyArgs {
  pub server_addr: (String, u16, String),
  pub password: Vec<u8>,
  pub local_addr: (String, u16),
  pub default_rule: MatchType,
  pub proxy_rules: Option<String>,
}

pub async fn run_proxy_loop<F: Fn(&str, &str) + Send + Sync + 'static>(
  args: ProxyArgs,
  shutdown_token: CancellationToken,
  log_fn: F,
) -> Result<(), Box<dyn std::error::Error>> {
  let router: RouteMatcher = RouteMatcher::load(args.default_rule, args.proxy_rules).await?;
  let proxy_rules_count = router.get_count().await;
  let log_fn = Arc::new(log_fn);
  let log_fn2 = log_fn.clone();
  let server_addr = Arc::new(args.server_addr.clone());
  let password = Arc::new(args.password);
  println!("Password: {}", hex2str(&password));

  let hyper_service = hyper::service::service_fn(move |req: Request<Incoming>| {
    let server_addr = server_addr.clone();
    let router = router.clone();
    let log_fn = log_fn2.clone();
    let password = password.clone();
    async move {
      if req.method() == Method::CONNECT {
        proxy_request(req, server_addr, router, password, log_fn).await
      } else {
        Ok("to be implemented".into_response())
      }
    }
  });

  let listener = TcpListener::bind(&args.local_addr).await?;

  log_fn(
    "proxy::info",
    &format!(
      "CloudTun Client Listening at {}:{}",
      args.local_addr.0, args.local_addr.1,
    ),
  );
  log_fn(
    "proxy::info",
    &format!("Proxy to ==> {}:{}", args.server_addr.0, args.server_addr.1,),
  );
  log_fn(
    "proxy::info",
    &format!(
      "Proxy Rules: {}, default {}",
      proxy_rules_count, args.default_rule
    ),
  );

  loop {
    let accept_future = listener.accept();
    let cancel_future = shutdown_token.cancelled();

    tokio::select! {
      result = accept_future => {
        match result {
          Ok((stream, _)) => {
            let io = TokioIo::new(stream);
            let hyper_service = hyper_service.clone();
            let log_fn = log_fn.clone();
            tokio::task::spawn(async move {
              if let Err(err) = http1::Builder::new()
                .preserve_header_case(true)
                .title_case_headers(true)
                .serve_connection(io, hyper_service)
                .with_upgrades()
                .await
              {
                log_fn("proxy::error", &format!("Failed to serve connection: {err:?}"));
              }
            });

          },
          Err(err) => {
            log_fn("proxy::error", &format!("failed accept tcp: {err}"));
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
