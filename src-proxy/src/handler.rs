use std::{net::SocketAddr, time::Duration};

use axum::{body::Body, extract::Request, routing::get};
use hyper::{Method, body::Incoming, server::conn::http1};
use hyper_util::rt::TokioIo;
use tokio::{net::TcpListener, runtime::Runtime};
use tokio_util::sync::CancellationToken;
use tower::{Service, ServiceExt};

use crate::{proxy::proxy_request, router::Router};

pub struct ProxyHandler {
  router: Router,
  shutdown_token: Option<CancellationToken>,
  rt: Option<Runtime>,
}

async fn start_proxy(cancel_token: CancellationToken) {
  let router_svc = axum::Router::new().route("/", get(|| async { "Ok!" }));

  let tower_service = tower::service_fn(move |req: Request<_>| {
    let router_svc = router_svc.clone();
    let req = req.map(Body::new);
    async move {
      if req.method() == Method::CONNECT {
        proxy_request(req).await
      } else {
        router_svc.oneshot(req).await.map_err(|err| match err {})
      }
    }
  });

  let hyper_service = hyper::service::service_fn(move |request: Request<Incoming>| {
    tower_service.clone().call(request)
  });

  let addr = SocketAddr::from(([127, 0, 0, 1], 7891));
  println!("listening on {}", addr);

  let listener = TcpListener::bind(addr).await.unwrap();

  loop {
    let accept_future = listener.accept();
    let cancel_future = cancel_token.cancelled();

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
        break;
      }
    }
  }
}

impl ProxyHandler {
  pub fn new() -> Self {
    ProxyHandler {
      router: Router::new(),
      shutdown_token: None,
      rt: None,
    }
  }

  pub fn start_loop(&mut self) {
    let rt = Runtime::new().unwrap();
    let cancel_token = CancellationToken::new();
    let cancel_token2 = cancel_token.clone();
    rt.spawn(async move {
      start_proxy(cancel_token2);
    });
    self.shutdown_token.replace(cancel_token);
    self.rt.replace(rt);
  }

  pub fn stop_loop(&mut self) {
    self.shutdown_token.take().map(|tk| tk.cancel());

    self
      .rt
      .take()
      .map(|rt| rt.shutdown_timeout(Duration::from_secs(6)));
  }
}
