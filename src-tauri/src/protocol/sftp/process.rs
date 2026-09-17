use super::client::SftpClient;
use crate::askpass::{AskpassServer, PromptHandler};
use crate::error::{VError, VResult};
use crate::logging::Logger;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

const MAX_STDERR_LINES: usize = 500;
const EXIT_GRACE_SECS: u64 = 3;

#[derive(Debug, Clone, Default)]
pub struct SshOptions {
  pub host: String,
  pub port: u16,
  pub user: Option<String>,
  pub identity_file: Option<String>,
  pub proxy_jump: Option<String>,
  pub extra_options: Vec<String>,
  pub keepalive_secs: u32,
  pub connect_timeout_secs: u32,
  pub auto_accept_hostkey: bool,
  pub disable_password: bool,
}

impl SshOptions {
  fn target(&self) -> String {
    match self.user.as_deref().filter(|u| !u.is_empty()) {
      Some(user) => format!("{user}@{}", self.host),
      None => self.host.clone(),
    }
  }

  fn apply_to(&self, cmd: &mut Command) {
    cmd
      .arg("-s")
      .arg("-p")
      .arg(self.port.to_string())
      .arg("-oBatchMode=no")
      .arg("-oNumberOfPasswordPrompts=3")
      .arg("-oLogLevel=INFO");
    let strictness = if self.auto_accept_hostkey {
      "accept-new"
    } else {
      "ask"
    };
    cmd.arg(format!("-oStrictHostKeyChecking={strictness}"));
    if self.keepalive_secs > 0 {
      cmd
        .arg(format!("-oServerAliveInterval={}", self.keepalive_secs))
        .arg("-oServerAliveCountMax=3");
    }
    if self.connect_timeout_secs > 0 {
      cmd.arg(format!("-oConnectTimeout={}", self.connect_timeout_secs));
    }
    if let Some(key) = self
      .identity_file
      .as_deref()
      .map(str::trim)
      .filter(|k| !k.is_empty())
    {
      cmd.arg("-i").arg(key).arg("-oIdentitiesOnly=yes");
    }
    if self.disable_password {
      cmd
        .arg("-oPasswordAuthentication=no")
        .arg("-oKbdInteractiveAuthentication=no");
    }
    if let Some(jump) = self
      .proxy_jump
      .as_deref()
      .map(str::trim)
      .filter(|j| !j.is_empty())
    {
      cmd.arg("-J").arg(jump);
    }
    for option in &self.extra_options {
      let option = option.trim().trim_start_matches("-o").trim();
      if !option.is_empty() {
        cmd.arg(format!("-o{option}"));
      }
    }
    cmd.arg(self.target()).arg("sftp");
  }
}

pub fn find_ssh() -> Option<PathBuf> {
  if let Ok(root) = std::env::var("SystemRoot") {
    let candidate = PathBuf::from(root)
      .join("System32")
      .join("OpenSSH")
      .join("ssh.exe");
    if candidate.exists() {
      return Some(candidate);
    }
  }
  let path = std::env::var_os("PATH")?;
  std::env::split_paths(&path)
    .map(|dir| dir.join("ssh.exe"))
    .find(|candidate| candidate.exists())
}

pub fn find_ssh_tool(name: &str) -> Option<PathBuf> {
  find_ssh().map(|ssh| ssh.with_file_name(name))
}

pub async fn ssh_version() -> Option<String> {
  let ssh = find_ssh()?;
  let output = hidden_command(ssh).arg("-V").output().await.ok()?;
  let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
  let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
  Some(if stderr.is_empty() { stdout } else { stderr })
}

fn hidden_command(program: PathBuf) -> Command {
  let mut cmd = Command::new(program);
  #[cfg(windows)]
  {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
  }
  cmd
}

pub async fn forget_host_key(host: &str, port: u16) -> VResult<()> {
  let keygen = find_ssh_tool("ssh-keygen.exe").ok_or(VError::OpenSshMissing)?;
  let target = if port == 22 {
    host.to_string()
  } else {
    format!("[{host}]:{port}")
  };
  let output = hidden_command(keygen)
    .arg("-R")
    .arg(&target)
    .output()
    .await?;
  if !output.status.success() {
    return Err(VError::Other(format!(
      "ssh-keygen -R failed: {}",
      String::from_utf8_lossy(&output.stderr)
    )));
  }
  Ok(())
}

type SharedChild = Arc<parking_lot::Mutex<Option<Child>>>;
type StderrLines = Arc<parking_lot::Mutex<Vec<String>>>;

pub async fn connect(
  opts: SshOptions,
  log: Arc<Logger>,
  prompts: Arc<dyn PromptHandler>,
) -> VResult<SftpClient> {
  let ssh = find_ssh().ok_or(VError::OpenSshMissing)?;
  let askpass = AskpassServer::start(prompts)?;
  let self_exe = std::env::current_exe().map_err(|e| VError::Io(e.to_string()))?;

  let mut cmd = hidden_command(ssh);
  opts.apply_to(&mut cmd);
  cmd
    .env("SSH_ASKPASS", &self_exe)
    .env("SSH_ASKPASS_REQUIRE", "force")
    .env("DISPLAY", "vaultra:0")
    .env(crate::askpass::ENV_PIPE, &askpass.pipe_name)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .kill_on_drop(true);

  log.status(format!(
    "Starting OpenSSH: ssh -s -p {} {} sftp",
    opts.port,
    opts.target()
  ));
  let mut child = cmd
    .spawn()
    .map_err(|e| VError::Io(format!("failed to start ssh.exe: {e}")))?;
  let stdin = child
    .stdin
    .take()
    .ok_or_else(|| VError::Io("ssh stdin unavailable".into()))?;
  let stdout = child
    .stdout
    .take()
    .ok_or_else(|| VError::Io("ssh stdout unavailable".into()))?;
  let stderr = child
    .stderr
    .take()
    .ok_or_else(|| VError::Io("ssh stderr unavailable".into()))?;

  let stderr_lines: StderrLines = Arc::new(parking_lot::Mutex::new(Vec::new()));
  tokio::spawn(relay_stderr(stderr, stderr_lines.clone(), log.clone()));

  let child: SharedChild = Arc::new(parking_lot::Mutex::new(Some(child)));
  let on_close = {
    let child = child.clone();
    let askpass = Arc::new(parking_lot::Mutex::new(Some(askpass)));
    Box::new(move || {
      if let Some(mut process) = child.lock().take() {
        let _ = process.start_kill();
      }
      askpass.lock().take();
    })
  };

  match SftpClient::handshake(stdout, stdin, log.clone(), on_close).await {
    Ok(client) => {
      log.status("Connected via SFTP");
      Ok(client)
    }
    Err(error) => {
      let taken = child.lock().take();
      if let Some(mut process) = taken {
        let _ = tokio::time::timeout(
          std::time::Duration::from_secs(EXIT_GRACE_SECS),
          process.wait(),
        )
        .await;
        let _ = process.start_kill();
      }
      tokio::time::sleep(std::time::Duration::from_millis(150)).await;
      let lines = stderr_lines.lock().clone();
      Err(SshFailure::new(&opts, &lines).into_error(error))
    }
  }
}

async fn relay_stderr(stderr: tokio::process::ChildStderr, sink: StderrLines, log: Arc<Logger>) {
  let mut lines = BufReader::new(stderr).lines();
  while let Ok(Some(line)) = lines.next_line().await {
    let line = line.trim().to_string();
    if line.is_empty() {
      continue;
    }
    let lower = line.to_ascii_lowercase();
    let is_problem = ["warning", "error", "denied", "failed"]
      .iter()
      .any(|w| lower.contains(w));
    if is_problem {
      log.error(format!("ssh: {line}"));
    } else {
      log.trace(format!("ssh: {line}"));
    }
    let mut buffer = sink.lock();
    if buffer.len() < MAX_STDERR_LINES {
      buffer.push(line);
    }
  }
}

struct SshFailure<'a> {
  opts: &'a SshOptions,
  lines: &'a [String],
  joined: String,
}

impl<'a> SshFailure<'a> {
  fn new(opts: &'a SshOptions, lines: &'a [String]) -> Self {
    Self {
      opts,
      lines,
      joined: lines.join("\n"),
    }
  }

  fn contains(&self, needle: &str) -> bool {
    self.joined.contains(needle)
  }

  fn capture(&self, pattern: &str) -> String {
    regex::Regex::new(pattern)
      .ok()
      .and_then(|re| re.captures(&self.joined))
      .map(|c| c[1].to_string())
      .unwrap_or_default()
  }

  fn into_error(self, fallback: VError) -> VError {
    if self.contains("REMOTE HOST IDENTIFICATION HAS CHANGED") {
      return VError::HostKeyChanged {
        host: format!("{}:{}", self.opts.host, self.opts.port),
        fingerprint: self.capture(r"(SHA256:[A-Za-z0-9+/=]+)"),
        key_type: self.capture(r"fingerprint for the (\w+) key"),
        known_hosts_line: self.capture(r"Offending .*key in (.+:\d+)"),
      };
    }
    if self.contains("Host key verification failed") {
      return VError::AuthFailed("host key rejected".into());
    }
    if self.contains("Permission denied") {
      return VError::AuthFailed("permission denied (wrong password or key not accepted)".into());
    }
    if self.contains("Too many authentication failures") {
      return VError::AuthFailed("too many authentication failures".into());
    }
    if self.contains("Could not resolve hostname") {
      return VError::ConnectionLost(format!("could not resolve host {}", self.opts.host));
    }
    if self.contains("Connection refused") {
      return VError::ConnectionLost("connection refused".into());
    }
    if self.contains("timed out") {
      return VError::Timeout("connection timed out".into());
    }
    if self.contains("subsystem request failed") {
      return VError::Unsupported("server does not offer the sftp subsystem".into());
    }
    match self.lines.iter().rev().find(|l| !l.starts_with("debug")) {
      Some(last) => VError::Protocol(format!("{fallback} (ssh: {last})")),
      None => fallback,
    }
  }
}
