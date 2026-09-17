use super::packet::*;
use crate::error::{VError, VResult};
use crate::logging::Logger;
use crate::protocol::{
  basename, join, parent, EntryKind, RemoteClient, RemoteEntry, RemoteReader, RemoteWriter,
};
use async_trait::async_trait;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot, Mutex};

const CHUNK_SIZE: usize = 32 * 1024;
const MAX_IN_FLIGHT: usize = 64;
const MAX_PACKET_SIZE: usize = 512 * 1024 + 1024;
const INIT_TIMEOUT_SECS: u64 = 60;
const EXT_POSIX_RENAME: &str = "posix-rename@openssh.com";
const EXT_CHECK_FILE: &str = "check-file";

type PendingMap = Arc<parking_lot::Mutex<HashMap<u32, oneshot::Sender<VResult<Response>>>>>;
type CloseHook = Box<dyn FnOnce() + Send>;

#[derive(Clone)]
struct Transport {
  tx: mpsc::Sender<Vec<u8>>,
  pending: PendingMap,
  next_id: Arc<AtomicU32>,
  alive: Arc<AtomicBool>,
}

impl Transport {
  fn next_id(&self) -> u32 {
    self.next_id.fetch_add(1, Ordering::Relaxed)
  }

  fn connection_lost() -> VError {
    VError::ConnectionLost("SFTP connection closed".into())
  }

  async fn send(&self, id: u32, packet: Vec<u8>) -> VResult<oneshot::Receiver<VResult<Response>>> {
    if !self.alive.load(Ordering::Relaxed) {
      return Err(Self::connection_lost());
    }
    let (tx, rx) = oneshot::channel();
    self.pending.lock().insert(id, tx);
    if self.tx.send(packet).await.is_err() {
      self.pending.lock().remove(&id);
      return Err(Self::connection_lost());
    }
    Ok(rx)
  }

  async fn call(&self, id: u32, packet: Vec<u8>) -> VResult<Response> {
    let rx = self.send(id, packet).await?;
    rx.await.map_err(|_| Self::connection_lost())?
  }

  async fn close_handle(&self, handle: &[u8]) -> VResult<()> {
    let id = self.next_id();
    let packet = PacketBuilder::new(SSH_FXP_CLOSE, id).bytes(handle).build();
    self.call(id, packet).await?.into_status()
  }
}

pub struct SftpClient {
  transport: Transport,
  extensions: HashMap<String, String>,
  home: Mutex<Option<String>>,
  log: Arc<Logger>,
  on_close: Mutex<Option<CloseHook>>,
}

impl SftpClient {
  pub async fn handshake<R, W>(
    reader: R,
    writer: W,
    log: Arc<Logger>,
    on_close: CloseHook,
  ) -> VResult<Self>
  where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
  {
    let (tx, rx) = mpsc::channel::<Vec<u8>>(256);
    let pending: PendingMap = Arc::new(parking_lot::Mutex::new(HashMap::new()));
    let alive = Arc::new(AtomicBool::new(true));
    let (init_tx, init_rx) = oneshot::channel::<VResult<Response>>();

    tokio::spawn(write_loop(writer, rx, alive.clone()));
    tokio::spawn(read_loop(reader, pending.clone(), alive.clone(), init_tx));

    tx.send(PacketBuilder::init())
      .await
      .map_err(|_| VError::ConnectionLost("failed to send SFTP INIT".into()))?;

    let response = tokio::time::timeout(std::time::Duration::from_secs(INIT_TIMEOUT_SECS), init_rx)
      .await
      .map_err(|_| VError::Timeout("SFTP server did not answer INIT".into()))?
      .map_err(|_| VError::ConnectionLost("connection closed before INIT".into()))??;

    if response.packet_type != SSH_FXP_VERSION {
      return Err(VError::Protocol(format!(
        "unexpected reply to INIT: packet type {}",
        response.packet_type
      )));
    }
    let version = response.id;
    let mut extensions = HashMap::new();
    let mut reader = Reader::new(&response.payload);
    while reader.remaining() >= 8 {
      let name = reader.string()?;
      let value = reader.string()?;
      extensions.insert(name, value);
    }
    log.status(format!(
      "SFTP version {version} negotiated ({} extensions)",
      extensions.len()
    ));

    Ok(Self {
      transport: Transport {
        tx,
        pending,
        next_id: Arc::new(AtomicU32::new(1)),
        alive,
      },
      extensions,
      home: Mutex::new(None),
      log,
      on_close: Mutex::new(Some(on_close)),
    })
  }

  pub fn has_extension(&self, name: &str) -> bool {
    self.extensions.contains_key(name)
  }

  async fn status_request(
    &self,
    packet_type: u8,
    build: impl FnOnce(PacketBuilder) -> PacketBuilder,
  ) -> VResult<()> {
    let id = self.transport.next_id();
    let packet = build(PacketBuilder::new(packet_type, id)).build();
    self.transport.call(id, packet).await?.into_status()
  }

  async fn attrs_request(&self, packet_type: u8, path: &str) -> VResult<Attrs> {
    let id = self.transport.next_id();
    let packet = PacketBuilder::new(packet_type, id).string(path).build();
    let response = self.transport.call(id, packet).await?;
    match response.packet_type {
      SSH_FXP_ATTRS => Attrs::decode(&mut Reader::new(&response.payload)),
      _ => Err(response.unexpected()),
    }
  }

  async fn name_request(&self, packet_type: u8, path: &str) -> VResult<String> {
    let id = self.transport.next_id();
    let packet = PacketBuilder::new(packet_type, id).string(path).build();
    let response = self.transport.call(id, packet).await?;
    match response.packet_type {
      SSH_FXP_NAME => {
        let mut reader = Reader::new(&response.payload);
        if reader.u32()? < 1 {
          return Err(VError::Protocol("empty NAME reply".into()));
        }
        reader.string()
      }
      _ => Err(response.unexpected()),
    }
  }

  async fn handle_request(
    &self,
    packet_type: u8,
    build: impl FnOnce(PacketBuilder) -> PacketBuilder,
  ) -> VResult<Vec<u8>> {
    let id = self.transport.next_id();
    let packet = build(PacketBuilder::new(packet_type, id)).build();
    let response = self.transport.call(id, packet).await?;
    match response.packet_type {
      SSH_FXP_HANDLE => Ok(Reader::new(&response.payload).bytes()?.to_vec()),
      _ => Err(response.unexpected()),
    }
  }

  pub async fn stat_attrs(&self, path: &str) -> VResult<Attrs> {
    self.attrs_request(SSH_FXP_STAT, path).await
  }

  fn entry_from(dir: &str, name: &str, longname: &str, attrs: &Attrs) -> RemoteEntry {
    let kind = if attrs.is_dir() {
      EntryKind::Dir
    } else if attrs.is_symlink() {
      EntryKind::Symlink
    } else if attrs.is_regular() || attrs.permissions.is_none() {
      EntryKind::File
    } else {
      EntryKind::Other
    };
    let mut fields = longname.split_whitespace().skip(2);
    let owner = fields.next().map(str::to_string);
    let group = fields.next().map(str::to_string);
    RemoteEntry {
      name: name.to_string(),
      path: join(dir, name),
      kind,
      size: attrs.size.unwrap_or(0),
      mtime: attrs.mtime.map(i64::from),
      permissions: attrs.permissions.map(|p| p & 0o7777),
      owner: owner.or_else(|| attrs.uid.map(|u| u.to_string())),
      group: group.or_else(|| attrs.gid.map(|g| g.to_string())),
      link_target: None,
      link_broken: false,
      target_is_dir: None,
    }
  }

  async fn read_directory(&self, path: &str) -> VResult<Vec<RemoteEntry>> {
    let handle = self
      .handle_request(SSH_FXP_OPENDIR, |b| b.string(path))
      .await?;
    let result = self.collect_directory(path, &handle).await;
    let _ = self.transport.close_handle(&handle).await;
    result
  }

  async fn collect_directory(&self, path: &str, handle: &[u8]) -> VResult<Vec<RemoteEntry>> {
    let mut entries = Vec::new();
    loop {
      let id = self.transport.next_id();
      let packet = PacketBuilder::new(SSH_FXP_READDIR, id)
        .bytes(handle)
        .build();
      let response = self.transport.call(id, packet).await?;
      match response.packet_type {
        SSH_FXP_NAME => {
          let mut reader = Reader::new(&response.payload);
          let count = reader.u32()?;
          for _ in 0..count {
            let name = reader.string()?;
            let longname = reader.string()?;
            let attrs = Attrs::decode(&mut reader)?;
            if name != "." && name != ".." {
              entries.push(Self::entry_from(path, &name, &longname, &attrs));
            }
          }
        }
        SSH_FXP_STATUS if response.is_eof() => return Ok(entries),
        _ => return Err(response.unexpected()),
      }
    }
  }

  async fn resolve_symlinks(&self, entries: &mut [RemoteEntry]) {
    let indices: Vec<usize> = entries
      .iter()
      .enumerate()
      .filter(|(_, e)| e.kind == EntryKind::Symlink)
      .map(|(i, _)| i)
      .collect();
    if indices.is_empty() {
      return;
    }
    let lookups = indices.iter().map(|&i| {
      let path = entries[i].path.clone();
      async move {
        (
          self.readlink(&path).await.ok(),
          self.stat_attrs(&path).await,
        )
      }
    });
    let results = futures::future::join_all(lookups).await;
    for (&i, (target, stat)) in indices.iter().zip(results) {
      let entry = &mut entries[i];
      entry.link_target = target;
      match stat {
        Ok(attrs) => {
          entry.target_is_dir = Some(attrs.is_dir());
          if !attrs.is_dir() {
            entry.size = attrs.size.unwrap_or(entry.size);
          }
        }
        Err(_) => {
          entry.link_broken = true;
          entry.target_is_dir = Some(false);
        }
      }
    }
  }
}

async fn write_loop<W: AsyncWrite + Unpin>(
  mut writer: W,
  mut rx: mpsc::Receiver<Vec<u8>>,
  alive: Arc<AtomicBool>,
) {
  while let Some(packet) = rx.recv().await {
    if writer.write_all(&packet).await.is_err() {
      break;
    }
    while let Ok(next) = rx.try_recv() {
      if writer.write_all(&next).await.is_err() {
        alive.store(false, Ordering::Relaxed);
        return;
      }
    }
    if writer.flush().await.is_err() {
      break;
    }
  }
  alive.store(false, Ordering::Relaxed);
}

async fn read_loop<R: AsyncRead + Unpin>(
  mut reader: R,
  pending: PendingMap,
  alive: Arc<AtomicBool>,
  init_tx: oneshot::Sender<VResult<Response>>,
) {
  let mut init_tx = Some(init_tx);
  let mut header = [0u8; 4];
  loop {
    if reader.read_exact(&mut header).await.is_err() {
      break;
    }
    let len = u32::from_be_bytes(header) as usize;
    if !(5..=MAX_PACKET_SIZE).contains(&len) {
      tracing::error!("invalid SFTP packet length: {len}");
      break;
    }
    let mut body = vec![0u8; len];
    if reader.read_exact(&mut body).await.is_err() {
      break;
    }
    let packet_type = body[0];
    let id = u32::from_be_bytes([body[1], body[2], body[3], body[4]]);
    let payload = body.split_off(5);
    let response = Response {
      packet_type,
      id,
      payload,
    };
    if packet_type == SSH_FXP_VERSION {
      if let Some(tx) = init_tx.take() {
        let _ = tx.send(Ok(response));
      }
      continue;
    }
    match pending.lock().remove(&id) {
      Some(tx) => {
        let _ = tx.send(Ok(response));
      }
      None => {
        tracing::warn!("SFTP reply without pending request (id={id}, type={packet_type})")
      }
    }
  }
  alive.store(false, Ordering::Relaxed);
  if let Some(tx) = init_tx.take() {
    let _ = tx.send(Err(VError::ConnectionLost("connection closed".into())));
  }
  let waiters: Vec<_> = pending.lock().drain().collect();
  for (_, tx) in waiters {
    let _ = tx.send(Err(Transport::connection_lost()));
  }
}

#[async_trait]
impl RemoteClient for SftpClient {
  fn supports_multiplexing(&self) -> bool {
    true
  }

  async fn initial_dir(&self) -> VResult<String> {
    if let Some(home) = self.home.lock().await.clone() {
      return Ok(home);
    }
    let home = self.realpath(".").await?;
    *self.home.lock().await = Some(home.clone());
    Ok(home)
  }

  async fn realpath(&self, path: &str) -> VResult<String> {
    self.name_request(SSH_FXP_REALPATH, path).await
  }

  async fn list(&self, path: &str) -> VResult<Vec<RemoteEntry>> {
    let mut entries = self.read_directory(path).await?;
    self.resolve_symlinks(&mut entries).await;
    Ok(entries)
  }

  async fn stat(&self, path: &str) -> VResult<RemoteEntry> {
    let attrs = self.stat_attrs(path).await?;
    Ok(Self::entry_from(&parent(path), &basename(path), "", &attrs))
  }

  async fn readlink(&self, path: &str) -> VResult<String> {
    self.name_request(SSH_FXP_READLINK, path).await
  }

  async fn mkdir(&self, path: &str) -> VResult<()> {
    self
      .status_request(SSH_FXP_MKDIR, |b| b.string(path).attrs(&Attrs::default()))
      .await
  }

  async fn rmdir(&self, path: &str) -> VResult<()> {
    self.status_request(SSH_FXP_RMDIR, |b| b.string(path)).await
  }

  async fn remove(&self, path: &str) -> VResult<()> {
    self
      .status_request(SSH_FXP_REMOVE, |b| b.string(path))
      .await
  }

  async fn rename(&self, from: &str, to: &str) -> VResult<()> {
    if self.has_extension(EXT_POSIX_RENAME) {
      return self
        .status_request(SSH_FXP_EXTENDED, |b| {
          b.string(EXT_POSIX_RENAME).string(from).string(to)
        })
        .await;
    }
    self
      .status_request(SSH_FXP_RENAME, |b| b.string(from).string(to))
      .await
  }

  async fn chmod(&self, path: &str, mode: u32) -> VResult<()> {
    let attrs = Attrs {
      permissions: Some(mode & 0o7777),
      ..Default::default()
    };
    self
      .status_request(SSH_FXP_SETSTAT, |b| b.string(path).attrs(&attrs))
      .await
  }

  async fn touch(&self, path: &str) -> VResult<()> {
    let flags = SSH_FXF_WRITE | SSH_FXF_CREAT | SSH_FXF_EXCL;
    let handle = self
      .handle_request(SSH_FXP_OPEN, |b| {
        b.string(path).u32(flags).attrs(&Attrs::default())
      })
      .await?;
    self.transport.close_handle(&handle).await
  }

  async fn open_read(&self, path: &str, offset: u64) -> VResult<Box<dyn RemoteReader>> {
    let handle = self
      .handle_request(SSH_FXP_OPEN, |b| {
        b.string(path).u32(SSH_FXF_READ).attrs(&Attrs::default())
      })
      .await?;
    Ok(Box::new(SftpReader::new(
      self.transport.clone(),
      handle,
      offset,
    )))
  }

  async fn open_write(&self, path: &str, offset: u64) -> VResult<Box<dyn RemoteWriter>> {
    let flags = if offset > 0 {
      SSH_FXF_WRITE | SSH_FXF_CREAT
    } else {
      SSH_FXF_WRITE | SSH_FXF_CREAT | SSH_FXF_TRUNC
    };
    let handle = self
      .handle_request(SSH_FXP_OPEN, |b| {
        b.string(path).u32(flags).attrs(&Attrs::default())
      })
      .await?;
    Ok(Box::new(SftpWriter::new(
      self.transport.clone(),
      handle,
      offset,
    )))
  }

  async fn keepalive(&self) -> VResult<()> {
    self.realpath(".").await.map(|_| ())
  }

  async fn disconnect(&self) {
    self.transport.alive.store(false, Ordering::Relaxed);
    if let Some(hook) = self.on_close.lock().await.take() {
      hook();
    }
    self.log.status("Disconnected");
  }

  fn supported_hashes(&self) -> Vec<String> {
    if self.has_extension(EXT_CHECK_FILE) {
      vec!["sha256".into(), "sha1".into(), "md5".into()]
    } else {
      Vec::new()
    }
  }

  async fn hash(&self, path: &str, algo: &str) -> VResult<Option<String>> {
    if !self.has_extension(EXT_CHECK_FILE) {
      return Ok(None);
    }
    let id = self.transport.next_id();
    let packet = PacketBuilder::new(SSH_FXP_EXTENDED, id)
      .string("check-file-name")
      .string(path)
      .string(algo)
      .u64(0)
      .u64(0)
      .u32(0)
      .build();
    let response = self.transport.call(id, packet).await?;
    if response.packet_type != SSH_FXP_EXTENDED_REPLY {
      return Ok(None);
    }
    let mut reader = Reader::new(&response.payload);
    let _algorithm = reader.string()?;
    let digest = reader.bytes().unwrap_or(&[]);
    Ok((!digest.is_empty()).then(|| hex::encode(digest)))
  }
}

struct SftpReader {
  transport: Transport,
  handle: Vec<u8>,
  next_offset: u64,
  in_flight: VecDeque<oneshot::Receiver<VResult<Response>>>,
  eof: bool,
  buffer: Vec<u8>,
  buffer_pos: usize,
}

impl SftpReader {
  fn new(transport: Transport, handle: Vec<u8>, offset: u64) -> Self {
    Self {
      transport,
      handle,
      next_offset: offset,
      in_flight: VecDeque::new(),
      eof: false,
      buffer: Vec::new(),
      buffer_pos: 0,
    }
  }

  async fn fill_pipeline(&mut self) -> VResult<()> {
    while !self.eof && self.in_flight.len() < MAX_IN_FLIGHT {
      let id = self.transport.next_id();
      let packet = PacketBuilder::new(SSH_FXP_READ, id)
        .bytes(&self.handle)
        .u64(self.next_offset)
        .u32(CHUNK_SIZE as u32)
        .build();
      let rx = self.transport.send(id, packet).await?;
      self.in_flight.push_back(rx);
      self.next_offset += CHUNK_SIZE as u64;
    }
    Ok(())
  }

  fn drain_buffer(&mut self, out: &mut [u8]) -> usize {
    let n = (self.buffer.len() - self.buffer_pos).min(out.len());
    out[..n].copy_from_slice(&self.buffer[self.buffer_pos..self.buffer_pos + n]);
    self.buffer_pos += n;
    n
  }
}

#[async_trait]
impl RemoteReader for SftpReader {
  async fn read(&mut self, out: &mut [u8]) -> VResult<usize> {
    loop {
      if self.buffer_pos < self.buffer.len() {
        return Ok(self.drain_buffer(out));
      }
      if self.eof && self.in_flight.is_empty() {
        return Ok(0);
      }
      self.fill_pipeline().await?;
      let Some(rx) = self.in_flight.pop_front() else {
        return Ok(0);
      };
      let response = rx.await.map_err(|_| Transport::connection_lost())??;
      match response.packet_type {
        SSH_FXP_DATA => {
          let data = Reader::new(&response.payload).bytes()?;
          if data.len() < CHUNK_SIZE {
            self.eof = true;
          }
          if data.is_empty() {
            continue;
          }
          self.buffer = data.to_vec();
          self.buffer_pos = 0;
        }
        SSH_FXP_STATUS if response.is_eof() => self.eof = true,
        _ => return Err(response.unexpected()),
      }
    }
  }

  async fn finish(&mut self) -> VResult<()> {
    while let Some(rx) = self.in_flight.pop_front() {
      let _ = rx.await;
    }
    self.transport.close_handle(&self.handle).await
  }
}

struct SftpWriter {
  transport: Transport,
  handle: Vec<u8>,
  offset: u64,
  in_flight: VecDeque<oneshot::Receiver<VResult<Response>>>,
}

impl SftpWriter {
  fn new(transport: Transport, handle: Vec<u8>, offset: u64) -> Self {
    Self {
      transport,
      handle,
      offset,
      in_flight: VecDeque::new(),
    }
  }

  async fn await_oldest(&mut self) -> VResult<()> {
    match self.in_flight.pop_front() {
      Some(rx) => rx
        .await
        .map_err(|_| Transport::connection_lost())??
        .into_status(),
      None => Ok(()),
    }
  }
}

#[async_trait]
impl RemoteWriter for SftpWriter {
  async fn write(&mut self, buf: &[u8]) -> VResult<()> {
    for chunk in buf.chunks(CHUNK_SIZE) {
      while self.in_flight.len() >= MAX_IN_FLIGHT {
        self.await_oldest().await?;
      }
      let id = self.transport.next_id();
      let packet = PacketBuilder::new(SSH_FXP_WRITE, id)
        .bytes(&self.handle)
        .u64(self.offset)
        .bytes(chunk)
        .build();
      let rx = self.transport.send(id, packet).await?;
      self.in_flight.push_back(rx);
      self.offset += chunk.len() as u64;
    }
    Ok(())
  }

  async fn finish(&mut self) -> VResult<()> {
    let mut first_error = None;
    while !self.in_flight.is_empty() {
      if let Err(e) = self.await_oldest().await {
        first_error.get_or_insert(e);
      }
    }
    let closed = self.transport.close_handle(&self.handle).await;
    match first_error {
      Some(e) => Err(e),
      None => closed,
    }
  }
}
