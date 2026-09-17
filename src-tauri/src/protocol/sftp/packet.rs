use crate::error::{VError, VResult};
use bytes::{Buf, BufMut, BytesMut};

pub const SFTP_VERSION: u32 = 3;

pub const SSH_FXP_INIT: u8 = 1;
pub const SSH_FXP_VERSION: u8 = 2;
pub const SSH_FXP_OPEN: u8 = 3;
pub const SSH_FXP_CLOSE: u8 = 4;
pub const SSH_FXP_READ: u8 = 5;
pub const SSH_FXP_WRITE: u8 = 6;
pub const SSH_FXP_SETSTAT: u8 = 9;
pub const SSH_FXP_OPENDIR: u8 = 11;
pub const SSH_FXP_READDIR: u8 = 12;
pub const SSH_FXP_REMOVE: u8 = 13;
pub const SSH_FXP_MKDIR: u8 = 14;
pub const SSH_FXP_RMDIR: u8 = 15;
pub const SSH_FXP_REALPATH: u8 = 16;
pub const SSH_FXP_STAT: u8 = 17;
pub const SSH_FXP_RENAME: u8 = 18;
pub const SSH_FXP_READLINK: u8 = 19;
pub const SSH_FXP_STATUS: u8 = 101;
pub const SSH_FXP_HANDLE: u8 = 102;
pub const SSH_FXP_DATA: u8 = 103;
pub const SSH_FXP_NAME: u8 = 104;
pub const SSH_FXP_ATTRS: u8 = 105;
pub const SSH_FXP_EXTENDED: u8 = 200;
pub const SSH_FXP_EXTENDED_REPLY: u8 = 201;

pub const SSH_FX_OK: u32 = 0;
pub const SSH_FX_EOF: u32 = 1;
pub const SSH_FX_NO_SUCH_FILE: u32 = 2;
pub const SSH_FX_PERMISSION_DENIED: u32 = 3;
pub const SSH_FX_FAILURE: u32 = 4;
pub const SSH_FX_BAD_MESSAGE: u32 = 5;
pub const SSH_FX_NO_CONNECTION: u32 = 6;
pub const SSH_FX_CONNECTION_LOST: u32 = 7;
pub const SSH_FX_OP_UNSUPPORTED: u32 = 8;

pub const SSH_FXF_READ: u32 = 0x01;
pub const SSH_FXF_WRITE: u32 = 0x02;
pub const SSH_FXF_CREAT: u32 = 0x08;
pub const SSH_FXF_TRUNC: u32 = 0x10;
pub const SSH_FXF_EXCL: u32 = 0x20;

pub const SSH_FILEXFER_ATTR_SIZE: u32 = 0x1;
pub const SSH_FILEXFER_ATTR_UIDGID: u32 = 0x2;
pub const SSH_FILEXFER_ATTR_PERMISSIONS: u32 = 0x4;
pub const SSH_FILEXFER_ATTR_ACMODTIME: u32 = 0x8;
pub const SSH_FILEXFER_ATTR_EXTENDED: u32 = 0x8000_0000;

pub const S_IFMT: u32 = 0o170000;
pub const S_IFDIR: u32 = 0o040000;
pub const S_IFREG: u32 = 0o100000;
pub const S_IFLNK: u32 = 0o120000;

#[derive(Debug, Clone, Default)]
pub struct Attrs {
  pub size: Option<u64>,
  pub uid: Option<u32>,
  pub gid: Option<u32>,
  pub permissions: Option<u32>,
  pub atime: Option<u32>,
  pub mtime: Option<u32>,
}

impl Attrs {
  pub fn is_dir(&self) -> bool {
    self
      .permissions
      .map(|p| p & S_IFMT == S_IFDIR)
      .unwrap_or(false)
  }

  pub fn is_symlink(&self) -> bool {
    self
      .permissions
      .map(|p| p & S_IFMT == S_IFLNK)
      .unwrap_or(false)
  }

  pub fn is_regular(&self) -> bool {
    self
      .permissions
      .map(|p| p & S_IFMT == S_IFREG)
      .unwrap_or(false)
  }

  pub fn encode(&self, out: &mut BytesMut) {
    let mut flags = 0u32;
    if self.size.is_some() {
      flags |= SSH_FILEXFER_ATTR_SIZE;
    }
    if self.uid.is_some() && self.gid.is_some() {
      flags |= SSH_FILEXFER_ATTR_UIDGID;
    }
    if self.permissions.is_some() {
      flags |= SSH_FILEXFER_ATTR_PERMISSIONS;
    }
    if self.atime.is_some() && self.mtime.is_some() {
      flags |= SSH_FILEXFER_ATTR_ACMODTIME;
    }
    out.put_u32(flags);
    if let Some(size) = self.size {
      out.put_u64(size);
    }
    if let (Some(uid), Some(gid)) = (self.uid, self.gid) {
      out.put_u32(uid);
      out.put_u32(gid);
    }
    if let Some(permissions) = self.permissions {
      out.put_u32(permissions);
    }
    if let (Some(atime), Some(mtime)) = (self.atime, self.mtime) {
      out.put_u32(atime);
      out.put_u32(mtime);
    }
  }

  pub fn decode(reader: &mut Reader<'_>) -> VResult<Attrs> {
    let flags = reader.u32()?;
    let mut attrs = Attrs::default();
    if flags & SSH_FILEXFER_ATTR_SIZE != 0 {
      attrs.size = Some(reader.u64()?);
    }
    if flags & SSH_FILEXFER_ATTR_UIDGID != 0 {
      attrs.uid = Some(reader.u32()?);
      attrs.gid = Some(reader.u32()?);
    }
    if flags & SSH_FILEXFER_ATTR_PERMISSIONS != 0 {
      attrs.permissions = Some(reader.u32()?);
    }
    if flags & SSH_FILEXFER_ATTR_ACMODTIME != 0 {
      attrs.atime = Some(reader.u32()?);
      attrs.mtime = Some(reader.u32()?);
    }
    if flags & SSH_FILEXFER_ATTR_EXTENDED != 0 {
      let count = reader.u32()?;
      for _ in 0..count {
        reader.string()?;
        reader.string()?;
      }
    }
    Ok(attrs)
  }
}

pub struct Reader<'a> {
  buf: &'a [u8],
}

impl<'a> Reader<'a> {
  pub fn new(buf: &'a [u8]) -> Self {
    Self { buf }
  }

  pub fn remaining(&self) -> usize {
    self.buf.len()
  }

  fn ensure(&self, n: usize) -> VResult<()> {
    if self.buf.len() < n {
      Err(VError::Protocol("truncated SFTP packet".into()))
    } else {
      Ok(())
    }
  }

  pub fn u32(&mut self) -> VResult<u32> {
    self.ensure(4)?;
    Ok(self.buf.get_u32())
  }

  pub fn u64(&mut self) -> VResult<u64> {
    self.ensure(8)?;
    Ok(self.buf.get_u64())
  }

  pub fn bytes(&mut self) -> VResult<&'a [u8]> {
    let n = self.u32()? as usize;
    self.ensure(n)?;
    let (head, tail) = self.buf.split_at(n);
    self.buf = tail;
    Ok(head)
  }

  pub fn string(&mut self) -> VResult<String> {
    let bytes = self.bytes()?;
    Ok(String::from_utf8_lossy(bytes).into_owned())
  }
}

pub struct PacketBuilder {
  buf: BytesMut,
}

impl PacketBuilder {
  pub fn new(packet_type: u8, id: u32) -> Self {
    let mut buf = BytesMut::with_capacity(64);
    buf.put_u32(0);
    buf.put_u8(packet_type);
    buf.put_u32(id);
    Self { buf }
  }

  pub fn init() -> Vec<u8> {
    let mut buf = BytesMut::with_capacity(9);
    buf.put_u32(5);
    buf.put_u8(SSH_FXP_INIT);
    buf.put_u32(SFTP_VERSION);
    buf.to_vec()
  }

  pub fn u32(mut self, value: u32) -> Self {
    self.buf.put_u32(value);
    self
  }

  pub fn u64(mut self, value: u64) -> Self {
    self.buf.put_u64(value);
    self
  }

  pub fn bytes(mut self, value: &[u8]) -> Self {
    self.buf.put_u32(value.len() as u32);
    self.buf.put_slice(value);
    self
  }

  pub fn string(self, value: &str) -> Self {
    self.bytes(value.as_bytes())
  }

  pub fn attrs(mut self, attrs: &Attrs) -> Self {
    attrs.encode(&mut self.buf);
    self
  }

  pub fn build(mut self) -> Vec<u8> {
    let len = (self.buf.len() - 4) as u32;
    self.buf[0..4].copy_from_slice(&len.to_be_bytes());
    self.buf.to_vec()
  }
}

#[derive(Debug)]
pub struct Response {
  pub packet_type: u8,
  pub id: u32,
  pub payload: Vec<u8>,
}

impl Response {
  pub fn into_status(self) -> VResult<()> {
    self.status()
  }

  pub fn status(&self) -> VResult<()> {
    if self.packet_type != SSH_FXP_STATUS {
      return Err(VError::Protocol(format!(
        "expected STATUS, got packet type {}",
        self.packet_type
      )));
    }
    let mut reader = Reader::new(&self.payload);
    let code = reader.u32()?;
    let message = reader.string().unwrap_or_default();
    status_to_result(code, &message)
  }

  pub fn unexpected(&self) -> VError {
    self.status().err().unwrap_or_else(|| {
      VError::Protocol(format!("unexpected SFTP packet type {}", self.packet_type))
    })
  }

  pub fn is_eof(&self) -> bool {
    if self.packet_type != SSH_FXP_STATUS {
      return false;
    }
    matches!(Reader::new(&self.payload).u32(), Ok(SSH_FX_EOF))
  }
}

pub fn status_to_result(code: u32, message: &str) -> VResult<()> {
  match code {
    SSH_FX_OK => Ok(()),
    SSH_FX_EOF => Err(VError::Protocol("EOF".into())),
    SSH_FX_NO_SUCH_FILE => Err(VError::NotFound(message.to_string())),
    SSH_FX_PERMISSION_DENIED => Err(VError::PermissionDenied(message.to_string())),
    SSH_FX_FAILURE => Err(VError::Protocol(if message.is_empty() {
      "operation failed".into()
    } else {
      message.to_string()
    })),
    SSH_FX_BAD_MESSAGE => Err(VError::Protocol(format!("bad message: {message}"))),
    SSH_FX_NO_CONNECTION | SSH_FX_CONNECTION_LOST => {
      Err(VError::ConnectionLost(message.to_string()))
    }
    SSH_FX_OP_UNSUPPORTED => Err(VError::Unsupported(message.to_string())),
    other => Err(VError::Protocol(format!("SFTP status {other}: {message}"))),
  }
}
