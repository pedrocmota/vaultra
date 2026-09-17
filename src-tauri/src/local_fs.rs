use crate::error::{VError, VResult};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
const FILE_ATTRIBUTE_READONLY: u32 = 0x1;
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalKind {
  File,
  Dir,
  Symlink,
  Junction,
  Shortcut,
  Drive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalEntry {
  pub name: String,
  pub path: String,
  pub kind: LocalKind,
  pub size: u64,
  pub mtime: Option<i64>,
  pub hidden: bool,
  pub readonly: bool,
  pub link_target: Option<String>,
  pub link_broken: bool,
  pub target_is_dir: Option<bool>,
}

impl LocalEntry {
  pub fn is_dir_like(&self) -> bool {
    matches!(self.kind, LocalKind::Dir | LocalKind::Drive) || self.target_is_dir == Some(true)
  }
}

pub fn home_dir() -> String {
  dirs::home_dir()
    .map(|p| p.to_string_lossy().into_owned())
    .unwrap_or_else(|| "C:\\".into())
}

pub fn list_drives() -> Vec<LocalEntry> {
  drive_letters()
    .into_iter()
    .map(|letter| {
      let path = format!("{letter}:\\");
      LocalEntry {
        name: path.clone(),
        path,
        kind: LocalKind::Drive,
        size: 0,
        mtime: None,
        hidden: false,
        readonly: false,
        link_target: None,
        link_broken: false,
        target_is_dir: Some(true),
      }
    })
    .collect()
}

#[cfg(windows)]
fn drive_letters() -> Vec<char> {
  let mask = unsafe { windows::Win32::Storage::FileSystem::GetLogicalDrives() };
  (0..26)
    .filter(|i| mask & (1 << i) != 0)
    .map(|i| (b'A' + i as u8) as char)
    .collect()
}

#[cfg(not(windows))]
fn drive_letters() -> Vec<char> {
  Vec::new()
}

pub fn list_dir(path: &str, show_hidden: bool) -> VResult<Vec<LocalEntry>> {
  let dir = Path::new(path);
  let read = std::fs::read_dir(dir).map_err(|e| VError::from(e))?;
  let mut entries = Vec::new();
  for item in read.flatten() {
    let entry_path = item.path();
    let Some(entry) = describe(&entry_path) else {
      continue;
    };
    if entry.hidden && !show_hidden {
      continue;
    }
    entries.push(entry);
  }
  Ok(entries)
}

pub fn describe(path: &Path) -> Option<LocalEntry> {
  let meta = std::fs::symlink_metadata(path).ok()?;
  let name = path.file_name()?.to_string_lossy().into_owned();
  let attributes = file_attributes(&meta);
  let hidden = attributes & FILE_ATTRIBUTE_HIDDEN != 0 || name.starts_with('.');
  let readonly = attributes & FILE_ATTRIBUTE_READONLY != 0;
  let mtime = meta
    .modified()
    .ok()
    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
    .map(|d| d.as_secs() as i64);
  let is_reparse = attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0;
  let mut entry = LocalEntry {
    name: name.clone(),
    path: path.to_string_lossy().into_owned(),
    kind: if meta.is_dir() {
      LocalKind::Dir
    } else {
      LocalKind::File
    },
    size: if meta.is_dir() { 0 } else { meta.len() },
    mtime,
    hidden,
    readonly,
    link_target: None,
    link_broken: false,
    target_is_dir: None,
  };
  if meta.file_type().is_symlink() || is_reparse {
    entry.kind = if meta.file_type().is_symlink() {
      LocalKind::Symlink
    } else {
      LocalKind::Junction
    };
    annotate_link(&mut entry, path, std::fs::read_link(path).ok());
  } else if name.to_ascii_lowercase().ends_with(".lnk") {
    entry.kind = LocalKind::Shortcut;
    annotate_link(&mut entry, path, shortcut_target(path));
  }
  Some(entry)
}

fn annotate_link(entry: &mut LocalEntry, link_path: &Path, target: Option<PathBuf>) {
  let Some(target) = target else {
    entry.link_broken = true;
    return;
  };
  let resolved = if target.is_absolute() {
    target.clone()
  } else {
    link_path
      .parent()
      .map(|p| p.join(&target))
      .unwrap_or(target.clone())
  };
  entry.link_target = Some(target.to_string_lossy().into_owned());
  match std::fs::metadata(&resolved) {
    Ok(meta) => {
      entry.target_is_dir = Some(meta.is_dir());
      if !meta.is_dir() {
        entry.size = meta.len();
      }
    }
    Err(_) => {
      entry.link_broken = true;
      entry.target_is_dir = Some(false);
    }
  }
}

fn shortcut_target(path: &Path) -> Option<PathBuf> {
  let link = lnk::ShellLink::open(path).ok()?;
  let info = link.link_info().as_ref();
  if let Some(base) = info.and_then(|i| i.local_base_path().clone()) {
    let suffix = info
      .map(|i| i.common_path_suffix().clone())
      .unwrap_or_default();
    return Some(PathBuf::from(format!("{base}{suffix}")));
  }
  let relative = link.relative_path().clone()?;
  Some(PathBuf::from(relative))
}

#[cfg(windows)]
fn file_attributes(meta: &std::fs::Metadata) -> u32 {
  use std::os::windows::fs::MetadataExt;
  meta.file_attributes()
}

#[cfg(not(windows))]
fn file_attributes(_meta: &std::fs::Metadata) -> u32 {
  0
}

pub fn resolve_link(path: &str) -> VResult<String> {
  let p = Path::new(path);
  if p
    .extension()
    .map(|e| e.eq_ignore_ascii_case("lnk"))
    .unwrap_or(false)
  {
    return shortcut_target(p)
      .map(|t| t.to_string_lossy().into_owned())
      .ok_or_else(|| VError::NotFound("shortcut target unavailable".into()));
  }
  let target = std::fs::read_link(p)?;
  let resolved = if target.is_absolute() {
    target
  } else {
    p.parent().map(|d| d.join(&target)).unwrap_or(target)
  };
  Ok(strip_verbatim(&resolved.to_string_lossy()))
}

pub fn strip_verbatim(path: &str) -> String {
  path.strip_prefix(r"\\?\").unwrap_or(path).to_string()
}

pub fn mkdir(path: &str) -> VResult<()> {
  std::fs::create_dir(path).map_err(VError::from)
}

pub fn create_file(path: &str) -> VResult<()> {
  std::fs::OpenOptions::new()
    .write(true)
    .create_new(true)
    .open(path)
    .map(drop)
    .map_err(VError::from)
}

pub fn rename(from: &str, to: &str) -> VResult<()> {
  std::fs::rename(from, to).map_err(VError::from)
}

pub fn delete(path: &str) -> VResult<()> {
  let meta = std::fs::symlink_metadata(path)?;
  if meta.is_dir() && !meta.file_type().is_symlink() {
    std::fs::remove_dir_all(path).map_err(VError::from)
  } else if meta.is_dir() {
    std::fs::remove_dir(path).map_err(VError::from)
  } else {
    std::fs::remove_file(path).map_err(VError::from)
  }
}

pub fn copy_tree(source: &str, destination: &str, overwrite: bool) -> VResult<()> {
  let from = Path::new(source);
  let to = Path::new(destination);
  if is_same_or_inside(from, to) {
    return Err(VError::Io(format!(
      "cannot copy {source} into itself or one of its subfolders"
    )));
  }
  let meta = std::fs::metadata(from)?;
  if meta.is_dir() {
    copy_dir(from, to, overwrite)
  } else {
    copy_file(from, to, overwrite)
  }
}

fn is_same_or_inside(source: &Path, destination: &Path) -> bool {
  let Ok(source) = std::fs::canonicalize(source) else {
    return false;
  };
  let Some(existing) = destination.ancestors().find(|c| c.exists()) else {
    return false;
  };
  let Ok(base) = std::fs::canonicalize(existing) else {
    return false;
  };
  let suffix = destination.strip_prefix(existing).unwrap_or(Path::new(""));
  let source_text = comparable(&source);
  let target_text = comparable(&base.join(suffix));
  target_text == source_text || target_text.starts_with(&format!("{source_text}\\"))
}

fn comparable(path: &Path) -> String {
  strip_verbatim(&path.to_string_lossy())
    .trim_end_matches('\\')
    .to_lowercase()
}

fn copy_file(from: &Path, to: &Path, overwrite: bool) -> VResult<()> {
  if to.exists() && !overwrite {
    return Err(VError::Io(format!("{} already exists", to.display())));
  }
  if let Some(parent) = to.parent() {
    std::fs::create_dir_all(parent)?;
  }
  std::fs::copy(from, to)?;
  Ok(())
}

fn copy_dir(from: &Path, to: &Path, overwrite: bool) -> VResult<()> {
  std::fs::create_dir_all(to)?;
  for entry in std::fs::read_dir(from)? {
    let entry = entry?;
    let target = to.join(entry.file_name());
    let source = entry.path();
    let meta = match std::fs::metadata(&source) {
      Ok(meta) => meta,
      Err(_) => continue,
    };
    if meta.is_dir() {
      copy_dir(&source, &target, overwrite)?;
    } else {
      copy_file(&source, &target, overwrite)?;
    }
  }
  Ok(())
}

pub fn parent_of(path: &str) -> Option<String> {
  let p = Path::new(path);
  let parent = p.parent()?;
  let text = parent.to_string_lossy().into_owned();
  Some(if text.is_empty() {
    text
  } else if text.ends_with(':') {
    format!("{text}\\")
  } else {
    text
  })
}

pub fn join(base: &str, name: &str) -> String {
  Path::new(base).join(name).to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn copies_nested_folders() {
    let root = tempfile::tempdir().expect("tempdir");
    let source = root.path().join("src");
    std::fs::create_dir_all(source.join("inner")).unwrap();
    std::fs::write(source.join("a.txt"), b"alpha").unwrap();
    std::fs::write(source.join("inner").join("b.txt"), b"beta").unwrap();
    let destination = root.path().join("dst");
    copy_tree(
      &source.to_string_lossy(),
      &destination.to_string_lossy(),
      false,
    )
    .unwrap();
    assert_eq!(std::fs::read(destination.join("a.txt")).unwrap(), b"alpha");
    assert_eq!(
      std::fs::read(destination.join("inner").join("b.txt")).unwrap(),
      b"beta"
    );
  }

  #[test]
  fn refuses_copy_into_itself() {
    let root = tempfile::tempdir().expect("tempdir");
    let source = root.path().join("src");
    std::fs::create_dir_all(&source).unwrap();
    let inside = source.join("copy");
    let result = copy_tree(&source.to_string_lossy(), &inside.to_string_lossy(), false);
    assert!(result.is_err());
  }

  #[test]
  fn refuses_overwrite_unless_requested() {
    let root = tempfile::tempdir().expect("tempdir");
    let source = root.path().join("a.txt");
    let destination = root.path().join("b.txt");
    std::fs::write(&source, b"new").unwrap();
    std::fs::write(&destination, b"old").unwrap();
    let denied = copy_tree(
      &source.to_string_lossy(),
      &destination.to_string_lossy(),
      false,
    );
    assert!(denied.is_err());
    copy_tree(
      &source.to_string_lossy(),
      &destination.to_string_lossy(),
      true,
    )
    .unwrap();
    assert_eq!(std::fs::read(&destination).unwrap(), b"new");
  }
}
