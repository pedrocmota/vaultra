use crate::error::{VError, VResult};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::client::WebPkiServerVerifier;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::sync::Arc;

const SESSION_CACHE_SIZE: usize = 32;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertInfo {
  pub host: String,
  pub fingerprint: String,
  pub subject: String,
  pub issuer: String,
  pub not_before: String,
  pub not_after: String,
  pub reason: String,
}

impl CertInfo {
  fn describe(host: &str, der: &[u8], reason: String) -> Self {
    let fingerprint = fingerprint_sha256(der);
    let parsed = x509_parser::parse_x509_certificate(der).ok();
    let (subject, issuer, not_before, not_after) = match parsed {
      Some((_, cert)) => (
        cert.subject().to_string(),
        cert.issuer().to_string(),
        cert.validity().not_before.to_rfc2822().unwrap_or_default(),
        cert.validity().not_after.to_rfc2822().unwrap_or_default(),
      ),
      None => Default::default(),
    };
    Self {
      host: host.to_string(),
      fingerprint,
      subject,
      issuer,
      not_before,
      not_after,
      reason,
    }
  }

  pub fn into_error(self) -> VError {
    VError::UntrustedCertificate {
      host: self.host,
      fingerprint: self.fingerprint,
      subject: self.subject,
      issuer: self.issuer,
      not_before: self.not_before,
      not_after: self.not_after,
      reason: self.reason,
    }
  }
}

#[derive(Default)]
pub struct TrustStore {
  persistent: parking_lot::Mutex<HashSet<String>>,
  session: parking_lot::Mutex<HashSet<String>>,
}

impl std::fmt::Debug for TrustStore {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str("TrustStore")
  }
}

impl TrustStore {
  pub fn load() -> Arc<Self> {
    let store = Self::default();
    if let Ok(text) = std::fs::read_to_string(crate::paths::trusted_certs_file()) {
      if let Ok(list) = serde_json::from_str::<Vec<String>>(&text) {
        *store.persistent.lock() = list.into_iter().collect();
      }
    }
    Arc::new(store)
  }

  pub fn is_trusted(&self, fingerprint: &str) -> bool {
    self.persistent.lock().contains(fingerprint) || self.session.lock().contains(fingerprint)
  }

  pub fn trust(&self, fingerprint: &str, remember: bool) {
    if !remember {
      self.session.lock().insert(fingerprint.to_string());
      return;
    }
    let list: Vec<String> = {
      let mut persistent = self.persistent.lock();
      persistent.insert(fingerprint.to_string());
      persistent.iter().cloned().collect()
    };
    if let Ok(text) = serde_json::to_string_pretty(&list) {
      let _ = std::fs::write(crate::paths::trusted_certs_file(), text);
    }
  }
}

pub fn fingerprint_sha256(der: &[u8]) -> String {
  hex::encode(Sha256::digest(der))
}

#[derive(Debug)]
pub struct VaultraVerifier {
  inner: Arc<WebPkiServerVerifier>,
  trust: Arc<TrustStore>,
  host: String,
  pending: parking_lot::Mutex<Option<CertInfo>>,
}

impl VaultraVerifier {
  pub fn new(host: &str, trust: Arc<TrustStore>) -> VResult<Arc<Self>> {
    let mut roots = rustls::RootCertStore::empty();
    for cert in rustls_native_certs::load_native_certs().certs {
      let _ = roots.add(cert);
    }
    if roots.is_empty() {
      roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    }
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let inner = WebPkiServerVerifier::builder_with_provider(Arc::new(roots), provider)
      .build()
      .map_err(|e| VError::Other(format!("failed to build TLS verifier: {e}")))?;
    Ok(Arc::new(Self {
      inner,
      trust,
      host: host.to_string(),
      pending: parking_lot::Mutex::new(None),
    }))
  }

  pub fn take_pending(&self) -> Option<CertInfo> {
    self.pending.lock().take()
  }
}

impl ServerCertVerifier for VaultraVerifier {
  fn verify_server_cert(
    &self,
    end_entity: &CertificateDer<'_>,
    intermediates: &[CertificateDer<'_>],
    server_name: &ServerName<'_>,
    ocsp_response: &[u8],
    now: UnixTime,
  ) -> Result<ServerCertVerified, rustls::Error> {
    match self
      .inner
      .verify_server_cert(end_entity, intermediates, server_name, ocsp_response, now)
    {
      Ok(verified) => Ok(verified),
      Err(error) => {
        if self
          .trust
          .is_trusted(&fingerprint_sha256(end_entity.as_ref()))
        {
          return Ok(ServerCertVerified::assertion());
        }
        *self.pending.lock() = Some(CertInfo::describe(
          &self.host,
          end_entity.as_ref(),
          error.to_string(),
        ));
        Err(error)
      }
    }
  }

  fn verify_tls12_signature(
    &self,
    message: &[u8],
    cert: &CertificateDer<'_>,
    dss: &DigitallySignedStruct,
  ) -> Result<HandshakeSignatureValid, rustls::Error> {
    self.inner.verify_tls12_signature(message, cert, dss)
  }

  fn verify_tls13_signature(
    &self,
    message: &[u8],
    cert: &CertificateDer<'_>,
    dss: &DigitallySignedStruct,
  ) -> Result<HandshakeSignatureValid, rustls::Error> {
    self.inner.verify_tls13_signature(message, cert, dss)
  }

  fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
    self.inner.supported_verify_schemes()
  }
}

pub fn client_config(verifier: Arc<VaultraVerifier>) -> Arc<rustls::ClientConfig> {
  let provider = Arc::new(rustls::crypto::ring::default_provider());
  let mut config = rustls::ClientConfig::builder_with_provider(provider)
    .with_safe_default_protocol_versions()
    .expect("default TLS protocol versions")
    .dangerous()
    .with_custom_certificate_verifier(verifier)
    .with_no_client_auth();
  config.resumption = rustls::client::Resumption::in_memory_sessions(SESSION_CACHE_SIZE);
  Arc::new(config)
}
