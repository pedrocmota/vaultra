use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

const MAX_LOG_FILES: usize = 7;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LogKind {
  Status,
  Command,
  Response,
  Error,
  Trace,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogMessage {
  pub session_id: Option<String>,
  pub kind: LogKind,
  pub text: String,
  pub ts: i64,
}

#[derive(Clone)]
pub struct Logger {
  app: Option<AppHandle>,
  session_id: Option<String>,
}

impl Logger {
  pub fn new(app: Option<AppHandle>, session_id: Option<String>) -> Arc<Self> {
    Arc::new(Self { app, session_id })
  }

  pub fn log(&self, kind: LogKind, text: impl Into<String>) {
    let text = text.into();
    match kind {
      LogKind::Error => tracing::error!(session = ?self.session_id, "{text}"),
      LogKind::Trace => tracing::debug!(session = ?self.session_id, "{text}"),
      _ => tracing::info!(session = ?self.session_id, "{text}"),
    }
    if let Some(app) = &self.app {
      let _ = app.emit(
        "log",
        LogMessage {
          session_id: self.session_id.clone(),
          kind,
          text,
          ts: chrono::Utc::now().timestamp_millis(),
        },
      );
    }
  }

  pub fn status(&self, text: impl Into<String>) {
    self.log(LogKind::Status, text)
  }

  pub fn command(&self, text: impl Into<String>) {
    self.log(LogKind::Command, text)
  }

  pub fn response(&self, text: impl Into<String>) {
    self.log(LogKind::Response, text)
  }

  pub fn error(&self, text: impl Into<String>) {
    self.log(LogKind::Error, text)
  }

  pub fn trace(&self, text: impl Into<String>) {
    self.log(LogKind::Trace, text)
  }
}

pub fn log_dir() -> PathBuf {
  crate::paths::data_dir().join("logs")
}

pub fn init_file_logging() -> Option<tracing_appender::non_blocking::WorkerGuard> {
  let dir = log_dir();
  std::fs::create_dir_all(&dir).ok()?;
  prune_old_logs(&dir);
  let appender = tracing_appender::rolling::daily(&dir, "vaultra.log");
  let (writer, guard) = tracing_appender::non_blocking(appender);
  use tracing_subscriber::prelude::*;
  let filter = tracing_subscriber::EnvFilter::try_from_env("VAULTRA_LOG")
    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,vaultra_lib=debug"));
  let _ = tracing_subscriber::registry()
    .with(filter)
    .with(
      tracing_subscriber::fmt::layer()
        .with_writer(writer)
        .with_ansi(false),
    )
    .try_init();
  Some(guard)
}

fn prune_old_logs(dir: &PathBuf) {
  let Ok(entries) = std::fs::read_dir(dir) else {
    return;
  };
  let mut files: Vec<PathBuf> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
  files.sort();
  while files.len() > MAX_LOG_FILES {
    let oldest = files.remove(0);
    let _ = std::fs::remove_file(oldest);
  }
}
