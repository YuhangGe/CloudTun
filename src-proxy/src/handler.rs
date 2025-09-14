use std::net::SocketAddr;

use axum::{body::Body, extract::Request, routing::get};
use hyper::{Method, body::Incoming, server::conn::http1};
use hyper_util::rt::TokioIo;
use tokio::{net::TcpListener, runtime::Runtime};
use tokio_util::sync::CancellationToken;
use tower::{Service, ServiceExt};

use crate::{proxy::proxy_request, route::RouteMatcher};

async fn start_proxy(cancel_token: CancellationToken, router: RouteMatcher) {
  let router_svc = axum::Router::new().route("/", get(|| async { "Ok2!" }));
  let tower_service = tower::service_fn(move |req: Request<_>| {
    let router_svc = router_svc.clone();
    let router_matcher = router.clone();
    let req = req.map(Body::new);
    async move {
      if req.method() == Method::CONNECT {
        proxy_request(req, router_matcher).await
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
        println!("Got cancel notify");
        break;
      }
    }
  }
}

pub struct ProxyHandler {
  router: RouteMatcher,
  shutdown_token: CancellationToken,
  rt: Option<Runtime>,
}

impl ProxyHandler {
  pub fn new() -> Self {
    ProxyHandler {
      router: RouteMatcher::new(),
      shutdown_token: CancellationToken::new(),
      rt: None,
    }
  }

  pub fn init_rt(&mut self, rules: Option<String>) {
    let rt = Runtime::new().unwrap();
    rt.block_on(self.router.config_rules(rules));
    self.rt = Some(rt);
  }

  pub fn deinit_rt(&mut self) {
    let rt = self.rt.take();
    rt.map(|rt| rt.block_on(self.router.config_rules(None))); // 回收内存
  }

  pub fn start_loop(&self) {
    let cancel_token = self.shutdown_token.clone();
    let router = self.router.clone();
    self.rt.as_ref().unwrap().block_on(async move {
      start_proxy(cancel_token, router).await;
    });
  }

  pub fn stop_loop(&self) {
    self.shutdown_token.cancel();
  }
}
