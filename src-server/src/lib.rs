pub mod context;
pub mod proxy;
pub mod routes;
pub mod tls;
use axum::{
  Router,
  routing::{any, get},
};
use hyper_util::{
  rt::{TokioExecutor, TokioIo},
  server::conn::auto::Builder,
  service::TowerToHyperService,
};
use std::{io, sync::Arc, time::Duration};
use tokio_rustls::{TlsAcceptor, rustls};
use tokio_util::sync::CancellationToken;
pub fn router(context: Arc<context::Context>) -> Router {
  Router::new()
    .route("/ping", get(routes::ping_handler))
    .route("/ws", any(proxy::proxy_handler))
    .route("/udp", any(proxy::udp_proxy_handler))
    .with_state(context)
}
pub async fn serve(
  listener: tokio::net::TcpListener,
  app: Router,
  tls: Arc<rustls::ServerConfig>,
  cancel: CancellationToken,
) -> io::Result<()> {
  let acceptor = TlsAcceptor::from(tls);
  let mut tasks = tokio::task::JoinSet::new();
  loop {
    tokio::select! {
      biased;
      _ = cancel.cancelled() => break,
      _ = tasks.join_next(), if !tasks.is_empty() => {},
      accepted = listener.accept() => {
        let (tcp, _) = accepted?; tcp.set_nodelay(true)?;
        let acceptor = acceptor.clone(); let app = app.clone(); let cancel = cancel.clone();
        tasks.spawn(async move {
          let tls = tokio::select! { biased; _ = cancel.cancelled() => return, result = tokio::time::timeout(Duration::from_secs(10), acceptor.accept(tcp)) => match result { Ok(Ok(tls)) => tls, _ => return } };
          let builder = Builder::new(TokioExecutor::new());
          tokio::select! { biased; _ = cancel.cancelled() => {}, _ = builder.serve_connection_with_upgrades(TokioIo::new(tls), TowerToHyperService::new(app)) => {} }
        });
      }
    }
  }
  tasks.abort_all();
  while tasks.join_next().await.is_some() {}
  Ok(())
}
