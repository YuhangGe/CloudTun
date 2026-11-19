use cloudtun_common::tls::private_write;
use serde::{Deserialize, Serialize};
use std::{path::Path, sync::Arc};
use tokio_rustls::rustls::{
  self,
  pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer},
};
#[derive(Serialize, Deserialize)]
pub struct Identity {
  certificate: Vec<u8>,
  key: Vec<u8>,
  pub token: String,
}
impl Identity {
  pub fn load_or_create(directory: &Path, token: Option<String>) -> anyhow::Result<Self> {
    let path = directory.join("identity.json");
    let value = match std::fs::read(&path) {
      Ok(bytes) => {
        let value: Self = serde_json::from_slice(&bytes)?;
        anyhow::ensure!(
          token.as_ref().is_none_or(|token| token == &value.token),
          "访问凭据与已保存的服务端身份不一致"
        );
        value
      }
      Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
        let rcgen::CertifiedKey { cert, signing_key } =
          rcgen::generate_simple_self_signed(vec!["cloudtun.local".into()])?;
        let value = Self {
          certificate: cert.der().to_vec(),
          key: signing_key.serialize_der(),
          token: token.unwrap_or_else(|| hex::encode(rand::random::<[u8; 32]>())),
        };
        value.validate()?;
        private_write(&path, &serde_json::to_vec(&value)?)?;
        value
      }
      Err(error) => return Err(error.into()),
    };
    value.validate()?;
    Ok(value)
  }
  fn validate(&self) -> anyhow::Result<()> {
    anyhow::ensure!(
      self.token.len() == 64 && self.token.bytes().all(|b| b.is_ascii_hexdigit()),
      "访问凭据必须是 32 字节随机值的十六进制编码"
    );
    Ok(())
  }
  pub fn config(&self) -> anyhow::Result<Arc<rustls::ServerConfig>> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ServerConfig::builder_with_provider(provider)
      .with_safe_default_protocol_versions()?
      .with_no_client_auth()
      .with_single_cert(
        vec![CertificateDer::from(self.certificate.clone())],
        PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(self.key.clone())),
      )?;
    Ok(Arc::new(config))
  }
}
