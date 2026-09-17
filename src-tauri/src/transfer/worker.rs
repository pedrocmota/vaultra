use super::conflict::{
  resolve, unique_name, ConflictPolicy, ConflictPrompt, FileFacts, Resolution,
};
use super::queue::{Control, TransferQueue};
use super::{Direction, TransferItem, TransferStatus};
use crate::error::{VError, VResult};
use crate::local_fs;
use crate::protocol::{self, EntryKind, RemoteClient};
use crate::session::Session;
use sha2::Digest;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

const BUFFER_SIZE: usize = 128 * 1024;
const PROGRESS_INTERVAL: Duration = Duration::from_millis(200);

enum Outcome {
  Done,
  Skipped,
  Paused,
  Cancelled,
}

pub(super) async fn run(queue: Arc<TransferQueue>, id: String, control: Control) {
  let Some(item) = queue.get(&id) else { return };
  let session = match item
    .session_id
    .as_deref()
    .and_then(|s| queue.sessions.get(s).ok())
  {
    Some(session) => session,
    None => {
      queue.complete(
        &id,
        TransferStatus::Failed,
        Some("session is not connected".into()),
      );
      return;
    }
  };
  let result = if item.is_dir {
    expand_directory(&queue, &item, &session).await
  } else {
    transfer_file(&queue, &item, &session, &control).await
  };
  match result {
    Ok(Outcome::Done) => queue.complete(&id, TransferStatus::Done, None),
    Ok(Outcome::Skipped) => queue.complete(&id, TransferStatus::Skipped, None),
    Ok(Outcome::Paused) => queue.set_paused(&id),
    Ok(Outcome::Cancelled) => queue.remove(&id),
    Err(error) => handle_failure(&queue, &item, &session, error),
  }
}

fn handle_failure(queue: &TransferQueue, item: &TransferItem, session: &Session, error: VError) {
  let settings = queue.settings.get();
  let retryable = matches!(
    error,
    VError::ConnectionLost(_) | VError::Timeout(_) | VError::Io(_)
  );
  let attempts = item.attempts + 1;
  queue.update(&item.id, |i| i.attempts = attempts);
  session
    .log
    .error(format!("Transfer failed ({}): {error}", item.remote_path));
  if retryable && attempts <= settings.max_retries {
    let delay = Duration::from_millis(settings.retry_backoff_ms.max(250) * 2u64.pow(attempts - 1));
    session.log.status(format!(
      "Retrying in {:.0}s (attempt {attempts}/{})",
      delay.as_secs_f64(),
      settings.max_retries
    ));
    queue.schedule_retry(&item.id, error.to_string(), delay);
  } else {
    queue.complete(&item.id, TransferStatus::Failed, Some(error.to_string()));
  }
}

async fn expand_directory(
  queue: &Arc<TransferQueue>,
  item: &TransferItem,
  session: &Arc<Session>,
) -> VResult<Outcome> {
  let children = match item.direction {
    Direction::Download => expand_remote_directory(item, session).await?,
    Direction::Upload => expand_local_directory(item, session).await?,
  };
  queue.insert_children(&item.id, children);
  Ok(Outcome::Done)
}

fn child_of(
  parent: &TransferItem,
  local_path: String,
  remote_path: String,
  is_dir: bool,
  size: Option<u64>,
  mtime: Option<i64>,
) -> TransferItem {
  TransferItem {
    id: uuid::Uuid::new_v4().to_string(),
    session_id: parent.session_id.clone(),
    site_id: parent.site_id.clone(),
    site_name: parent.site_name.clone(),
    direction: parent.direction,
    local_path,
    remote_path,
    is_dir,
    size,
    source_mtime: mtime,
    transferred: 0,
    status: TransferStatus::Queued,
    priority: parent.priority,
    error: None,
    attempts: 0,
    created_at: chrono::Utc::now().timestamp_millis(),
    finished_at: None,
    speed_bps: 0,
    follow_symlink: parent.follow_symlink,
    conflict_policy: parent.conflict_policy,
    link_target: None,
    retry_after: None,
  }
}

async fn expand_remote_directory(
  item: &TransferItem,
  session: &Arc<Session>,
) -> VResult<Vec<TransferItem>> {
  std::fs::create_dir_all(&item.local_path)?;
  let entries = session.list(&item.remote_path, true).await?;
  let mut children = Vec::with_capacity(entries.len());
  for entry in entries {
    let local = local_fs::join(&item.local_path, &entry.name);
    match entry.kind {
      EntryKind::Dir => children.push(child_of(item, local, entry.path, true, None, None)),
      EntryKind::File => children.push(child_of(
        item,
        local,
        entry.path,
        false,
        Some(entry.size),
        entry.mtime,
      )),
      EntryKind::Symlink if entry.link_broken => {
        session
          .log
          .error(format!("Skipping broken symlink {}", entry.path));
      }
      EntryKind::Symlink if item.follow_symlink => {
        let is_dir = entry.target_is_dir.unwrap_or(false);
        children.push(child_of(
          item,
          local,
          entry.path,
          is_dir,
          (!is_dir).then_some(entry.size),
          entry.mtime,
        ));
      }
      EntryKind::Symlink => {
        let mut child = child_of(item, local, entry.path, false, Some(0), entry.mtime);
        child.link_target = entry.link_target.clone().or(Some(String::new()));
        children.push(child);
      }
      EntryKind::Other => {}
    }
  }
  Ok(children)
}

async fn expand_local_directory(
  item: &TransferItem,
  session: &Arc<Session>,
) -> VResult<Vec<TransferItem>> {
  let client = session.client();
  if let Err(error) = client.mkdir(&item.remote_path).await {
    if client
      .stat(&item.remote_path)
      .await
      .map(|e| e.kind != EntryKind::Dir)
      .unwrap_or(true)
    {
      return Err(error);
    }
  }
  session.invalidate(&item.remote_path);
  let entries = local_fs::list_dir(&item.local_path, true)?;
  let mut children = Vec::with_capacity(entries.len());
  for entry in entries {
    let remote = protocol::join(&item.remote_path, &entry.name);
    if entry.link_broken {
      session
        .log
        .error(format!("Skipping broken link {}", entry.path));
      continue;
    }
    if entry.is_dir_like() {
      children.push(child_of(item, entry.path, remote, true, None, None));
    } else {
      children.push(child_of(
        item,
        entry.path,
        remote,
        false,
        Some(entry.size),
        entry.mtime,
      ));
    }
  }
  Ok(children)
}

async fn copy_symlink(item: &TransferItem, session: &Arc<Session>) -> VResult<Outcome> {
  let client = session.client();
  let raw_target = match item.link_target.as_deref().filter(|t| !t.is_empty()) {
    Some(target) => target.to_string(),
    None => client.readlink(&item.remote_path).await?,
  };
  let resolved = protocol::resolve_link_target(&item.remote_path, &raw_target);
  let target_is_dir = client
    .stat(&resolved)
    .await
    .map(|entry| entry.kind == EntryKind::Dir)
    .unwrap_or(false);
  let local_target = raw_target.replace('/', "\\");
  let local_path = PathBuf::from(&item.local_path);
  if std::fs::symlink_metadata(&local_path).is_ok() {
    local_fs::delete(&item.local_path)?;
  }
  create_local_symlink(&local_target, &local_path, target_is_dir)?;
  session.log.status(format!(
    "Copied symlink {} -> {raw_target}",
    item.remote_path
  ));
  Ok(Outcome::Done)
}

#[cfg(windows)]
fn create_local_symlink(target: &str, link: &Path, is_dir: bool) -> VResult<()> {
  let result = if is_dir {
    std::os::windows::fs::symlink_dir(target, link)
  } else {
    std::os::windows::fs::symlink_file(target, link)
  };
  result.map_err(|e| {
    VError::PermissionDenied(format!(
      "cannot create local symlink (enable Developer Mode or run elevated): {e}"
    ))
  })
}

#[cfg(not(windows))]
fn create_local_symlink(target: &str, link: &Path, _is_dir: bool) -> VResult<()> {
  std::os::unix::fs::symlink(target, link).map_err(VError::from)
}

struct Plan {
  offset: u64,
  local_path: PathBuf,
  remote_path: String,
  source: FileFacts,
}

async fn transfer_file(
  queue: &Arc<TransferQueue>,
  item: &TransferItem,
  session: &Arc<Session>,
  control: &Control,
) -> VResult<Outcome> {
  if item.direction == Direction::Download && item.link_target.is_some() {
    return copy_symlink(item, session).await;
  }
  let lease = session.transfer_client().await?;
  let client = lease.client.clone();
  let plan = match plan_transfer(queue, item, session, client.as_ref()).await? {
    Some(plan) => plan,
    None => return Ok(Outcome::Skipped),
  };
  queue.update(&item.id, |i| {
    i.size = Some(plan.source.size);
    i.local_path = plan.local_path.to_string_lossy().into_owned();
    i.remote_path = plan.remote_path.clone();
  });
  let outcome = match item.direction {
    Direction::Download => download(queue, item, client.as_ref(), &plan, control).await?,
    Direction::Upload => upload(queue, item, client.as_ref(), &plan, control).await?,
  };
  if matches!(outcome, Outcome::Done) {
    verify(queue, item, client.as_ref(), &plan).await?;
    if item.direction == Direction::Upload {
      session.invalidate(&plan.remote_path);
    }
    session
      .log
      .status(format!("Transfer complete: {}", plan.remote_path));
  }
  Ok(outcome)
}

async fn plan_transfer(
  queue: &Arc<TransferQueue>,
  item: &TransferItem,
  session: &Arc<Session>,
  client: &dyn RemoteClient,
) -> VResult<Option<Plan>> {
  let local_path = PathBuf::from(&item.local_path);
  let remote_path = item.remote_path.clone();
  let (source, existing) = match item.direction {
    Direction::Download => {
      let remote = client.stat(&remote_path).await?;
      if remote.kind == EntryKind::Dir {
        return Err(VError::Protocol(format!("{remote_path} is a directory")));
      }
      (
        FileFacts {
          size: remote.size,
          mtime: remote.mtime,
        },
        local_facts(&local_path),
      )
    }
    Direction::Upload => {
      let meta = std::fs::metadata(&local_path)?;
      let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);
      let existing = match client.stat(&remote_path).await {
        Ok(entry) if entry.kind != EntryKind::Dir => Some(FileFacts {
          size: entry.size,
          mtime: entry.mtime,
        }),
        _ => None,
      };
      (
        FileFacts {
          size: meta.len(),
          mtime,
        },
        existing,
      )
    }
  };
  let Some(existing) = existing else {
    return Ok(Some(Plan {
      offset: 0,
      local_path,
      remote_path,
      source,
    }));
  };
  if item.transferred > 0 && existing.size <= source.size {
    return Ok(Some(Plan {
      offset: existing.size,
      local_path,
      remote_path,
      source,
    }));
  }
  let mut policy = queue.effective_policy(item, session);
  if policy == ConflictPolicy::Ask {
    let prompt = ConflictPrompt {
      item_id: item.id.clone(),
      session_id: session.id.clone(),
      direction: item.direction,
      source_path: match item.direction {
        Direction::Download => remote_path.clone(),
        Direction::Upload => item.local_path.clone(),
      },
      source_size: source.size,
      source_mtime: source.mtime,
      target_path: match item.direction {
        Direction::Download => item.local_path.clone(),
        Direction::Upload => remote_path.clone(),
      },
      target_size: existing.size,
      target_mtime: existing.mtime,
    };
    policy = queue.ask_conflict(prompt).await?.action.as_policy();
  }
  match resolve(policy, source, existing) {
    Resolution::Skip => Ok(None),
    Resolution::Write {
      offset,
      rename: false,
    } => Ok(Some(Plan {
      offset,
      local_path,
      remote_path,
      source,
    })),
    Resolution::Write {
      offset,
      rename: true,
    } => {
      let (local_path, remote_path) = match item.direction {
        Direction::Download => {
          let dir = local_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
          let name = local_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
          let fresh = unique_name(&name, |candidate| dir.join(candidate).exists());
          (dir.join(fresh), remote_path)
        }
        Direction::Upload => {
          let dir = protocol::parent(&remote_path);
          let siblings: Vec<String> = session
            .list(&dir, true)
            .await
            .map(|l| l.into_iter().map(|e| e.name).collect())
            .unwrap_or_default();
          let fresh = unique_name(&protocol::basename(&remote_path), |candidate| {
            siblings.iter().any(|s| s == candidate)
          });
          (local_path, protocol::join(&dir, &fresh))
        }
      };
      Ok(Some(Plan {
        offset,
        local_path,
        remote_path,
        source,
      }))
    }
  }
}

fn local_facts(path: &Path) -> Option<FileFacts> {
  let meta = std::fs::metadata(path).ok()?;
  if meta.is_dir() {
    return None;
  }
  let mtime = meta
    .modified()
    .ok()
    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
    .map(|d| d.as_secs() as i64);
  Some(FileFacts {
    size: meta.len(),
    mtime,
  })
}

struct ProgressTracker<'a> {
  queue: &'a TransferQueue,
  id: &'a str,
  size: u64,
  transferred: u64,
  window_start: Instant,
  window_bytes: u64,
  last_emit: Instant,
  speed: u64,
}

impl<'a> ProgressTracker<'a> {
  fn new(queue: &'a TransferQueue, id: &'a str, size: u64, offset: u64) -> Self {
    let now = Instant::now();
    Self {
      queue,
      id,
      size,
      transferred: offset,
      window_start: now,
      window_bytes: 0,
      last_emit: now,
      speed: 0,
    }
  }

  fn advance(&mut self, bytes: usize) {
    self.transferred += bytes as u64;
    self.window_bytes += bytes as u64;
    let now = Instant::now();
    if now.duration_since(self.last_emit) >= PROGRESS_INTERVAL {
      let elapsed = now
        .duration_since(self.window_start)
        .as_secs_f64()
        .max(0.001);
      self.speed = (self.window_bytes as f64 / elapsed) as u64;
      self
        .queue
        .record_progress(self.id, self.transferred, self.speed, Some(self.size));
      self.last_emit = now;
      if elapsed > 2.0 {
        self.window_start = now;
        self.window_bytes = 0;
      }
    }
  }

  fn finish(&self) {
    self
      .queue
      .record_progress(self.id, self.transferred, 0, Some(self.size));
  }
}

fn interrupted(control: &Control) -> Option<Outcome> {
  if !control.token.is_cancelled() {
    return None;
  }
  Some(if control.pausing.load(Ordering::Relaxed) {
    Outcome::Paused
  } else {
    Outcome::Cancelled
  })
}

async fn download(
  queue: &Arc<TransferQueue>,
  item: &TransferItem,
  client: &dyn RemoteClient,
  plan: &Plan,
  control: &Control,
) -> VResult<Outcome> {
  if let Some(parent) = plan.local_path.parent() {
    std::fs::create_dir_all(parent)?;
  }
  let mut file = tokio::fs::OpenOptions::new()
    .create(true)
    .write(true)
    .truncate(plan.offset == 0)
    .open(&plan.local_path)
    .await?;
  if plan.offset > 0 {
    file.set_len(plan.offset).await?;
    file.seek(std::io::SeekFrom::Start(plan.offset)).await?;
  }
  let mut reader = client.open_read(&plan.remote_path, plan.offset).await?;
  let mut progress = ProgressTracker::new(queue, &item.id, plan.source.size, plan.offset);
  let mut buffer = vec![0u8; BUFFER_SIZE];
  let outcome = loop {
    if let Some(outcome) = interrupted(control) {
      break outcome;
    }
    let n = reader.read(&mut buffer).await?;
    if n == 0 {
      break Outcome::Done;
    }
    file.write_all(&buffer[..n]).await?;
    progress.advance(n);
    queue.throttle.consume(n).await;
  };
  file.flush().await?;
  let finish = reader.finish().await;
  progress.finish();
  if matches!(outcome, Outcome::Done) {
    finish?;
    if let Some(mtime) = plan.source.mtime {
      let stamp = std::time::UNIX_EPOCH + Duration::from_secs(mtime.max(0) as u64);
      let _ = file.into_std().await.set_modified(stamp);
    }
  }
  Ok(outcome)
}

async fn upload(
  queue: &Arc<TransferQueue>,
  item: &TransferItem,
  client: &dyn RemoteClient,
  plan: &Plan,
  control: &Control,
) -> VResult<Outcome> {
  let mut file = tokio::fs::File::open(&plan.local_path).await?;
  if plan.offset > 0 {
    file.seek(std::io::SeekFrom::Start(plan.offset)).await?;
  }
  let mut writer = client.open_write(&plan.remote_path, plan.offset).await?;
  let mut progress = ProgressTracker::new(queue, &item.id, plan.source.size, plan.offset);
  let mut buffer = vec![0u8; BUFFER_SIZE];
  let outcome = loop {
    if let Some(outcome) = interrupted(control) {
      break outcome;
    }
    let n = file.read(&mut buffer).await?;
    if n == 0 {
      break Outcome::Done;
    }
    writer.write(&buffer[..n]).await?;
    progress.advance(n);
    queue.throttle.consume(n).await;
  };
  let finish = writer.finish().await;
  progress.finish();
  if matches!(outcome, Outcome::Done) {
    finish?;
  }
  Ok(outcome)
}

async fn verify(
  queue: &Arc<TransferQueue>,
  item: &TransferItem,
  client: &dyn RemoteClient,
  plan: &Plan,
) -> VResult<()> {
  let expected = plan.source.size;
  let actual = match item.direction {
    Direction::Download => std::fs::metadata(&plan.local_path)?.len(),
    Direction::Upload => client.stat(&plan.remote_path).await?.size,
  };
  if actual != expected {
    return Err(VError::Protocol(format!(
      "size mismatch after transfer: expected {expected} bytes, got {actual}"
    )));
  }
  if !queue.settings.get().verify_hash {
    return Ok(());
  }
  let algorithms = client.supported_hashes();
  let Some(algo) = ["sha256", "md5", "sha1"]
    .iter()
    .find(|a| algorithms.iter().any(|s| s == *a))
  else {
    return Ok(());
  };
  let Some(remote_digest) = client.hash(&plan.remote_path, algo).await? else {
    return Ok(());
  };
  let local_digest = local_digest(&plan.local_path, algo).await?;
  if !remote_digest.eq_ignore_ascii_case(&local_digest) {
    return Err(VError::Protocol(format!("{algo} mismatch after transfer")));
  }
  Ok(())
}

async fn local_digest(path: &Path, algo: &str) -> VResult<String> {
  let path = path.to_path_buf();
  let algo = algo.to_string();
  tokio::task::spawn_blocking(move || -> VResult<String> {
    let mut file = std::fs::File::open(&path)?;
    Ok(match algo.as_str() {
      "sha256" => hash_stream::<sha2::Sha256>(&mut file)?,
      "sha1" => hash_stream::<sha1::Sha1>(&mut file)?,
      _ => hash_stream::<md5::Md5>(&mut file)?,
    })
  })
  .await
  .map_err(|e| VError::Other(e.to_string()))?
}

fn hash_stream<D: Digest + Default>(reader: &mut impl std::io::Read) -> VResult<String> {
  let mut hasher = D::default();
  let mut buffer = vec![0u8; BUFFER_SIZE];
  loop {
    let n = reader.read(&mut buffer)?;
    if n == 0 {
      break;
    }
    hasher.update(&buffer[..n]);
  }
  Ok(hex::encode(hasher.finalize()))
}
