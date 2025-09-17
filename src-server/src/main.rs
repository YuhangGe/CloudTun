mod routes; 
mod proxy;
 
use axum::{  routing::{any, get}, Router};
use cloudtun_common::REMOTE_PROXY_PORT;

//allows to split the websocket stream into separate TX and RX branches

use crate::{routes::home_handler, proxy::proxy_handler};

#[tokio::main]
async fn main() {
  // build our application with some routes
  let app = Router::new()
     .route("/", get(home_handler))
    .route("/ws", any(proxy_handler));
    
   let listener = tokio::net::TcpListener::bind(("0.0.0.0", REMOTE_PROXY_PORT))
    .await
    .unwrap();

   println!("cloudtun server listening at 0.0.0.0:{REMOTE_PROXY_PORT}");
   
   axum::serve(
    listener,
    app.into_make_service(),
  )
  .await
  .unwrap();
}
