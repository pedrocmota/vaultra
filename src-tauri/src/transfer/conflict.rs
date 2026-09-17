use super::Direction;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConflictPolicy {
  #[default]
  Ask,
  Overwrite,
  Skip,
  Rename,
  Resume,
  OverwriteIfNewer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictAction {
  Overwrite,
  Skip,
  Rename,
  Resume,
  OverwriteIfNewer,
}

impl ConflictAction {
  pub fn as_policy(self) -> ConflictPolicy {
    match self {
      ConflictAction::Overwrite => ConflictPolicy::Overwrite,
      ConflictAction::Skip => ConflictPolicy::Skip,
      ConflictAction::Rename => ConflictPolicy::Rename,
      ConflictAction::Resume => ConflictPolicy::Resume,
      ConflictAction::OverwriteIfNewer => ConflictPolicy::OverwriteIfNewer,
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictAnswer {
  pub action: ConflictAction,
  pub apply_to_all: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictPrompt {
  pub item_id: String,
  pub session_id: String,
  pub direction: Direction,
  pub source_path: String,
  pub source_size: u64,
  pub source_mtime: Option<i64>,
  pub target_path: String,
  pub target_size: u64,
  pub target_mtime: Option<i64>,
}

#[derive(Debug, Clone, Copy)]
pub struct FileFacts {
  pub size: u64,
  pub mtime: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
  Write { offset: u64, rename: bool },
  Skip,
}

pub fn resolve(policy: ConflictPolicy, source: FileFacts, existing: FileFacts) -> Resolution {
  match policy {
    ConflictPolicy::Ask | ConflictPolicy::Overwrite => Resolution::Write {
      offset: 0,
      rename: false,
    },
    ConflictPolicy::Skip => Resolution::Skip,
    ConflictPolicy::Rename => Resolution::Write {
      offset: 0,
      rename: true,
    },
    ConflictPolicy::Resume => match existing.size.cmp(&source.size) {
      std::cmp::Ordering::Less => Resolution::Write {
        offset: existing.size,
        rename: false,
      },
      std::cmp::Ordering::Equal => Resolution::Skip,
      std::cmp::Ordering::Greater => Resolution::Write {
        offset: 0,
        rename: false,
      },
    },
    ConflictPolicy::OverwriteIfNewer => match (source.mtime, existing.mtime) {
      (Some(s), Some(e)) if s > e => Resolution::Write {
        offset: 0,
        rename: false,
      },
      (Some(_), None) => Resolution::Write {
        offset: 0,
        rename: false,
      },
      _ => Resolution::Skip,
    },
  }
}

pub fn unique_name(name: &str, exists: impl Fn(&str) -> bool) -> String {
  let (stem, ext) = match name.rsplit_once('.') {
    Some((s, e)) if !s.is_empty() => (s.to_string(), format!(".{e}")),
    _ => (name.to_string(), String::new()),
  };
  (1..10_000)
    .map(|n| format!("{stem} ({n}){ext}"))
    .find(|candidate| !exists(candidate))
    .unwrap_or_else(|| format!("{stem} ({}){ext}", uuid::Uuid::new_v4().simple()))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn resumes_when_partial() {
    let r = resolve(
      ConflictPolicy::Resume,
      FileFacts {
        size: 100,
        mtime: None,
      },
      FileFacts {
        size: 40,
        mtime: None,
      },
    );
    assert_eq!(
      r,
      Resolution::Write {
        offset: 40,
        rename: false
      }
    );
    let r = resolve(
      ConflictPolicy::Resume,
      FileFacts {
        size: 100,
        mtime: None,
      },
      FileFacts {
        size: 100,
        mtime: None,
      },
    );
    assert_eq!(r, Resolution::Skip);
  }

  #[test]
  fn overwrite_if_newer_compares_mtime() {
    let newer = resolve(
      ConflictPolicy::OverwriteIfNewer,
      FileFacts {
        size: 1,
        mtime: Some(20),
      },
      FileFacts {
        size: 1,
        mtime: Some(10),
      },
    );
    assert_eq!(
      newer,
      Resolution::Write {
        offset: 0,
        rename: false
      }
    );
    let older = resolve(
      ConflictPolicy::OverwriteIfNewer,
      FileFacts {
        size: 1,
        mtime: Some(5),
      },
      FileFacts {
        size: 1,
        mtime: Some(10),
      },
    );
    assert_eq!(older, Resolution::Skip);
  }

  #[test]
  fn unique_names_increment() {
    let taken = ["a.txt".to_string(), "a (1).txt".to_string()];
    assert_eq!(
      unique_name("a.txt", |n| taken.contains(&n.to_string())),
      "a (2).txt"
    );
    assert_eq!(unique_name("noext", |_| false), "noext (1)");
  }
}
