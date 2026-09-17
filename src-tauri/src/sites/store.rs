use super::model::{SiteConfig, SiteTree};
use crate::credentials::CredentialStore;
use crate::error::{VError, VResult};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

const MAX_RECENT: usize = 10;

pub struct SiteStore {
  file: PathBuf,
  recent_file: PathBuf,
  tree: parking_lot::RwLock<SiteTree>,
  recent: parking_lot::RwLock<Vec<RecentConnection>>,
  credentials: Arc<dyn CredentialStore>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentConnection {
  pub site: SiteConfig,
  pub last_used: i64,
}

impl SiteStore {
  pub fn load(credentials: Arc<dyn CredentialStore>) -> Self {
    let file = crate::paths::sites_file();
    let recent_file = crate::paths::recent_file();
    let tree = read_json(&file).unwrap_or_default();
    let recent = read_json(&recent_file).unwrap_or_default();
    Self {
      file,
      recent_file,
      tree: parking_lot::RwLock::new(tree),
      recent: parking_lot::RwLock::new(recent),
      credentials,
    }
  }

  pub fn tree(&self) -> SiteTree {
    self.tree.read().clone()
  }

  pub fn save_tree(&self, tree: SiteTree) -> VResult<()> {
    let removed: Vec<String> = {
      let current = self.tree.read();
      let kept = tree.site_ids();
      current
        .site_ids()
        .into_iter()
        .filter(|id| !kept.contains(id))
        .collect()
    };
    for id in removed {
      let _ = self.credentials.delete(&id);
    }
    write_json(&self.file, &tree)?;
    *self.tree.write() = tree;
    Ok(())
  }

  pub fn password(&self, site_id: &str) -> VResult<Option<String>> {
    self.credentials.get(site_id)
  }

  pub fn set_password(&self, site_id: &str, user: &str, password: &str) -> VResult<()> {
    if password.is_empty() {
      self.credentials.delete(site_id)
    } else {
      self.credentials.set(site_id, user, password)
    }
  }

  pub fn recent(&self) -> Vec<RecentConnection> {
    self.recent.read().clone()
  }

  pub fn remember_recent(&self, site: &SiteConfig) -> VResult<()> {
    let mut recent = self.recent.write();
    recent.retain(|r| {
      !(r.site.host == site.host
        && r.site.port == site.port
        && r.site.user == site.user
        && r.site.protocol == site.protocol)
    });
    recent.insert(
      0,
      RecentConnection {
        site: site.clone(),
        last_used: chrono::Utc::now().timestamp(),
      },
    );
    recent.truncate(MAX_RECENT);
    write_json(&self.recent_file, &*recent)
  }

  pub fn clear_recent(&self) -> VResult<()> {
    self.recent.write().clear();
    write_json(&self.recent_file, &Vec::<RecentConnection>::new())
  }
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &PathBuf) -> Option<T> {
  let text = std::fs::read_to_string(path).ok()?;
  serde_json::from_str(&text).ok()
}

fn write_json<T: Serialize>(path: &PathBuf, value: &T) -> VResult<()> {
  let text = serde_json::to_string_pretty(value)?;
  let tmp = path.with_extension("json.tmp");
  std::fs::write(&tmp, text)
    .map_err(|e| VError::Io(format!("failed to write {}: {e}", path.display())))?;
  std::fs::rename(&tmp, path)
    .map_err(|e| VError::Io(format!("failed to replace {}: {e}", path.display())))?;
  Ok(())
}
