use async_trait::async_trait;
use std::io::{BufRead, BufReader};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};
use vaultra_lib::askpass::{maybe_run_askpass_client, PromptHandler, PromptKind};
use vaultra_lib::error::VError;
use vaultra_lib::logging::Logger;
use vaultra_lib::protocol::ftp::client::{EncodingMode, FtpOptions, TransferMode};
use vaultra_lib::protocol::ftp::tls::TrustStore;
use vaultra_lib::protocol::ftp::FtpClient;
use vaultra_lib::protocol::sftp::{self, SshOptions};
use vaultra_lib::protocol::{EntryKind, Protocol, RemoteClient};

const SFTP_PORT: u16 = 2222;
const FTP_PORT: u16 = 2121;
const FTPS_PORT: u16 = 2990;
const PAYLOAD_SIZE: usize = 3 * 1024 * 1024;
const RESUME_OFFSET: u64 = 1024 * 1024 + 123;

type TestResult = Result<(), String>;

macro_rules! check {
    ($cond:expr, $($arg:tt)*) => {
        if !$cond {
            return Err(format!("{} (line {})", format!($($arg)*), line!()));
        }
    };
}

fn describe<T>(result: Result<T, VError>, context: &str) -> Result<T, String> {
  result.map_err(|e| format!("{context}: {e}"))
}

struct TestPrompts;

#[async_trait]
impl PromptHandler for TestPrompts {
  async fn on_prompt(&self, kind: PromptKind, text: String) -> Option<String> {
    eprintln!("[prompt {kind:?}] {}", text.replace('\n', " | "));
    match kind {
      PromptKind::HostKey | PromptKind::Confirm => Some("yes".into()),
      PromptKind::Info => Some(String::new()),
      PromptKind::Secret => {
        if text.to_ascii_lowercase().contains("code") {
          Some("123456".into())
        } else {
          Some("secret".into())
        }
      }
    }
  }
}

struct ServerProcess {
  child: Child,
}

impl Drop for ServerProcess {
  fn drop(&mut self) {
    let _ = self.child.kill();
    let _ = self.child.wait();
  }
}

fn spawn_python(script: &Path, args: &[String]) -> ServerProcess {
  let mut child = Command::new("python")
    .arg(script)
    .args(args)
    .stdout(Stdio::piped())
    .stderr(Stdio::inherit())
    .spawn()
    .expect("python must be available");
  let stdout = child.stdout.take().unwrap();
  let mut reader = BufReader::new(stdout);
  let mut line = String::new();
  let started = Instant::now();
  while !line.starts_with("READY") {
    line.clear();
    if reader.read_line(&mut line).unwrap_or(0) == 0 || started.elapsed() > Duration::from_secs(30)
    {
      panic!("server {} did not become ready", script.display());
    }
  }
  std::thread::spawn(move || {
    let mut sink = String::new();
    while reader.read_line(&mut sink).unwrap_or(0) > 0 {
      sink.clear();
    }
  });
  ServerProcess { child }
}

fn wait_for_port(port: u16) {
  let started = Instant::now();
  while TcpStream::connect(("127.0.0.1", port)).is_err() {
    if started.elapsed() > Duration::from_secs(20) {
      panic!("port {port} never opened");
    }
    std::thread::sleep(Duration::from_millis(200));
  }
}

fn payload() -> Vec<u8> {
  (0..PAYLOAD_SIZE)
    .map(|i| ((i * 7 + i / 251) % 251) as u8)
    .collect()
}

async fn exercise_client(client: &dyn RemoteClient, base: &str) -> TestResult {
  let dir = format!("{}/vaultra-it", base.trim_end_matches('/'));
  if let Ok(leftovers) = client.list(&dir).await {
    for entry in leftovers {
      let _ = client.remove(&entry.path).await;
    }
    let _ = client.rmdir(&dir).await;
  }
  describe(client.mkdir(&dir).await, "mkdir")?;
  let file = format!("{dir}/payload.bin");
  let data = payload();

  let mut writer = describe(client.open_write(&file, 0).await, "open_write")?;
  describe(writer.write(&data).await, "write")?;
  describe(writer.finish().await, "finish write")?;

  let stat = describe(client.stat(&file).await, "stat")?;
  check!(
    stat.size as usize == data.len(),
    "uploaded size {} != {}",
    stat.size,
    data.len()
  );
  check!(stat.kind == EntryKind::File, "stat kind is {:?}", stat.kind);

  let mut reader = describe(client.open_read(&file, RESUME_OFFSET).await, "open_read")?;
  let mut received = Vec::new();
  let mut buffer = vec![0u8; 64 * 1024];
  loop {
    let n = describe(reader.read(&mut buffer).await, "read")?;
    if n == 0 {
      break;
    }
    received.extend_from_slice(&buffer[..n]);
  }
  describe(reader.finish().await, "finish read")?;
  let expected = &data[RESUME_OFFSET as usize..];
  let first_diff = received
    .iter()
    .zip(expected.iter())
    .position(|(a, b)| a != b);
  check!(
    received == expected,
    "resumed read mismatch ({} bytes, first diff at {:?}: got {:?} expected {:?})",
    received.len(),
    first_diff,
    first_diff.map(|i| received[i..(i + 8).min(received.len())].to_vec()),
    first_diff.map(|i| expected[i..(i + 8).min(expected.len())].to_vec())
  );

  let mut appender = describe(
    client.open_write(&file, RESUME_OFFSET).await,
    "open_write resume",
  )?;
  describe(
    appender.write(&data[RESUME_OFFSET as usize..]).await,
    "resume write",
  )?;
  describe(appender.finish().await, "finish resume write")?;
  let after = describe(client.stat(&file).await, "stat after resume")?;
  check!(
    after.size as usize == data.len(),
    "size after resume {} != {}",
    after.size,
    data.len()
  );

  describe(client.touch(&format!("{dir}/empty.txt")).await, "touch")?;
  let listing = describe(client.list(&dir).await, "list")?;
  let names: Vec<&str> = listing.iter().map(|e| e.name.as_str()).collect();
  check!(
    names.contains(&"payload.bin") && names.contains(&"empty.txt"),
    "listing missing entries: {names:?}"
  );

  let renamed = format!("{dir}/renamed.bin");
  describe(client.rename(&file, &renamed).await, "rename")?;
  check!(
    client.stat(&file).await.is_err(),
    "old name still exists after rename"
  );
  describe(client.remove(&renamed).await, "remove")?;
  describe(
    client.remove(&format!("{dir}/empty.txt")).await,
    "remove empty",
  )?;
  describe(client.rmdir(&dir).await, "rmdir")?;
  check!(
    client.stat(&dir).await.is_err(),
    "directory still exists after rmdir"
  );
  Ok(())
}

async fn sftp_flow(user: &str, known_hosts: &Path) -> TestResult {
  let options = SshOptions {
    host: "127.0.0.1".into(),
    port: SFTP_PORT,
    user: Some(user.into()),
    identity_file: None,
    proxy_jump: None,
    extra_options: vec![
      format!("UserKnownHostsFile={}", known_hosts.display()),
      "PubkeyAuthentication=no".into(),
      "PreferredAuthentications=keyboard-interactive,password".into(),
    ],
    keepalive_secs: 0,
    connect_timeout_secs: 15,
    auto_accept_hostkey: false,
    disable_password: false,
  };
  let client = describe(
    sftp::connect(options, Logger::new(None, None), Arc::new(TestPrompts)).await,
    "sftp connect",
  )?;
  let home = describe(client.initial_dir().await, "initial_dir")?;
  check!(home.starts_with('/'), "home is not absolute: {home}");
  exercise_client(&client, &home).await?;
  client.disconnect().await;
  Ok(())
}

fn ftp_options(
  protocol: Protocol,
  host: &str,
  port: u16,
  mode: TransferMode,
  trust: Arc<TrustStore>,
) -> FtpOptions {
  FtpOptions {
    protocol,
    host: host.into(),
    port,
    user: "test".into(),
    password: "secret".into(),
    mode,
    ascii: false,
    encoding: EncodingMode::Auto,
    timeout_secs: 15,
    trust,
  }
}

async fn ftp_flow(mode: TransferMode) -> TestResult {
  let trust = TrustStore::load();
  let options = ftp_options(Protocol::Ftp, "127.0.0.1", FTP_PORT, mode, trust);
  let client = describe(
    FtpClient::connect(options, Logger::new(None, None)).await,
    "ftp connect",
  )?;
  let home = describe(client.initial_dir().await, "pwd")?;
  exercise_client(&client, &home).await?;
  client.disconnect().await;
  Ok(())
}

async fn ftps_flow() -> TestResult {
  let trust = TrustStore::load();
  let options = ftp_options(
    Protocol::FtpsExplicit,
    "localhost",
    FTPS_PORT,
    TransferMode::Passive,
    trust.clone(),
  );
  let first = FtpClient::connect(options.clone(), Logger::new(None, None)).await;
  let fingerprint = match first {
    Err(VError::UntrustedCertificate {
      fingerprint,
      subject,
      ..
    }) => {
      check!(
        subject.contains("Vaultra Test"),
        "unexpected certificate subject {subject}"
      );
      fingerprint
    }
    Err(other) => return Err(format!("expected untrusted certificate, got {other}")),
    Ok(_) => return Err("self-signed certificate was accepted without confirmation".into()),
  };
  trust.trust(&fingerprint, false);
  let client = describe(
    FtpClient::connect(options, Logger::new(None, None)).await,
    "ftps connect after trust",
  )?;
  let home = describe(client.initial_dir().await, "pwd")?;
  exercise_client(&client, &home).await?;
  client.disconnect().await;
  Ok(())
}

fn main() {
  if maybe_run_askpass_client() {
    return;
  }
  let _ = rustls::crypto::ring::default_provider().install_default();
  let scripts = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    .join("tests")
    .join("servers");
  let work = tempfile::tempdir().expect("temp dir");
  let sftp_root = work.path().join("sftp-root");
  let ftp_root = work.path().join("ftp-root");
  let known_hosts = work.path().join("known_hosts");

  let _sftp = spawn_python(
    &scripts.join("sftp_server.py"),
    &[sftp_root.to_string_lossy().into(), SFTP_PORT.to_string()],
  );
  let _ftp = spawn_python(
    &scripts.join("ftp_server.py"),
    &[
      ftp_root.to_string_lossy().into(),
      FTP_PORT.to_string(),
      FTPS_PORT.to_string(),
    ],
  );
  wait_for_port(SFTP_PORT);
  wait_for_port(FTP_PORT);
  wait_for_port(FTPS_PORT);

  let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
  let results: Vec<(&str, TestResult)> = runtime.block_on(async {
    vec![
      ("sftp password auth", sftp_flow("test", &known_hosts).await),
      (
        "sftp keyboard-interactive (totp)",
        sftp_flow("totp", &known_hosts).await,
      ),
      ("ftp passive", ftp_flow(TransferMode::Passive).await),
      ("ftp active", ftp_flow(TransferMode::Active).await),
      ("ftps explicit self-signed", ftps_flow().await),
    ]
  });

  let mut failed = false;
  for (name, result) in &results {
    match result {
      Ok(()) => println!("PASS  {name}"),
      Err(error) => {
        failed = true;
        println!("FAIL  {name}: {error}");
      }
    }
  }
  drop(runtime);
  if failed {
    std::process::exit(1);
  }
}
