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

pub async fn test_handler(State(ctx): State<Arc<Context>>) -> Result<&'static str, StatusCode> {
  let ret = match ctx.tx.describe_instances().await {
    Err(e) => {
      eprintln!("{e}");
      return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    Ok(ret) => ret,
  };
  let Some(inst) = ret.iter().find(|inst| inst.name.eq("vray::proxy")) else {
    return Err(StatusCode::NOT_FOUND);
  };
  match ctx.tx.desroy_instance(&inst.id).await {
    Err(e) => {
      eprintln!("{e}");
      Err(StatusCode::INTERNAL_SERVER_ERROR)
    }
    Ok(_) => Ok("success!"),
  }
}
