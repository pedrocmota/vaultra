use crate::error::VResult;
use crate::local_fs;
use crate::protocol::{self, EntryKind};
use crate::session::Session;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;

const MTIME_TOLERANCE_SECS: i64 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffKind {
  Equal,
  LocalNewer,
  RemoteNewer,
  LocalOnly,
  RemoteOnly,
  Different,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffEntry {
  pub relative_path: String,
  pub is_dir: bool,
  pub kind: DiffKind,
  pub local_size: Option<u64>,
  pub remote_size: Option<u64>,
  pub local_mtime: Option<i64>,
  pub remote_mtime: Option<i64>,
  pub local_path: String,
  pub remote_path: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncRequest {
  pub session_id: String,
  pub local_dir: String,
  pub remote_dir: String,
  #[serde(default)]
  pub excludes: Vec<String>,
}

#[derive(Debug, Clone)]
struct Facts {
  size: u64,
  mtime: Option<i64>,
  is_dir: bool,
  absolute: String,
}

struct ExcludeFilter {
  patterns: Vec<String>,
}

impl ExcludeFilter {
  fn new(patterns: &[String]) -> Self {
    Self {
      patterns: patterns
        .iter()
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect(),
    }
  }

  fn matches(&self, relative: &str, name: &str) -> bool {
    self
      .patterns
      .iter()
      .any(|p| glob_match::glob_match(p, relative) || glob_match::glob_match(p, name))
  }
}

pub async fn compare(session: Arc<Session>, request: SyncRequest) -> VResult<Vec<DiffEntry>> {
  let filter = ExcludeFilter::new(&request.excludes);
  let local = collect_local(&request.local_dir, &filter)?;
  let remote = collect_remote(&session, &request.remote_dir, &filter).await?;
  let mut keys: Vec<&String> = local.keys().chain(remote.keys()).collect();
  keys.sort();
  keys.dedup();
  let mut diff = Vec::with_capacity(keys.len());
  for key in keys {
    let l = local.get(key);
    let r = remote.get(key);
    let kind = classify(l, r);
    diff.push(DiffEntry {
      relative_path: key.clone(),
      is_dir: l.map(|f| f.is_dir).or(r.map(|f| f.is_dir)).unwrap_or(false),
      kind,
      local_size: l.map(|f| f.size),
      remote_size: r.map(|f| f.size),
      local_mtime: l.and_then(|f| f.mtime),
      remote_mtime: r.and_then(|f| f.mtime),
      local_path: l
        .map(|f| f.absolute.clone())
        .unwrap_or_else(|| local_fs::join(&request.local_dir, &key.replace('/', "\\"))),
      remote_path: r
        .map(|f| f.absolute.clone())
        .unwrap_or_else(|| protocol::join(&request.remote_dir, key)),
    });
  }
  Ok(diff)
}

fn classify(local: Option<&Facts>, remote: Option<&Facts>) -> DiffKind {
  match (local, remote) {
    (Some(_), None) => DiffKind::LocalOnly,
    (None, Some(_)) => DiffKind::RemoteOnly,
    (Some(l), Some(r)) => {
      if l.is_dir || r.is_dir {
        return if l.is_dir == r.is_dir {
          DiffKind::Equal
        } else {
          DiffKind::Different
        };
      }
      if l.size != r.size {
        return match (l.mtime, r.mtime) {
          (Some(lm), Some(rm)) if lm > rm => DiffKind::LocalNewer,
          (Some(lm), Some(rm)) if rm > lm => DiffKind::RemoteNewer,
          _ => DiffKind::Different,
        };
      }
      match (l.mtime, r.mtime) {
        (Some(lm), Some(rm)) if (lm - rm).abs() <= MTIME_TOLERANCE_SECS => DiffKind::Equal,
        (Some(lm), Some(rm)) if lm > rm => DiffKind::LocalNewer,
        (Some(_), Some(_)) => DiffKind::RemoteNewer,
        _ => DiffKind::Equal,
      }
    }
    (None, None) => DiffKind::Equal,
  }
}

fn collect_local(root: &str, filter: &ExcludeFilter) -> VResult<BTreeMap<String, Facts>> {
  let mut out = BTreeMap::new();
  let mut stack = vec![(root.to_string(), String::new())];
  while let Some((dir, prefix)) = stack.pop() {
    for entry in local_fs::list_dir(&dir, true)? {
      let relative = if prefix.is_empty() {
        entry.name.clone()
      } else {
        format!("{prefix}/{}", entry.name)
      };
      if filter.matches(&relative, &entry.name) {
        continue;
      }
      let is_dir = entry.is_dir_like();
      out.insert(
        relative.clone(),
        Facts {
          size: entry.size,
          mtime: entry.mtime,
          is_dir,
          absolute: entry.path.clone(),
        },
      );
      if is_dir {
        stack.push((entry.path, relative));
      }
    }
  }
  Ok(out)
}

async fn collect_remote(
  session: &Arc<Session>,
  root: &str,
  filter: &ExcludeFilter,
) -> VResult<BTreeMap<String, Facts>> {
  let mut out = BTreeMap::new();
  let mut stack = vec![(root.to_string(), String::new())];
  while let Some((dir, prefix)) = stack.pop() {
    for entry in session.list(&dir, false).await? {
      let relative = if prefix.is_empty() {
        entry.name.clone()
      } else {
        format!("{prefix}/{}", entry.name)
      };
      if filter.matches(&relative, &entry.name) {
        continue;
      }
      if entry.kind == EntryKind::Symlink && entry.link_broken {
        continue;
      }
      let is_dir = entry.is_dir_like();
      out.insert(
        relative.clone(),
        Facts {
          size: entry.size,
          mtime: entry.mtime,
          is_dir,
          absolute: entry.path.clone(),
        },
      );
      if is_dir {
        stack.push((entry.path, relative));
      }
    }
  }
  Ok(out)
}
