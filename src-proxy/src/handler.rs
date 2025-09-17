use std::sync::Arc;

use axum::{body::Body, extract::Request, response::IntoResponse};
use hyper::{Method, body::Incoming, server::conn::http1};
use hyper_util::rt::TokioIo;
use tokio::{net::TcpListener, runtime::Runtime};
use tokio_util::sync::CancellationToken;
use tower::Service;

use crate::{
  StartProxyArgs,
  proxy::proxy_request,
  route::{MatchType, RouteMatcher},
};

async fn start_proxy(
  cancel_token: CancellationToken,
  server: Arc<(String, u16)>,
  local: Arc<(String, u16)>,
  default_rule: MatchType,
  router: RouteMatcher,
) {
  // let router_svc = axum::Router::new().route("/", get(|| async { "Ok2!" }));
  let server2 = server.clone();
  let router2 = router.clone();
  let tower_service = tower::service_fn(move |req: Request<_>| {
    let router_matcher = router2.clone();
    let server = server2.clone();
    let req = req.map(Body::new);
    async move {
      if req.method() == Method::CONNECT {
        proxy_request(req, server, default_rule, router_matcher).await
      } else {
        Ok("to be implemented".into_response())
      }
    }
  });
  //  let router_svc = router_svc.clone();
  //   //
  //    println!("XXX {}", req.uri().ho);
  //         router_svc.oneshot(req).await.map_err(|err| match err {})
  //
  let hyper_service = hyper::service::service_fn(move |request: Request<Incoming>| {
    tower_service.clone().call(request)
  });

  let listener = TcpListener::bind((local.0.clone(), local.1)).await.unwrap();

  println!(
    "CloudTun Client Listening at {}:{}\n  Proxy to ==> {}:{}\n  Default Rule: {}",
    local.0, local.1, server.0, server.1, default_rule
  );
  router.config_file.map(|f| {
    println!("  Rules File: {f}");
  });

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
  default_rule: MatchType,
  shutdown_token: CancellationToken,
  rt: Option<Runtime>,
  server: Arc<(String, u16)>,
  local: Arc<(String, u16)>,
}

impl ProxyHandler {
  pub fn new() -> Self {
    ProxyHandler {
      router: RouteMatcher::new(),
      default_rule: MatchType::Direct,
      shutdown_token: CancellationToken::new(),
      rt: None,
      server: Arc::new(("".to_string(), 0)),
      local: Arc::new(("".to_string(), 0)),
    }
  }

  pub fn init_rt(&mut self, args: StartProxyArgs) {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
      if let Err(e) = self.router.config_rules(args.rules_config_file).await {
        eprintln!("Failed load rules config file: {e}")
      }
    });
    self.rt = Some(rt);
    self.server = Arc::new((args.server_ip, args.server_port));
    self.local = Arc::new((args.local_ip, args.local_port));
    self.default_rule = args.default_rule;
  }

  pub fn deinit_rt(&mut self) {
    let rt = self.rt.take();
    rt.map(|rt| rt.block_on(self.router.config_rules(None))); // 回收内存
  }

  pub fn start_loop(&self) {
    let cancel_token = self.shutdown_token.clone();
    let router = self.router.clone();
    let server = self.server.clone();
    let local = self.local.clone();
    let default_rule = self.default_rule;
    self.rt.as_ref().unwrap().block_on(async move {
      start_proxy(cancel_token, server, local, default_rule, router).await;
    });
  }

  pub fn stop_loop(&self) {
    self.shutdown_token.cancel();
  }
}
