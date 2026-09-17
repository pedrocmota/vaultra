pub mod ftp;
pub mod path;
pub mod sftp;

use crate::error::VResult;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub use path::{basename, join, parent, resolve_link_target};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
  Ftp,
  FtpsExplicit,
  FtpsImplicit,
  #[default]
  Sftp,
}

impl Protocol {
  pub fn default_port(self) -> u16 {
    match self {
      Protocol::Ftp | Protocol::FtpsExplicit => 21,
      Protocol::FtpsImplicit => 990,
      Protocol::Sftp => 22,
    }
  }

  pub fn is_ftp_family(self) -> bool {
    !matches!(self, Protocol::Sftp)
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
  File,
  Dir,
  Symlink,
  Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteEntry {
  pub name: String,
  pub path: String,
  pub kind: EntryKind,
  pub size: u64,
  pub mtime: Option<i64>,
  pub permissions: Option<u32>,
  pub owner: Option<String>,
  pub group: Option<String>,
  pub link_target: Option<String>,
  pub link_broken: bool,
  pub target_is_dir: Option<bool>,
}

impl RemoteEntry {
  pub fn plain(path: &str, kind: EntryKind, size: u64, mtime: Option<i64>) -> Self {
    Self {
      name: basename(path),
      path: path.to_string(),
      kind,
      size,
      mtime,
      permissions: None,
      owner: None,
      group: None,
      link_target: None,
      link_broken: false,
      target_is_dir: None,
    }
  }

  pub fn is_dir_like(&self) -> bool {
    self.kind == EntryKind::Dir
      || (self.kind == EntryKind::Symlink && self.target_is_dir == Some(true))
  }
}

#[async_trait]
pub trait RemoteReader: Send {
  async fn read(&mut self, buf: &mut [u8]) -> VResult<usize>;
  async fn finish(&mut self) -> VResult<()>;
}

#[async_trait]
pub trait RemoteWriter: Send {
  async fn write(&mut self, buf: &[u8]) -> VResult<()>;
  async fn finish(&mut self) -> VResult<()>;
}

#[async_trait]
pub trait RemoteClient: Send + Sync {
  fn supports_multiplexing(&self) -> bool;
  async fn initial_dir(&self) -> VResult<String>;
  async fn realpath(&self, path: &str) -> VResult<String>;
  async fn list(&self, path: &str) -> VResult<Vec<RemoteEntry>>;
  async fn stat(&self, path: &str) -> VResult<RemoteEntry>;
  async fn readlink(&self, path: &str) -> VResult<String>;
  async fn mkdir(&self, path: &str) -> VResult<()>;
  async fn rmdir(&self, path: &str) -> VResult<()>;
  async fn remove(&self, path: &str) -> VResult<()>;
  async fn rename(&self, from: &str, to: &str) -> VResult<()>;
  async fn chmod(&self, path: &str, mode: u32) -> VResult<()>;
  async fn touch(&self, path: &str) -> VResult<()>;
  async fn open_read(&self, path: &str, offset: u64) -> VResult<Box<dyn RemoteReader>>;
  async fn open_write(&self, path: &str, offset: u64) -> VResult<Box<dyn RemoteWriter>>;
  async fn keepalive(&self) -> VResult<()>;
  async fn disconnect(&self);

  fn supported_hashes(&self) -> Vec<String> {
    Vec::new()
  }

  async fn hash(&self, _path: &str, _algo: &str) -> VResult<Option<String>> {
    Ok(None)
  }
}
