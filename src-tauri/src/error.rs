use serde::Serialize;

#[derive(Debug, thiserror::Error, Clone, Serialize)]
#[serde(
  tag = "code",
  content = "detail",
  rename_all = "camelCase",
  rename_all_fields = "camelCase"
)]
pub enum VError {
  #[error("{0}")]
  Io(String),
  #[error("{0}")]
  Protocol(String),
  #[error("Connection lost: {0}")]
  ConnectionLost(String),
  #[error("Authentication failed: {0}")]
  AuthFailed(String),
  #[error("Not found: {0}")]
  NotFound(String),
  #[error("Permission denied: {0}")]
  PermissionDenied(String),
  #[error("Unsupported operation: {0}")]
  Unsupported(String),
  #[error("Cancelled")]
  Cancelled,
  #[error("Timed out: {0}")]
  Timeout(String),
  #[error("Host key changed")]
  HostKeyChanged {
    host: String,
    fingerprint: String,
    key_type: String,
    known_hosts_line: String,
  },
  #[error("Untrusted TLS certificate")]
  UntrustedCertificate {
    host: String,
    fingerprint: String,
    subject: String,
    issuer: String,
    not_before: String,
    not_after: String,
    reason: String,
  },
  #[error("OpenSSH client not found")]
  OpenSshMissing,
  #[error("Invalid session")]
  InvalidSession,
  #[error("{0}")]
  Other(String),
}

pub type VResult<T> = Result<T, VError>;

impl From<std::io::Error> for VError {
  fn from(e: std::io::Error) -> Self {
    match e.kind() {
      std::io::ErrorKind::NotFound => VError::NotFound(e.to_string()),
      std::io::ErrorKind::PermissionDenied => VError::PermissionDenied(e.to_string()),
      std::io::ErrorKind::TimedOut => VError::Timeout(e.to_string()),
      std::io::ErrorKind::ConnectionReset
      | std::io::ErrorKind::ConnectionAborted
      | std::io::ErrorKind::BrokenPipe
      | std::io::ErrorKind::UnexpectedEof => VError::ConnectionLost(e.to_string()),
      _ => VError::Io(e.to_string()),
    }
  }
}

impl From<anyhow::Error> for VError {
  fn from(e: anyhow::Error) -> Self {
    VError::Other(format!("{e:#}"))
  }
}

impl From<tokio::time::error::Elapsed> for VError {
  fn from(e: tokio::time::error::Elapsed) -> Self {
    VError::Timeout(e.to_string())
  }
}

impl From<serde_json::Error> for VError {
  fn from(e: serde_json::Error) -> Self {
    VError::Other(e.to_string())
  }
}
