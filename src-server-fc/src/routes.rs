use std::sync::Arc;

use axum::{
  extract::State,
  http::{HeaderMap, StatusCode},
};

use crate::context::Context;

pub async fn test_handler(State(ctx): State<Arc<Context>>) -> Result<&'static str, StatusCode> {
  Ok("Hello, World!")
}
