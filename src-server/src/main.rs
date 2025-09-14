mod routes; 
mod ws;
mod config;
mod proxy;

 

  
 
use axum::{  routing::{any, get}, Router};

//allows to split the websocket stream into separate TX and RX branches

use crate::{routes::home_handler, ws::ws_handler};

#[tokio::main]
async fn main() {
//   tracing_subscriber::registry()
//     .with(
//       tracing_subscriber::EnvFilter::try_from_default_env()
//         .unwrap_or_else(|_| format!("{}=debug,tower_http=debug", env!("CARGO_CRATE_NAME")).into()),
//     )
//     .with(tracing_subscriber::fmt::layer())
//     .init();

  // let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets");

  // build our application with some routes
  let app = Router::new()
    // .fallback_service(ServeDir::new(assets_dir).append_index_html_on_directories(true))
    .route("/", get(home_handler))
    .route("/ws", any(ws_handler));
    // logging so we can see what's going on
    // .layer(
    //   TraceLayer::new_for_http().make_span_with(DefaultMakeSpan::default().include_headers(true)),
    // );

  // run it with hyper
  let listener = tokio::net::TcpListener::bind("0.0.0.0:24816")
    .await
    .unwrap();
  // tracing::debug!("listening on {}", listener.local_addr().unwrap());
  axum::serve(
    listener,
    app.into_make_service(),
  )
  .await
  .unwrap();
}
