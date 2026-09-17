use crate::protocol::ftp::client::{EncodingMode, TransferMode};
use crate::protocol::Protocol;
use crate::transfer::ConflictPolicy;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LogonType {
  Anonymous,
  #[default]
  Normal,
  Ask,
  Interactive,
  KeyFile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ServerType {
  #[default]
  Auto,
  Unix,
  Windows,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct SftpSettings {
  pub key_path: String,
  pub use_agent: bool,
  pub proxy_jump: String,
  pub extra_options: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SiteConfig {
  pub id: String,
  pub name: String,
  pub protocol: Protocol,
  pub host: String,
  pub port: u16,
  pub logon_type: LogonType,
  pub user: String,
  pub color: Option<String>,
  pub comments: String,
  pub local_dir: String,
  pub remote_dir: String,
  pub sync_browsing: bool,
  pub server_type: ServerType,
  pub bypass_proxy: bool,
  pub transfer_mode: TransferMode,
  pub ascii_mode: bool,
  pub max_connections: u32,
  pub conflict_policy: Option<ConflictPolicy>,
  pub encoding: EncodingMode,
  pub sftp: SftpSettings,
  pub keepalive_secs: u32,
  pub timeout_secs: u32,
}

impl Default for SiteConfig {
  fn default() -> Self {
    Self {
      id: uuid::Uuid::new_v4().to_string(),
      name: String::new(),
      protocol: Protocol::Sftp,
      host: String::new(),
      port: Protocol::Sftp.default_port(),
      logon_type: LogonType::Normal,
      user: String::new(),
      color: None,
      comments: String::new(),
      local_dir: String::new(),
      remote_dir: String::new(),
      sync_browsing: false,
      server_type: ServerType::Auto,
      bypass_proxy: false,
      transfer_mode: TransferMode::Default,
      ascii_mode: false,
      max_connections: 2,
      conflict_policy: None,
      encoding: EncodingMode::Auto,
      sftp: SftpSettings::default(),
      keepalive_secs: 30,
      timeout_secs: 30,
    }
  }
}

impl SiteConfig {
  pub fn effective_port(&self) -> u16 {
    if self.port == 0 {
      self.protocol.default_port()
    } else {
      self.port
    }
  }

  pub fn display_name(&self) -> String {
    if self.name.trim().is_empty() {
      format!("{}:{}", self.host, self.effective_port())
    } else {
      self.name.clone()
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Bookmark {
  pub id: String,
  pub name: String,
  pub local_dir: String,
  pub remote_dir: String,
  pub sync_browsing: bool,
}

impl Default for Bookmark {
  fn default() -> Self {
    Self {
      id: uuid::Uuid::new_v4().to_string(),
      name: String::new(),
      local_dir: String::new(),
      remote_dir: String::new(),
      sync_browsing: false,
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SiteNode {
  #[serde(rename_all = "camelCase")]
  Folder {
    id: String,
    name: String,
    children: Vec<SiteNode>,
  },
  #[serde(rename_all = "camelCase")]
  Site {
    site: SiteConfig,
    bookmarks: Vec<Bookmark>,
  },
}

impl SiteNode {
  pub fn folder(name: impl Into<String>, children: Vec<SiteNode>) -> Self {
    SiteNode::Folder {
      id: uuid::Uuid::new_v4().to_string(),
      name: name.into(),
      children,
    }
  }

  pub fn site(site: SiteConfig) -> Self {
    SiteNode::Site {
      site,
      bookmarks: Vec::new(),
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct SiteTree {
  pub root: Vec<SiteNode>,
}

impl SiteTree {
  pub fn all_sites(&self) -> Vec<&SiteConfig> {
    fn walk<'a>(nodes: &'a [SiteNode], out: &mut Vec<&'a SiteConfig>) {
      for node in nodes {
        match node {
          SiteNode::Folder { children, .. } => walk(children, out),
          SiteNode::Site { site, .. } => out.push(site),
        }
      }
    }
    let mut out = Vec::new();
    walk(&self.root, &mut out);
    out
  }

  pub fn site_ids(&self) -> Vec<String> {
    self.all_sites().into_iter().map(|s| s.id.clone()).collect()
  }
}
