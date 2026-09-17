use crate::error::{VError, VResult};
use crate::transfer::ConflictPolicy;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
  #[default]
  Dark,
  Light,
  System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SymlinkDownload {
  #[default]
  Follow,
  CopyLink,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AppSettings {
  pub theme: Theme,
  pub language: String,
  pub default_conflict_policy: ConflictPolicy,
  pub cache_ttl_secs: u64,
  pub default_local_dir: String,
  pub show_hidden: bool,
  pub local_pane_left: bool,
  pub confirm_delete: bool,
  pub symlink_download: SymlinkDownload,
  pub max_retries: u32,
  pub retry_backoff_ms: u64,
  pub keepalive_secs: u32,
  pub timeout_secs: u32,
  pub bandwidth_limit_kbps: u64,
  pub verify_hash: bool,
  pub max_successful_history: usize,
  pub system_icons: bool,
}

impl Default for AppSettings {
  fn default() -> Self {
    Self {
      theme: Theme::Dark,
      language: "pt-BR".into(),
      default_conflict_policy: ConflictPolicy::Ask,
      cache_ttl_secs: 60,
      default_local_dir: String::new(),
      show_hidden: false,
      local_pane_left: true,
      confirm_delete: true,
      symlink_download: SymlinkDownload::Follow,
      max_retries: 3,
      retry_backoff_ms: 2000,
      keepalive_secs: 30,
      timeout_secs: 30,
      bandwidth_limit_kbps: 0,
      verify_hash: false,
      max_successful_history: 500,
      system_icons: true,
    }
  }
}

pub struct SettingsStore {
  file: PathBuf,
  current: parking_lot::RwLock<AppSettings>,
}

impl SettingsStore {
  pub fn load() -> Self {
    let file = crate::paths::settings_file();
    let current = std::fs::read_to_string(&file)
      .ok()
      .and_then(|text| serde_json::from_str(&text).ok())
      .unwrap_or_default();
    Self {
      file,
      current: parking_lot::RwLock::new(current),
    }
  }

  pub fn get(&self) -> AppSettings {
    self.current.read().clone()
  }

  pub fn save(&self, settings: AppSettings) -> VResult<()> {
    let text = serde_json::to_string_pretty(&settings)?;
    std::fs::write(&self.file, text)
      .map_err(|e| VError::Io(format!("failed to save settings: {e}")))?;
    *self.current.write() = settings;
    Ok(())
  }
}
