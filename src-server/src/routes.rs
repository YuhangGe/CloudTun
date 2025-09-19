use std::sync::Arc;

use axum::{
  extract::State,
  http::{HeaderMap, StatusCode},
};
use cloudtun_common::constant::X_TOKEN_KEY;

use crate::context::Context;

pub async fn ping_handler(
  headers: HeaderMap,
  State(context): State<Arc<Context>>,
) -> Result<&'static str, StatusCode> {
  if !headers
    .get(X_TOKEN_KEY)
    .map(|tk| tk.eq(&context.token))
    .unwrap_or(false)
  {
    return Err(StatusCode::UNAUTHORIZED);
  }
  context.touch_ping_ts().await;
  Ok("pong!")
}
