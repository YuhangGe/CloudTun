//! TLS transport with trust-on-first-use, scoped to a cloud instance identity.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
  io,
  path::{Path, PathBuf},
  sync::{Arc, Mutex, OnceLock},
};
use tokio::{
  net::TcpStream,
  time::{Duration, timeout},
};
use tokio_rustls::{
  TlsConnector,
  client::TlsStream,
  rustls::{
    self, DigitallySignedStruct, SignatureScheme,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, ServerName, UnixTime},
  },
};

#[derive(Clone)]
pub struct CloudEndpoint {
  pub host: String,
  pub port: u16,
  pub identity: String,
  pub token: String,
  pub trust_dir: PathBuf,
}
impl CloudEndpoint {
  pub fn new(
    host: String,
    port: u16,
    identity: String,
    token: String,
    trust_dir: PathBuf,
  ) -> io::Result<Self> {
    if identity.is_empty()
      || port == 0
      || token.len() != 64
      || !token.bytes().all(|b| b.is_ascii_hexdigit())
    {
      return Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        "需要实例标识和独立的 256 位访问凭据",
      ));
    }
    Ok(Self {
      host,
      port,
      identity,
      token,
      trust_dir,
    })
  }
  fn pin_path(&self) -> PathBuf {
    self
      .trust_dir
      .join(format!("{}.json", fingerprint(self.identity.as_bytes())))
  }
  pub async fn connect(&self) -> io::Result<TlsStream<TcpStream>> {
    let expected = read_pin(&self.pin_path(), &self.identity)
      .map_err(|error| io::Error::other(format!("无法读取证书信任记录：{error}")))?;
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider.clone())
      .with_safe_default_protocol_versions()
      .map_err(io::Error::other)?
      .dangerous()
      .with_custom_certificate_verifier(Arc::new(PinnedVerifier {
        expected: expected.clone(),
        provider,
      }))
      .with_no_client_auth();
    let stream = timeout(Duration::from_secs(15), async {
      let tcp = TcpStream::connect((self.host.as_str(), self.port)).await?;
      tcp.set_nodelay(true)?;
      let name = ServerName::try_from(self.host.clone()).map_err(io::Error::other)?;
      TlsConnector::from(Arc::new(config))
        .connect(name, tcp)
        .await
    })
    .await
    .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "WSS TLS 连接超时"))??;
    let cert = stream
      .get_ref()
      .1
      .peer_certificates()
      .and_then(|c| c.first())
      .ok_or_else(|| io::Error::other("服务端未提供证书"))?;
    // Commit only after CertificateVerify succeeds. Parallel first connections must agree.
    remember_pin(
      &self.pin_path(),
      &self.identity,
      &fingerprint(cert.as_ref()),
    )?;
    Ok(stream)
  }
}
pub fn fingerprint(bytes: &[u8]) -> String {
  hex::encode(Sha256::digest(bytes))
}
#[derive(Serialize, Deserialize)]
struct Pin {
  identity: String,
  sha256: String,
}
fn read_pin(path: &Path, identity: &str) -> io::Result<Option<String>> {
  match std::fs::read(path) {
    Ok(bytes) => {
      let pin: Pin = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
      if pin.identity != identity
        || pin.sha256.len() != 64
        || !pin.sha256.bytes().all(|b| b.is_ascii_hexdigit())
      {
        return Err(io::Error::other("证书信任记录损坏，拒绝重新建立首次信任"));
      }
      Ok(Some(pin.sha256))
    }
    Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
    Err(e) => Err(e),
  }
}
fn remember_pin(path: &Path, identity: &str, sha256: &str) -> io::Result<()> {
  static LOCK: Mutex<()> = Mutex::new(());
  let _guard = LOCK
    .lock()
    .map_err(|_| io::Error::other("证书信任存储锁不可用"))?;
  if let Some(expected) = read_pin(path, identity)? {
    if expected != sha256 {
      return Err(io::Error::other("服务端证书与首次信任不一致，已拒绝连接"));
    }
    return Ok(());
  }
  private_write(
    path,
    &serde_json::to_vec(&Pin {
      identity: identity.into(),
      sha256: sha256.into(),
    })
    .map_err(io::Error::other)?,
  )
}
/// Atomic, private application data. Never silently replace a corrupt trust record.
pub fn private_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
  use std::io::Write;
  static SEQUENCE: OnceLock<std::sync::atomic::AtomicU64> = OnceLock::new();
  let seq = SEQUENCE
    .get_or_init(Default::default)
    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  std::fs::create_dir_all(
    path
      .parent()
      .ok_or_else(|| io::Error::other("无效存储路径"))?,
  )?;
  let tmp = path.with_extension(format!("{}-{seq}.tmp", std::process::id()));
  let mut options = std::fs::OpenOptions::new();
  options.write(true).create_new(true);
  #[cfg(unix)]
  {
    use std::os::unix::fs::OpenOptionsExt;
    options.mode(0o600);
  }
  let result = (|| {
    let mut file = options.open(&tmp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&tmp, path)
  })();
  if result.is_err() {
    let _ = std::fs::remove_file(tmp);
  }
  result
}
#[derive(Debug)]
struct PinnedVerifier {
  expected: Option<String>,
  provider: Arc<rustls::crypto::CryptoProvider>,
}
impl ServerCertVerifier for PinnedVerifier {
  fn verify_server_cert(
    &self,
    end: &CertificateDer<'_>,
    _: &[CertificateDer<'_>],
    _: &ServerName<'_>,
    _: &[u8],
    _: UnixTime,
  ) -> Result<ServerCertVerified, rustls::Error> {
    if self
      .expected
      .as_ref()
      .is_some_and(|expected| expected != &fingerprint(end.as_ref()))
    {
      return Err(rustls::Error::General("服务端证书与首次信任不一致".into()));
    }
    Ok(ServerCertVerified::assertion())
  }
  fn verify_tls12_signature(
    &self,
    message: &[u8],
    cert: &CertificateDer<'_>,
    dss: &DigitallySignedStruct,
  ) -> Result<HandshakeSignatureValid, rustls::Error> {
    rustls::crypto::verify_tls12_signature(
      message,
      cert,
      dss,
      &self.provider.signature_verification_algorithms,
    )
  }
  fn verify_tls13_signature(
    &self,
    message: &[u8],
    cert: &CertificateDer<'_>,
    dss: &DigitallySignedStruct,
  ) -> Result<HandshakeSignatureValid, rustls::Error> {
    rustls::crypto::verify_tls13_signature(
      message,
      cert,
      dss,
      &self.provider.signature_verification_algorithms,
    )
  }
  fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
    self
      .provider
      .signature_verification_algorithms
      .supported_schemes()
  }
}
