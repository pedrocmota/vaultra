use super::parser::{parse_list_line, parse_mlsd_line, parse_mlsd_time};
use super::tls::{client_config, TrustStore, VaultraVerifier};
use crate::error::{VError, VResult};
use crate::logging::Logger;
use crate::protocol::{
  basename, parent, EntryKind, Protocol, RemoteClient, RemoteEntry, RemoteReader, RemoteWriter,
};
use async_trait::async_trait;
use rustls::pki_types::ServerName;
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{
  AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader, ReadHalf,
  WriteHalf,
};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, OwnedMutexGuard};
use tokio_rustls::TlsConnector;

const MIN_TIMEOUT_SECS: u64 = 5;
const ANONYMOUS_USER: &str = "anonymous";
const ANONYMOUS_PASSWORD: &str = "vaultra@";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TransferMode {
  #[default]
  Default,
  Passive,
  Active,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case", tag = "mode", content = "label")]
pub enum EncodingMode {
  #[default]
  Auto,
  Utf8,
  Custom(String),
}

impl EncodingMode {
  fn encoder(&self) -> Option<&'static encoding_rs::Encoding> {
    match self {
      EncodingMode::Custom(label) => encoding_rs::Encoding::for_label(label.as_bytes()),
      _ => None,
    }
  }

  fn encode(&self, text: &str) -> Vec<u8> {
    match self.encoder() {
      Some(encoding) => encoding.encode(text).0.into_owned(),
      None => text.as_bytes().to_vec(),
    }
  }

  fn decode(&self, bytes: &[u8]) -> String {
    match self {
      EncodingMode::Utf8 => String::from_utf8_lossy(bytes).into_owned(),
      EncodingMode::Auto => match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => encoding_rs::WINDOWS_1252.decode(bytes).0.into_owned(),
      },
      EncodingMode::Custom(_) => match self.encoder() {
        Some(encoding) => encoding.decode(bytes).0.into_owned(),
        None => String::from_utf8_lossy(bytes).into_owned(),
      },
    }
  }
}

#[derive(Clone)]
pub struct FtpOptions {
  pub protocol: Protocol,
  pub host: String,
  pub port: u16,
  pub user: String,
  pub password: String,
  pub mode: TransferMode,
  pub ascii: bool,
  pub encoding: EncodingMode,
  pub timeout_secs: u64,
  pub trust: Arc<TrustStore>,
}

impl FtpOptions {
  fn timeout(&self) -> Duration {
    Duration::from_secs(self.timeout_secs.max(MIN_TIMEOUT_SECS))
  }
}

pub trait Rw: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Rw for T {}

#[derive(Debug, Clone)]
pub struct Reply {
  pub code: u16,
  pub text: String,
}

impl Reply {
  fn is_positive(&self) -> bool {
    self.code < 400
  }

  fn is_transfer_start(&self) -> bool {
    matches!(self.code, 125 | 150)
  }

  fn into_error(self) -> VError {
    let lower = self.text.to_ascii_lowercase();
    let denied = ["permission", "denied", "not allowed"]
      .iter()
      .any(|w| lower.contains(w));
    match self.code {
      530 | 332 => VError::AuthFailed(self.text),
      550 | 553 if denied => VError::PermissionDenied(self.text),
      550 => VError::NotFound(self.text),
      421 => VError::ConnectionLost(self.text),
      500 | 502 => VError::Unsupported(self.text),
      code => VError::Protocol(format!("{code} {}", self.text)),
    }
  }

  fn ok(self) -> VResult<Reply> {
    if self.is_positive() {
      Ok(self)
    } else {
      Err(self.into_error())
    }
  }
}

struct Conn {
  reader: BufReader<ReadHalf<Box<dyn Rw>>>,
  writer: WriteHalf<Box<dyn Rw>>,
  peer: SocketAddr,
  local: SocketAddr,
}

impl Conn {
  fn new(stream: Box<dyn Rw>, peer: SocketAddr, local: SocketAddr) -> Self {
    let (reader, writer) = tokio::io::split(stream);
    Self {
      reader: BufReader::new(reader),
      writer,
      peer,
      local,
    }
  }

  fn into_stream(self) -> Box<dyn Rw> {
    self.reader.into_inner().unsplit(self.writer)
  }
}

#[derive(Default, Debug, Clone)]
struct Features {
  mlsd: bool,
  utf8: bool,
  epsv: bool,
  eprt: bool,
  size: bool,
  mdtm: bool,
  hash: Vec<String>,
  xhash: Vec<String>,
}

impl Features {
  fn parse(feat_reply: &str) -> Self {
    let mut features = Features::default();
    for line in feat_reply.lines() {
      let upper = line.trim().to_ascii_uppercase();
      match upper.as_str() {
        u if u.starts_with("MLSD") || u.starts_with("MLST") => features.mlsd = true,
        "UTF8" => features.utf8 = true,
        "EPSV" => features.epsv = true,
        "EPRT" => features.eprt = true,
        "SIZE" => features.size = true,
        "MDTM" => features.mdtm = true,
        "XSHA256" | "XSHA1" | "XMD5" | "XSHA512" => features.xhash.push(upper.clone()),
        u if u.starts_with("HASH") => {
          features.hash = u[4..]
            .split(';')
            .map(|s| s.trim().trim_end_matches('*').to_string())
            .filter(|s| !s.is_empty())
            .collect();
        }
        _ => {}
      }
    }
    features
  }
}

struct ControlChannel {
  log: Arc<Logger>,
  reply_timeout: Duration,
  encoding: parking_lot::Mutex<EncodingMode>,
}

impl ControlChannel {
  fn encode(&self, text: &str) -> Vec<u8> {
    self.encoding.lock().encode(text)
  }

  fn decode(&self, bytes: &[u8]) -> String {
    self.encoding.lock().decode(bytes)
  }

  async fn read_line(&self, conn: &mut Conn) -> VResult<String> {
    let mut buf = Vec::new();
    let n = tokio::time::timeout(self.reply_timeout, conn.reader.read_until(b'\n', &mut buf))
      .await
      .map_err(|_| VError::Timeout("server did not reply".into()))??;
    if n == 0 {
      return Err(VError::ConnectionLost("connection closed by server".into()));
    }
    let line = self.decode(&buf).trim_end_matches(['\r', '\n']).to_string();
    self.log.response(line.clone());
    Ok(line)
  }

  async fn read_reply(&self, conn: &mut Conn) -> VResult<Reply> {
    let first = self.read_line(conn).await?;
    let code = parse_reply_code(&first)
      .ok_or_else(|| VError::Protocol(format!("invalid reply: {first}")))?;
    if first.as_bytes().get(3) != Some(&b'-') {
      return Ok(Reply {
        code,
        text: first.get(4..).unwrap_or("").to_string(),
      });
    }
    let mut lines = vec![first[4..].to_string()];
    loop {
      let line = self.read_line(conn).await?;
      let terminates =
        parse_reply_code(&line) == Some(code) && line.as_bytes().get(3) == Some(&b' ');
      if terminates {
        lines.push(line[4..].to_string());
        return Ok(Reply {
          code,
          text: lines.join("\n"),
        });
      }
      lines.push(line);
    }
  }

  async fn send(&self, conn: &mut Conn, command: &str, shown: &str) -> VResult<Reply> {
    self.log.command(shown.to_string());
    let mut bytes = self.encode(command);
    bytes.extend_from_slice(b"\r\n");
    conn.writer.write_all(&bytes).await?;
    conn.writer.flush().await?;
    self.read_reply(conn).await
  }

  async fn command(&self, conn: &mut Conn, command: &str) -> VResult<Reply> {
    self.send(conn, command, command).await
  }

  async fn expect_ok(&self, conn: &mut Conn, command: &str) -> VResult<Reply> {
    self.command(conn, command).await?.ok()
  }
}

fn parse_reply_code(line: &str) -> Option<u16> {
  let bytes = line.as_bytes();
  if bytes.len() >= 3 && bytes[..3].iter().all(u8::is_ascii_digit) {
    line[..3].parse().ok()
  } else {
    None
  }
}

struct TlsContext {
  config: Arc<rustls::ClientConfig>,
  verifier: Arc<VaultraVerifier>,
  host: String,
}

impl TlsContext {
  fn new(host: &str, trust: Arc<TrustStore>) -> VResult<Self> {
    let verifier = VaultraVerifier::new(host, trust)?;
    Ok(Self {
      config: client_config(verifier.clone()),
      verifier,
      host: host.to_string(),
    })
  }

  async fn wrap<S: AsyncRead + AsyncWrite + Unpin + Send + 'static>(
    &self,
    stream: S,
  ) -> VResult<tokio_rustls::client::TlsStream<S>> {
    let name = ServerName::try_from(self.host.clone())
      .map_err(|_| VError::Protocol(format!("invalid TLS server name: {}", self.host)))?;
    match TlsConnector::from(self.config.clone())
      .connect(name, stream)
      .await
    {
      Ok(stream) => Ok(stream),
      Err(error) => match self.verifier.take_pending() {
        Some(info) => Err(info.into_error()),
        None => Err(VError::Protocol(format!("TLS handshake failed: {error}"))),
      },
    }
  }
}

pub struct FtpClient {
  opts: FtpOptions,
  control: Arc<ControlChannel>,
  conn: Arc<Mutex<Conn>>,
  tls: Option<TlsContext>,
  features: Features,
  home: parking_lot::Mutex<Option<String>>,
}

impl FtpClient {
  pub async fn connect(opts: FtpOptions, log: Arc<Logger>) -> VResult<Self> {
    let control = Arc::new(ControlChannel {
      log: log.clone(),
      reply_timeout: opts.timeout() * 2,
      encoding: parking_lot::Mutex::new(opts.encoding.clone()),
    });
    let tls = match opts.protocol {
      Protocol::Ftp => None,
      _ => Some(TlsContext::new(&opts.host, opts.trust.clone())?),
    };

    let mut conn = Self::open_control(&opts, tls.as_ref(), &log).await?;
    control
      .read_reply(&mut conn)
      .await?
      .ok()
      .map_err(|_| VError::Protocol("unexpected greeting".into()))?;

    if opts.protocol == Protocol::FtpsExplicit {
      let reply = control.command(&mut conn, "AUTH TLS").await?;
      if !reply.is_positive() {
        return Err(VError::Protocol(format!(
          "server refused AUTH TLS: {} {}",
          reply.code, reply.text
        )));
      }
      let (peer, local) = (conn.peer, conn.local);
      let secured = tls.as_ref().unwrap().wrap(conn.into_stream()).await?;
      conn = Conn::new(Box::new(secured), peer, local);
      log.status("Control connection secured with TLS");
    }

    Self::login(&control, &mut conn, &opts).await?;
    log.status("Authenticated");

    if tls.is_some() {
      control.expect_ok(&mut conn, "PBSZ 0").await?;
      control
        .command(&mut conn, "PROT P")
        .await?
        .ok()
        .map_err(|_| VError::Protocol("server refused PROT P".into()))?;
    }

    let features = match control.command(&mut conn, "FEAT").await {
      Ok(reply) if reply.is_positive() => Features::parse(&reply.text),
      _ => Features::default(),
    };
    if features.utf8 {
      let _ = control.command(&mut conn, "OPTS UTF8 ON").await;
      let mut encoding = control.encoding.lock();
      if *encoding == EncodingMode::Auto {
        *encoding = EncodingMode::Utf8;
      }
    }
    if !control.command(&mut conn, "TYPE I").await?.is_positive() {
      log.error("server refused TYPE I; transfers may be corrupted");
    }
    log.status("Connected");
    Ok(Self {
      opts,
      control,
      conn: Arc::new(Mutex::new(conn)),
      tls,
      features,
      home: parking_lot::Mutex::new(None),
    })
  }

  async fn open_control(
    opts: &FtpOptions,
    tls: Option<&TlsContext>,
    log: &Logger,
  ) -> VResult<Conn> {
    log.status(format!("Connecting to {}:{}...", opts.host, opts.port));
    let addresses: Vec<SocketAddr> = tokio::net::lookup_host((opts.host.as_str(), opts.port))
      .await
      .map_err(|e| VError::ConnectionLost(format!("could not resolve {}: {e}", opts.host)))?
      .collect();
    if addresses.is_empty() {
      return Err(VError::ConnectionLost(format!(
        "no addresses for host {}",
        opts.host
      )));
    }
    let mut last_error = VError::ConnectionLost("connection failed".into());
    let mut connected = None;
    for addr in addresses {
      match tokio::time::timeout(opts.timeout(), TcpStream::connect(addr)).await {
        Ok(Ok(stream)) => {
          connected = Some(stream);
          break;
        }
        Ok(Err(e)) => {
          last_error = VError::ConnectionLost(format!("connection to {addr} failed: {e}"))
        }
        Err(_) => last_error = VError::Timeout(format!("connection to {addr} timed out")),
      }
    }
    let Some(tcp) = connected else {
      return Err(last_error);
    };
    let _ = tcp.set_nodelay(true);
    let peer = tcp.peer_addr()?;
    let local = tcp.local_addr()?;
    log.status(format!("Connection established with {peer}"));
    let stream: Box<dyn Rw> = match (opts.protocol, tls) {
      (Protocol::FtpsImplicit, Some(ctx)) => {
        log.status("Starting TLS (implicit FTPS)...");
        Box::new(ctx.wrap(tcp).await?)
      }
      _ => Box::new(tcp),
    };
    Ok(Conn::new(stream, peer, local))
  }

  async fn login(control: &ControlChannel, conn: &mut Conn, opts: &FtpOptions) -> VResult<()> {
    let user = if opts.user.is_empty() {
      ANONYMOUS_USER
    } else {
      opts.user.as_str()
    };
    let mut reply = control.command(conn, &format!("USER {user}")).await?;
    if reply.code == 331 {
      let password = if user == ANONYMOUS_USER && opts.password.is_empty() {
        ANONYMOUS_PASSWORD
      } else {
        opts.password.as_str()
      };
      reply = control
        .send(conn, &format!("PASS {password}"), "PASS ****")
        .await?;
    }
    if reply.code == 332 {
      return Err(VError::AuthFailed(
        "server requires ACCT, which is not supported".into(),
      ));
    }
    if !reply.is_positive() {
      return Err(VError::AuthFailed(reply.text));
    }
    Ok(())
  }

  fn passive(&self) -> bool {
    !matches!(self.opts.mode, TransferMode::Active)
  }

  async fn open_data(&self, conn: &mut Conn, command: &str) -> VResult<Box<dyn Rw>> {
    let tcp = if self.passive() {
      self.open_passive(conn, command).await?
    } else {
      self.open_active(conn, command).await?
    };
    let _ = tcp.set_nodelay(true);
    match &self.tls {
      Some(ctx) => Ok(Box::new(ctx.wrap(tcp).await?)),
      None => Ok(Box::new(tcp)),
    }
  }

  async fn open_passive(&self, conn: &mut Conn, command: &str) -> VResult<TcpStream> {
    let addr = self.passive_addr(conn).await?;
    let tcp = tokio::time::timeout(self.opts.timeout(), TcpStream::connect(addr))
      .await
      .map_err(|_| VError::Timeout("passive data connection timed out".into()))?
      .map_err(|e| VError::ConnectionLost(format!("data connection failed: {e}")))?;
    let reply = self.control.command(conn, command).await?;
    if !reply.is_transfer_start() {
      return Err(reply.into_error());
    }
    Ok(tcp)
  }

  async fn open_active(&self, conn: &mut Conn, command: &str) -> VResult<TcpStream> {
    let listener = TcpListener::bind(SocketAddr::new(conn.local.ip(), 0)).await?;
    let local = listener.local_addr()?;
    let port_command = match local.ip() {
      IpAddr::V4(v4) if !self.features.eprt => {
        let o = v4.octets();
        format!(
          "PORT {},{},{},{},{},{}",
          o[0],
          o[1],
          o[2],
          o[3],
          local.port() >> 8,
          local.port() & 0xff
        )
      }
      IpAddr::V4(ip) => format!("EPRT |1|{ip}|{}|", local.port()),
      IpAddr::V6(ip) => format!("EPRT |2|{ip}|{}|", local.port()),
    };
    self.control.expect_ok(conn, &port_command).await?;
    let reply = self.control.command(conn, command).await?;
    if !reply.is_transfer_start() {
      return Err(reply.into_error());
    }
    let (tcp, from) = tokio::time::timeout(self.opts.timeout(), listener.accept())
      .await
      .map_err(|_| {
        VError::Timeout("server did not connect back in active mode (firewall?)".into())
      })??;
    if from.ip() != conn.peer.ip() {
      self.control.log.error(format!(
        "data connection from unexpected address {from}; refusing"
      ));
      return Err(VError::Protocol(
        "data connection from unexpected origin".into(),
      ));
    }
    Ok(tcp)
  }

  async fn passive_addr(&self, conn: &mut Conn) -> VResult<SocketAddr> {
    if self.features.epsv || conn.peer.is_ipv6() {
      let reply = self.control.command(conn, "EPSV").await?;
      if let Some(port) = reply
        .is_positive()
        .then(|| parse_epsv(&reply.text))
        .flatten()
      {
        return Ok(SocketAddr::new(conn.peer.ip(), port));
      }
    }
    let reply = self.control.expect_ok(conn, "PASV").await?;
    let (ip, port) = parse_pasv(&reply.text)
      .ok_or_else(|| VError::Protocol(format!("invalid PASV reply: {}", reply.text)))?;
    let announced = IpAddr::V4(ip);
    let ip = if is_unroutable(&announced) && !is_unroutable(&conn.peer.ip()) {
      self.control.log.status(
        "Server announced an unroutable passive address; using the server address instead.",
      );
      conn.peer.ip()
    } else {
      announced
    };
    Ok(SocketAddr::new(ip, port))
  }

  async fn read_data_to_end(&self, mut data: Box<dyn Rw>) -> VResult<Vec<u8>> {
    let mut buf = Vec::new();
    tokio::time::timeout(self.opts.timeout() * 4, data.read_to_end(&mut buf))
      .await
      .map_err(|_| VError::Timeout("listing timed out".into()))??;
    let _ = data.shutdown().await;
    Ok(buf)
  }

  async fn fetch_listing(&self, conn: &mut Conn, command: &str) -> VResult<String> {
    let data = self.open_data(conn, command).await?;
    let raw = self.read_data_to_end(data).await?;
    self.control.read_reply(conn).await?.ok()?;
    Ok(self.control.decode(&raw))
  }

  async fn list_dir(&self, conn: &mut Conn, path: &str) -> VResult<Vec<RemoteEntry>> {
    self.control.expect_ok(conn, &format!("CWD {path}")).await?;
    let resolved = self.pwd(conn).await.unwrap_or_else(|_| path.to_string());
    if self.features.mlsd {
      let text = self.fetch_listing(conn, "MLSD").await?;
      return Ok(
        text
          .lines()
          .filter_map(|line| parse_mlsd_line(&resolved, line))
          .collect(),
      );
    }
    let text = match self.fetch_listing(conn, "LIST -a").await {
      Ok(text) => text,
      Err(VError::Unsupported(_) | VError::Protocol(_) | VError::NotFound(_)) => {
        self.fetch_listing(conn, "LIST").await?
      }
      Err(error) => return Err(error),
    };
    Ok(
      text
        .lines()
        .filter_map(|line| parse_list_line(&resolved, line))
        .collect(),
    )
  }

  async fn pwd(&self, conn: &mut Conn) -> VResult<String> {
    let reply = self.control.expect_ok(conn, "PWD").await?;
    parse_pwd(&reply.text)
      .ok_or_else(|| VError::Protocol(format!("invalid PWD reply: {}", reply.text)))
  }

  async fn size_of(&self, conn: &mut Conn, path: &str) -> VResult<u64> {
    let reply = self.control.command(conn, &format!("SIZE {path}")).await?;
    if reply.code == 213 {
      reply
        .text
        .trim()
        .parse()
        .map_err(|_| VError::Protocol("invalid SIZE reply".into()))
    } else {
      Err(reply.into_error())
    }
  }

  async fn prepare_transfer(&self, conn: &mut Conn, offset: u64) -> VResult<()> {
    let transfer_type = if self.opts.ascii { "TYPE A" } else { "TYPE I" };
    self.control.expect_ok(conn, transfer_type).await?;
    if offset > 0 {
      let reply = self
        .control
        .command(conn, &format!("REST {offset}"))
        .await?;
      if reply.code != 350 {
        return Err(VError::Unsupported(format!(
          "server rejected REST: {} {}",
          reply.code, reply.text
        )));
      }
    }
    Ok(())
  }

  async fn annotate_symlink(&self, conn: &mut Conn, entry: &mut RemoteEntry) {
    let is_dir = matches!(self.control.command(conn, &format!("CWD {}", entry.path)).await, Ok(r) if r.is_positive());
    entry.target_is_dir = Some(is_dir);
    if is_dir || !self.features.size {
      return;
    }
    match self.size_of(conn, &entry.path).await {
      Ok(size) => entry.size = size,
      Err(_) => entry.link_broken = true,
    }
  }
}

fn is_unroutable(ip: &IpAddr) -> bool {
  match ip {
    IpAddr::V4(v4) => {
      v4.is_private() || v4.is_loopback() || v4.is_unspecified() || v4.is_link_local()
    }
    IpAddr::V6(v6) => v6.is_loopback() || v6.is_unspecified(),
  }
}

fn parse_epsv(text: &str) -> Option<u16> {
  let start = text.find("(|||")?;
  let rest = &text[start + 4..];
  let end = rest.find('|')?;
  rest[..end].parse().ok()
}

fn parse_pasv(text: &str) -> Option<(std::net::Ipv4Addr, u16)> {
  let re = regex::Regex::new(r"(\d+),(\d+),(\d+),(\d+),(\d+),(\d+)").ok()?;
  let captures = re.captures(text)?;
  let field = |i: usize| captures[i].parse::<u16>().ok();
  let ip = std::net::Ipv4Addr::new(
    field(1)? as u8,
    field(2)? as u8,
    field(3)? as u8,
    field(4)? as u8,
  );
  Some((ip, (field(5)? << 8) | field(6)?))
}

fn parse_pwd(text: &str) -> Option<String> {
  let start = text.find('"')?;
  let mut out = String::new();
  let mut chars = text[start + 1..].chars().peekable();
  while let Some(c) = chars.next() {
    if c != '"' {
      out.push(c);
    } else if chars.peek() == Some(&'"') {
      out.push('"');
      chars.next();
    } else {
      break;
    }
  }
  Some(out)
}

#[async_trait]
impl RemoteClient for FtpClient {
  fn supports_multiplexing(&self) -> bool {
    false
  }

  async fn initial_dir(&self) -> VResult<String> {
    if let Some(home) = self.home.lock().clone() {
      return Ok(home);
    }
    let mut conn = self.conn.lock().await;
    let home = self.pwd(&mut conn).await?;
    *self.home.lock() = Some(home.clone());
    Ok(home)
  }

  async fn realpath(&self, path: &str) -> VResult<String> {
    let mut conn = self.conn.lock().await;
    self
      .control
      .expect_ok(&mut conn, &format!("CWD {path}"))
      .await?;
    self.pwd(&mut conn).await
  }

  async fn list(&self, path: &str) -> VResult<Vec<RemoteEntry>> {
    let mut conn = self.conn.lock().await;
    let mut entries = self.list_dir(&mut conn, path).await?;
    for entry in entries.iter_mut().filter(|e| e.kind == EntryKind::Symlink) {
      self.annotate_symlink(&mut conn, entry).await;
    }
    Ok(entries)
  }

  async fn stat(&self, path: &str) -> VResult<RemoteEntry> {
    let mut conn = self.conn.lock().await;
    if self
      .control
      .command(&mut conn, &format!("CWD {path}"))
      .await?
      .is_positive()
    {
      return Ok(RemoteEntry::plain(path, EntryKind::Dir, 0, None));
    }
    let size = self.size_of(&mut conn, path).await?;
    let mut mtime = None;
    if self.features.mdtm {
      if let Ok(reply) = self
        .control
        .command(&mut conn, &format!("MDTM {path}"))
        .await
      {
        if reply.code == 213 {
          mtime = parse_mlsd_time(reply.text.trim());
        }
      }
    }
    Ok(RemoteEntry::plain(path, EntryKind::File, size, mtime))
  }

  async fn readlink(&self, path: &str) -> VResult<String> {
    let name = basename(path);
    self
      .list(&parent(path))
      .await?
      .into_iter()
      .find(|e| e.name == name)
      .and_then(|e| e.link_target)
      .ok_or_else(|| VError::Unsupported("server does not expose the link target".into()))
  }

  async fn mkdir(&self, path: &str) -> VResult<()> {
    let mut conn = self.conn.lock().await;
    self
      .control
      .expect_ok(&mut conn, &format!("MKD {path}"))
      .await
      .map(drop)
  }

  async fn rmdir(&self, path: &str) -> VResult<()> {
    let mut conn = self.conn.lock().await;
    self
      .control
      .expect_ok(&mut conn, &format!("RMD {path}"))
      .await
      .map(drop)
  }

  async fn remove(&self, path: &str) -> VResult<()> {
    let mut conn = self.conn.lock().await;
    self
      .control
      .expect_ok(&mut conn, &format!("DELE {path}"))
      .await
      .map(drop)
  }

  async fn rename(&self, from: &str, to: &str) -> VResult<()> {
    let mut conn = self.conn.lock().await;
    let reply = self
      .control
      .command(&mut conn, &format!("RNFR {from}"))
      .await?;
    if reply.code != 350 {
      return Err(reply.into_error());
    }
    self
      .control
      .expect_ok(&mut conn, &format!("RNTO {to}"))
      .await
      .map(drop)
  }

  async fn chmod(&self, path: &str, mode: u32) -> VResult<()> {
    let mut conn = self.conn.lock().await;
    self
      .control
      .expect_ok(&mut conn, &format!("SITE CHMOD {:o} {path}", mode & 0o7777))
      .await
      .map(drop)
  }

  async fn touch(&self, path: &str) -> VResult<()> {
    self.open_write(path, 0).await?.finish().await
  }

  async fn open_read(&self, path: &str, offset: u64) -> VResult<Box<dyn RemoteReader>> {
    let mut guard = self.conn.clone().lock_owned().await;
    self.prepare_transfer(&mut guard, offset).await?;
    let data = self.open_data(&mut guard, &format!("RETR {path}")).await?;
    Ok(Box::new(FtpReader {
      control: self.control.clone(),
      guard: Some(guard),
      data: Some(data),
      eof: false,
    }))
  }

  async fn open_write(&self, path: &str, offset: u64) -> VResult<Box<dyn RemoteWriter>> {
    let mut guard = self.conn.clone().lock_owned().await;
    self.prepare_transfer(&mut guard, offset).await?;
    let data = self.open_data(&mut guard, &format!("STOR {path}")).await?;
    Ok(Box::new(FtpWriter {
      control: self.control.clone(),
      guard: Some(guard),
      data: Some(data),
    }))
  }

  async fn keepalive(&self) -> VResult<()> {
    if let Ok(mut conn) = self.conn.try_lock() {
      self.control.command(&mut conn, "NOOP").await?;
    }
    Ok(())
  }

  async fn disconnect(&self) {
    if let Ok(mut conn) = tokio::time::timeout(Duration::from_secs(2), self.conn.lock()).await {
      let _ = tokio::time::timeout(
        Duration::from_secs(3),
        self.control.command(&mut conn, "QUIT"),
      )
      .await;
      let _ = conn.writer.shutdown().await;
    }
    self.control.log.status("Disconnected");
  }

  fn supported_hashes(&self) -> Vec<String> {
    self
      .features
      .hash
      .iter()
      .map(|h| h.to_ascii_lowercase().replace('-', ""))
      .chain(
        self
          .features
          .xhash
          .iter()
          .map(|x| x.trim_start_matches('X').to_ascii_lowercase()),
      )
      .collect()
  }

  async fn hash(&self, path: &str, algo: &str) -> VResult<Option<String>> {
    let mut conn = self.conn.lock().await;
    let upper = algo.to_ascii_uppercase();
    if self
      .features
      .xhash
      .iter()
      .any(|x| x == &format!("X{upper}"))
    {
      let reply = self
        .control
        .command(&mut conn, &format!("X{upper} {path}"))
        .await?;
      let digest = matches!(reply.code, 250 | 213)
        .then(|| {
          reply
            .text
            .split_whitespace()
            .last()
            .map(str::to_ascii_lowercase)
        })
        .flatten();
      return Ok(digest);
    }
    let name = match upper.as_str() {
      "SHA256" => "SHA-256",
      "SHA1" => "SHA-1",
      "SHA512" => "SHA-512",
      "MD5" => "MD5",
      _ => return Ok(None),
    };
    if !self.features.hash.iter().any(|h| h == name) {
      return Ok(None);
    }
    let _ = self
      .control
      .command(&mut conn, &format!("OPTS HASH {name}"))
      .await;
    let reply = self
      .control
      .command(&mut conn, &format!("HASH {path}"))
      .await?;
    if reply.code != 213 {
      return Ok(None);
    }
    Ok(
      reply
        .text
        .split_whitespace()
        .nth(2)
        .map(str::to_ascii_lowercase),
    )
  }
}

const GRACEFUL_CLOSE_TIMEOUT: Duration = Duration::from_secs(5);
const ABORT_CLOSE_TIMEOUT: Duration = Duration::from_millis(500);

async fn close_data_channel(data: &mut Box<dyn Rw>, graceful: bool) {
  let _ = data.shutdown().await;
  let budget = if graceful {
    GRACEFUL_CLOSE_TIMEOUT
  } else {
    ABORT_CLOSE_TIMEOUT
  };
  let drain = async {
    let mut sink = vec![0u8; 16 * 1024];
    while let Ok(n) = data.read(&mut sink).await {
      if n == 0 {
        break;
      }
    }
  };
  let _ = tokio::time::timeout(budget, drain).await;
}

struct FtpReader {
  control: Arc<ControlChannel>,
  guard: Option<OwnedMutexGuard<Conn>>,
  data: Option<Box<dyn Rw>>,
  eof: bool,
}

#[async_trait]
impl RemoteReader for FtpReader {
  async fn read(&mut self, buf: &mut [u8]) -> VResult<usize> {
    let Some(data) = self.data.as_mut() else {
      return Ok(0);
    };
    let n = data.read(buf).await?;
    if n == 0 {
      self.eof = true;
    }
    Ok(n)
  }

  async fn finish(&mut self) -> VResult<()> {
    if let Some(mut data) = self.data.take() {
      close_data_channel(&mut data, self.eof).await;
    }
    let Some(mut conn) = self.guard.take() else {
      return Ok(());
    };
    let reply = self.control.read_reply(&mut conn).await?;
    if self.eof {
      reply.ok().map(drop)
    } else {
      Ok(())
    }
  }
}

struct FtpWriter {
  control: Arc<ControlChannel>,
  guard: Option<OwnedMutexGuard<Conn>>,
  data: Option<Box<dyn Rw>>,
}

#[async_trait]
impl RemoteWriter for FtpWriter {
  async fn write(&mut self, buf: &[u8]) -> VResult<()> {
    let Some(data) = self.data.as_mut() else {
      return Err(VError::Protocol("data stream closed".into()));
    };
    data.write_all(buf).await?;
    Ok(())
  }

  async fn finish(&mut self) -> VResult<()> {
    if let Some(mut data) = self.data.take() {
      data.flush().await?;
      close_data_channel(&mut data, true).await;
    }
    let Some(mut conn) = self.guard.take() else {
      return Ok(());
    };
    self.control.read_reply(&mut conn).await?.ok().map(drop)
  }
}
