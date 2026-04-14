use std::sync::Arc;

#[derive(Debug)]
pub struct Context {
  pub password: Arc<Vec<u8>>,
  pub token: String,
}

impl Context {
  pub fn new(token: String, password: Vec<u8>) -> Self {
    Context {
      password: Arc::new(password),
      token,
    }
  }
}
