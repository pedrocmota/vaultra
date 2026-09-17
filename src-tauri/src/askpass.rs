use crate::error::{VError, VResult};
use async_trait::async_trait;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::oneshot;

pub const ENV_PIPE: &str = "VAULTRA_ASKPASS_PIPE";
const CANCEL_TOKEN: &str = "!cancel";
const CLIENT_RETRIES: usize = 50;
const PROMPT_TIMEOUT_SECS: u64 = 600;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptKind {
  Secret,
  Confirm,
  HostKey,
  Info,
}

#[async_trait]
pub trait PromptHandler: Send + Sync {
  async fn on_prompt(&self, kind: PromptKind, text: String) -> Option<String>;
}

fn b64() -> base64::engine::GeneralPurpose {
  base64::engine::general_purpose::STANDARD
}

pub fn maybe_run_askpass_client() -> bool {
  let pipe = match std::env::var(ENV_PIPE) {
    Ok(p) if !p.is_empty() => p,
    _ => return false,
  };
  let prompt: String = std::env::args().skip(1).collect::<Vec<_>>().join(" ");
  let kind = std::env::var("SSH_ASKPASS_PROMPT").unwrap_or_default();
  let code = match askpass_client(&pipe, &kind, &prompt) {
    Ok(Some(answer)) => {
      use std::io::Write;
      let mut out = std::io::stdout().lock();
      let _ = out.write_all(answer.as_bytes());
      let _ = out.write_all(b"\n");
      let _ = out.flush();
      0
    }
    _ => 1,
  };
  std::process::exit(code);
}

fn askpass_client(pipe: &str, kind: &str, prompt: &str) -> std::io::Result<Option<String>> {
  use std::io::{BufRead, Write};
  let mut last_error = None;
  for _ in 0..CLIENT_RETRIES {
    match std::fs::OpenOptions::new()
      .read(true)
      .write(true)
      .open(pipe)
    {
      Ok(mut file) => {
        let encoded = b64().encode(prompt.as_bytes());
        file.write_all(format!("{kind}\n{encoded}\n").as_bytes())?;
        file.flush()?;
        let mut reader = std::io::BufReader::new(file);
        let mut line = String::new();
        reader.read_line(&mut line)?;
        let line = line.trim_end_matches(['\r', '\n']);
        if line == CANCEL_TOKEN {
          return Ok(None);
        }
        let decoded = b64()
          .decode(line)
          .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        return Ok(Some(String::from_utf8_lossy(&decoded).into_owned()));
      }
      Err(e) => {
        last_error = Some(e);
        std::thread::sleep(std::time::Duration::from_millis(100));
      }
    }
  }
  Err(last_error.unwrap_or_else(|| std::io::Error::other("askpass pipe unavailable")))
}

pub struct AskpassServer {
  pub pipe_name: String,
  stop: Option<oneshot::Sender<()>>,
}

impl AskpassServer {
  pub fn start(handler: Arc<dyn PromptHandler>) -> VResult<Self> {
    let pipe_name = format!(
      r"\\.\pipe\vaultra-askpass-{}",
      uuid::Uuid::new_v4().simple()
    );
    let (stop_tx, stop_rx) = oneshot::channel::<()>();
    #[cfg(windows)]
    spawn_pipe_server(pipe_name.clone(), handler, stop_rx)?;
    #[cfg(not(windows))]
    {
      let _ = (handler, stop_rx);
    }
    Ok(Self {
      pipe_name,
      stop: Some(stop_tx),
    })
  }
}

impl Drop for AskpassServer {
  fn drop(&mut self) {
    if let Some(stop) = self.stop.take() {
      let _ = stop.send(());
    }
  }
}

#[cfg(windows)]
fn spawn_pipe_server(
  name: String,
  handler: Arc<dyn PromptHandler>,
  mut stop_rx: oneshot::Receiver<()>,
) -> VResult<()> {
  use tokio::net::windows::named_pipe::ServerOptions;
  let mut server = ServerOptions::new()
    .first_pipe_instance(true)
    .create(&name)
    .map_err(|e| VError::Io(format!("failed to create askpass pipe: {e}")))?;
  tokio::spawn(async move {
    loop {
      tokio::select! {
          _ = &mut stop_rx => break,
          accepted = server.connect() => {
              if accepted.is_err() {
                  break;
              }
              let connected = server;
              server = match ServerOptions::new().create(&name) {
                  Ok(next) => next,
                  Err(_) => break,
              };
              let handler = handler.clone();
              tokio::spawn(async move {
                  let _ = serve_prompt(connected, handler).await;
              });
          }
      }
    }
  });
  Ok(())
}

#[cfg(windows)]
async fn serve_prompt(
  pipe: tokio::net::windows::named_pipe::NamedPipeServer,
  handler: Arc<dyn PromptHandler>,
) -> VResult<()> {
  let (reader, mut writer) = tokio::io::split(pipe);
  let mut reader = BufReader::new(reader);
  let mut kind_line = String::new();
  let mut prompt_line = String::new();
  reader.read_line(&mut kind_line).await?;
  reader.read_line(&mut prompt_line).await?;
  let prompt = b64()
    .decode(prompt_line.trim())
    .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
    .unwrap_or_default();
  let kind = classify_prompt(kind_line.trim(), &prompt);
  let reply = match handler.on_prompt(kind, prompt).await {
    Some(answer) => b64().encode(answer.as_bytes()),
    None => CANCEL_TOKEN.to_string(),
  };
  writer.write_all(format!("{reply}\n").as_bytes()).await?;
  writer.flush().await?;
  Ok(())
}

pub fn classify_prompt(ssh_kind: &str, text: &str) -> PromptKind {
  let lower = text.to_ascii_lowercase();
  if lower.contains("(yes/no") {
    if lower.contains("authenticity of host") || lower.contains("fingerprint") {
      return PromptKind::HostKey;
    }
    return PromptKind::Confirm;
  }
  match ssh_kind {
    "confirm" => PromptKind::Confirm,
    "none" => PromptKind::Info,
    _ => PromptKind::Secret,
  }
}

#[derive(Debug, Clone, Default)]
pub struct HostKeyPrompt {
  pub host: String,
  pub key_type: String,
  pub fingerprint: String,
}

pub fn parse_hostkey_prompt(text: &str) -> HostKeyPrompt {
  let host_re = regex::Regex::new(r"authenticity of host '([^']+)'").unwrap();
  let fingerprint_re =
    regex::Regex::new(r"(\w+) key fingerprint is (SHA256:[A-Za-z0-9+/=]+|MD5:[0-9a-f:]+)").unwrap();
  let host = host_re
    .captures(text)
    .map(|c| c[1].to_string())
    .unwrap_or_default();
  let (key_type, fingerprint) = fingerprint_re
    .captures(text)
    .map(|c| (c[1].to_string(), c[2].to_string()))
    .unwrap_or_default();
  HostKeyPrompt {
    host,
    key_type,
    fingerprint,
  }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UiPrompt {
  pub prompt_id: String,
  pub session_id: String,
  pub kind: PromptKind,
  pub text: String,
  pub host: Option<String>,
  pub key_type: Option<String>,
  pub fingerprint: Option<String>,
}

#[derive(Default)]
pub struct PromptBroker {
  pending: parking_lot::Mutex<HashMap<String, oneshot::Sender<Option<String>>>>,
}

impl PromptBroker {
  pub async fn ask(
    &self,
    app: &AppHandle,
    session_id: &str,
    kind: PromptKind,
    text: String,
  ) -> Option<String> {
    let prompt_id = uuid::Uuid::new_v4().to_string();
    let (tx, rx) = oneshot::channel();
    self.pending.lock().insert(prompt_id.clone(), tx);
    let details = (kind == PromptKind::HostKey).then(|| parse_hostkey_prompt(&text));
    let _ = app.emit(
      "auth-prompt",
      UiPrompt {
        prompt_id: prompt_id.clone(),
        session_id: session_id.to_string(),
        kind,
        text,
        host: details.as_ref().map(|d| d.host.clone()),
        key_type: details.as_ref().map(|d| d.key_type.clone()),
        fingerprint: details.as_ref().map(|d| d.fingerprint.clone()),
      },
    );
    match tokio::time::timeout(std::time::Duration::from_secs(PROMPT_TIMEOUT_SECS), rx).await {
      Ok(Ok(answer)) => answer,
      _ => {
        self.pending.lock().remove(&prompt_id);
        None
      }
    }
  }

  pub fn answer(&self, prompt_id: &str, answer: Option<String>) -> bool {
    match self.pending.lock().remove(prompt_id) {
      Some(tx) => tx.send(answer).is_ok(),
      None => false,
    }
  }
}
