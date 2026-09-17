use super::conflict::{ConflictAnswer, ConflictPolicy, ConflictPrompt};
use super::throttle::Throttle;
use super::{worker, Direction, TransferItem, TransferStatus};
use crate::error::{VError, VResult};
use crate::session::{Session, SessionManager};
use crate::settings::SettingsStore;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::sync::{oneshot, Notify};
use tokio_util::sync::CancellationToken;

const TICK: Duration = Duration::from_millis(400);
const CONFLICT_TIMEOUT: Duration = Duration::from_secs(3600);

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueRequest {
  pub session_id: String,
  pub direction: Direction,
  pub local_path: String,
  pub remote_path: String,
  pub is_dir: bool,
  pub size: Option<u64>,
  pub mtime: Option<i64>,
  #[serde(default)]
  pub priority: i32,
  #[serde(default = "default_true")]
  pub follow_symlink: bool,
  #[serde(default)]
  pub conflict_policy: Option<ConflictPolicy>,
  #[serde(default)]
  pub start_paused: bool,
  #[serde(default)]
  pub link_target: Option<String>,
  #[serde(default)]
  pub batch: Option<String>,
  #[serde(default)]
  pub target_session_id: Option<String>,
  #[serde(default)]
  pub target_path: Option<String>,
}

fn default_true() -> bool {
  true
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct QueueSnapshot {
  pub items: Vec<TransferItem>,
  pub history: Vec<TransferItem>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BatchStatus {
  pub pending: usize,
  pub failed: usize,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct QueueStats {
  pub active: usize,
  pub queued: usize,
  pub failed: usize,
  pub paused: usize,
  pub speed_bps: u64,
  pub remaining_bytes: u64,
}

#[derive(Clone)]
pub(super) struct Control {
  pub token: CancellationToken,
  pub pausing: Arc<AtomicBool>,
}

pub struct TransferQueue {
  app: AppHandle,
  pub(super) sessions: Arc<SessionManager>,
  pub(super) settings: Arc<SettingsStore>,
  items: parking_lot::Mutex<Vec<TransferItem>>,
  history: parking_lot::Mutex<Vec<TransferItem>>,
  controls: parking_lot::Mutex<HashMap<String, Control>>,
  conflicts: parking_lot::Mutex<HashMap<String, oneshot::Sender<ConflictAnswer>>>,
  apply_all: parking_lot::Mutex<HashMap<String, ConflictPolicy>>,
  pub(super) throttle: Throttle,
  notify: Notify,
  dirty: AtomicBool,
  snapshot_dirty: AtomicBool,
}

impl TransferQueue {
  pub fn new(
    app: AppHandle,
    sessions: Arc<SessionManager>,
    settings: Arc<SettingsStore>,
  ) -> Arc<Self> {
    let stored: QueueSnapshot = std::fs::read_to_string(crate::paths::queue_file())
      .ok()
      .and_then(|text| serde_json::from_str(&text).ok())
      .unwrap_or_default();
    let items = stored
      .items
      .into_iter()
      .map(|mut item| {
        item.session_id = None;
        item.speed_bps = 0;
        if item.status == TransferStatus::Active {
          item.status = TransferStatus::Queued;
        }
        item
      })
      .collect();
    let limit = settings.get().bandwidth_limit_kbps;
    let queue = Arc::new(Self {
      app,
      sessions,
      settings,
      items: parking_lot::Mutex::new(items),
      history: parking_lot::Mutex::new(stored.history),
      controls: parking_lot::Mutex::new(HashMap::new()),
      conflicts: parking_lot::Mutex::new(HashMap::new()),
      apply_all: parking_lot::Mutex::new(HashMap::new()),
      throttle: Throttle::new(limit),
      notify: Notify::new(),
      dirty: AtomicBool::new(false),
      snapshot_dirty: AtomicBool::new(true),
    });
    tauri::async_runtime::spawn(queue.clone().scheduler_loop());
    queue
  }

  pub fn set_bandwidth_limit_kbps(&self, limit: u64) {
    self.throttle.set_limit_kbps(limit);
  }

  pub fn snapshot(&self) -> QueueSnapshot {
    QueueSnapshot {
      items: self.items.lock().clone(),
      history: self.history.lock().clone(),
    }
  }

  pub fn stats(&self) -> QueueStats {
    let items = self.items.lock();
    let mut stats = QueueStats::default();
    for item in items.iter() {
      match item.status {
        TransferStatus::Active => {
          stats.active += 1;
          stats.speed_bps += item.speed_bps;
        }
        TransferStatus::Queued => stats.queued += 1,
        TransferStatus::Failed => stats.failed += 1,
        TransferStatus::Paused => stats.paused += 1,
        _ => {}
      }
      if item.is_pending() && !item.is_dir {
        stats.remaining_bytes += item.size.unwrap_or(0).saturating_sub(item.transferred);
      }
    }
    stats
  }

  pub fn add(&self, requests: Vec<QueueRequest>) -> VResult<Vec<String>> {
    let mut ids = Vec::with_capacity(requests.len());
    let mut new_items = Vec::with_capacity(requests.len());
    for request in requests {
      let session = self.sessions.get(&request.session_id)?;
      if let Some(target) = request.target_session_id.as_deref() {
        self.sessions.get(target)?;
      }
      let item = self.build_item(&session, request);
      ids.push(item.id.clone());
      new_items.push(item);
    }
    self.items.lock().extend(new_items);
    self.changed();
    Ok(ids)
  }

  fn build_item(&self, session: &Session, request: QueueRequest) -> TransferItem {
    TransferItem {
      id: uuid::Uuid::new_v4().to_string(),
      session_id: Some(session.id.clone()),
      site_id: session.site.id.clone(),
      site_name: session.site.display_name(),
      direction: request.direction,
      local_path: request.local_path,
      remote_path: request.remote_path,
      is_dir: request.is_dir,
      size: request.size,
      source_mtime: request.mtime,
      transferred: 0,
      status: if request.start_paused {
        TransferStatus::Paused
      } else {
        TransferStatus::Queued
      },
      priority: request.priority,
      error: None,
      attempts: 0,
      created_at: chrono::Utc::now().timestamp_millis(),
      finished_at: None,
      speed_bps: 0,
      follow_symlink: request.follow_symlink,
      conflict_policy: request.conflict_policy,
      link_target: request.link_target,
      batch: request.batch,
      target_session_id: request.target_session_id,
      target_path: request.target_path,
      retry_after: None,
    }
  }

  pub fn batch_status(&self, batch: &str) -> BatchStatus {
    let items = self.items.lock();
    let mut status = BatchStatus::default();
    for item in items.iter().filter(|i| i.batch.as_deref() == Some(batch)) {
      if item.status == TransferStatus::Failed {
        status.failed += 1;
      } else if item.is_pending() {
        status.pending += 1;
      }
    }
    status
  }

  pub fn remove_batch(&self, batch: &str) {
    let ids: Vec<String> = self
      .items
      .lock()
      .iter()
      .filter(|i| i.batch.as_deref() == Some(batch))
      .map(|i| i.id.clone())
      .collect();
    for id in ids {
      self.remove(&id);
    }
  }

  pub(super) fn insert_children(&self, parent_id: &str, children: Vec<TransferItem>) {
    let mut items = self.items.lock();
    let position = items
      .iter()
      .position(|i| i.id == parent_id)
      .map(|p| p + 1)
      .unwrap_or(items.len());
    for (offset, child) in children.into_iter().enumerate() {
      items.insert(position + offset, child);
    }
    drop(items);
    self.changed();
  }

  pub(super) fn get(&self, id: &str) -> Option<TransferItem> {
    self.items.lock().iter().find(|i| i.id == id).cloned()
  }

  pub(super) fn update(&self, id: &str, apply: impl FnOnce(&mut TransferItem)) -> bool {
    let mut items = self.items.lock();
    match items.iter_mut().find(|i| i.id == id) {
      Some(item) => {
        apply(item);
        true
      }
      None => false,
    }
  }

  pub(super) fn record_progress(
    &self,
    id: &str,
    transferred: u64,
    speed_bps: u64,
    size: Option<u64>,
  ) {
    let updated = self.update(id, |item| {
      item.transferred = transferred;
      item.speed_bps = speed_bps;
      if size.is_some() {
        item.size = size;
      }
    });
    if updated {
      let _ = self.app.emit(
        "transfer-progress",
        ProgressEvent {
          id: id.to_string(),
          transferred,
          speed_bps,
          size,
        },
      );
    }
  }

  pub(super) fn complete(&self, id: &str, status: TransferStatus, error: Option<String>) {
    let finished = {
      let mut items = self.items.lock();
      let Some(index) = items.iter().position(|i| i.id == id) else {
        return;
      };
      let item = &mut items[index];
      item.status = status;
      item.error = error;
      item.speed_bps = 0;
      item.finished_at = Some(chrono::Utc::now().timestamp_millis());
      if matches!(status, TransferStatus::Done | TransferStatus::Skipped) {
        Some(items.remove(index))
      } else {
        None
      }
    };
    if let Some(item) = finished {
      let mut history = self.history.lock();
      history.insert(0, item);
      let cap = self.settings.get().max_successful_history.max(10);
      history.truncate(cap);
    }
    self.controls.lock().remove(id);
    self.changed();
  }

  pub(super) fn schedule_retry(&self, id: &str, error: String, delay: Duration) {
    self.update(id, |item| {
      item.status = TransferStatus::Queued;
      item.error = Some(error);
      item.speed_bps = 0;
      item.retry_after = Some(chrono::Utc::now().timestamp_millis() + delay.as_millis() as i64);
    });
    self.controls.lock().remove(id);
    self.changed();
  }

  pub(super) fn set_paused(&self, id: &str) {
    self.update(id, |item| {
      item.status = TransferStatus::Paused;
      item.speed_bps = 0;
    });
    self.controls.lock().remove(id);
    self.changed();
  }

  pub fn pause(&self, id: &str) {
    let control = self.controls.lock().get(id).cloned();
    match control {
      Some(control) => {
        control.pausing.store(true, Ordering::Relaxed);
        control.token.cancel();
      }
      None => {
        self.update(id, |item| {
          if item.status == TransferStatus::Queued {
            item.status = TransferStatus::Paused;
          }
        });
        self.changed();
      }
    }
  }

  pub fn resume(&self, id: &str) {
    self.update(id, |item| {
      if matches!(item.status, TransferStatus::Paused | TransferStatus::Failed) {
        item.status = TransferStatus::Queued;
        item.error = None;
        item.attempts = 0;
        item.retry_after = None;
      }
    });
    self.changed();
  }

  pub fn remove(&self, id: &str) {
    let control = self.controls.lock().get(id).cloned();
    if let Some(control) = control {
      control.pausing.store(false, Ordering::Relaxed);
      control.token.cancel();
    }
    self.items.lock().retain(|i| i.id != id);
    self.conflicts.lock().remove(id);
    self.changed();
  }

  pub fn move_item(&self, id: &str, new_index: usize) {
    let mut items = self.items.lock();
    let Some(current) = items.iter().position(|i| i.id == id) else {
      return;
    };
    let item = items.remove(current);
    let target = new_index.min(items.len());
    items.insert(target, item);
    drop(items);
    self.changed();
  }

  pub fn set_priority(&self, id: &str, priority: i32) {
    self.update(id, |item| item.priority = priority);
    self.changed();
  }

  pub fn pause_all(&self) {
    let ids: Vec<String> = self
      .items
      .lock()
      .iter()
      .filter(|i| matches!(i.status, TransferStatus::Queued | TransferStatus::Active))
      .map(|i| i.id.clone())
      .collect();
    for id in ids {
      self.pause(&id);
    }
  }

  pub fn resume_all(&self) {
    let ids: Vec<String> = self
      .items
      .lock()
      .iter()
      .filter(|i| i.status == TransferStatus::Paused)
      .map(|i| i.id.clone())
      .collect();
    for id in ids {
      self.resume(&id);
    }
  }

  pub fn retry_failed(&self) {
    for item in self
      .items
      .lock()
      .iter_mut()
      .filter(|i| i.status == TransferStatus::Failed)
    {
      item.status = TransferStatus::Queued;
      item.error = None;
      item.attempts = 0;
      item.retry_after = None;
    }
    self.changed();
  }

  pub fn remove_failed(&self) {
    self
      .items
      .lock()
      .retain(|i| i.status != TransferStatus::Failed);
    self.changed();
  }

  pub fn clear(&self) {
    let controls: Vec<_> = self.controls.lock().values().cloned().collect();
    for control in controls {
      control.pausing.store(false, Ordering::Relaxed);
      control.token.cancel();
    }
    self.items.lock().clear();
    self.conflicts.lock().clear();
    self.changed();
  }

  pub fn clear_history(&self) {
    self.history.lock().clear();
    self.changed();
  }

  pub fn bind_session(&self, site_id: &str, session_id: &str) {
    let mut items = self.items.lock();
    for item in items
      .iter_mut()
      .filter(|i| i.site_id == site_id && i.session_id.is_none())
    {
      item.session_id = Some(session_id.to_string());
    }
    drop(items);
    self.changed();
  }

  pub fn unbind_session(&self, session_id: &str) {
    let ids: Vec<String> = self
      .items
      .lock()
      .iter()
      .filter(|i| i.session_id.as_deref() == Some(session_id))
      .map(|i| i.id.clone())
      .collect();
    for id in &ids {
      self.pause(id);
    }
    let mut items = self.items.lock();
    for item in items
      .iter_mut()
      .filter(|i| i.session_id.as_deref() == Some(session_id))
    {
      item.session_id = None;
    }
    drop(items);
    self.apply_all.lock().remove(session_id);
    self.changed();
  }

  pub fn answer_conflict(&self, item_id: &str, answer: ConflictAnswer) -> bool {
    match self.conflicts.lock().remove(item_id) {
      Some(tx) => tx.send(answer).is_ok(),
      None => false,
    }
  }

  pub(super) fn effective_policy(&self, item: &TransferItem, session: &Session) -> ConflictPolicy {
    if let Some(policy) = item.conflict_policy {
      return policy;
    }
    if let Some(policy) = self.apply_all.lock().get(&session.id) {
      return *policy;
    }
    session
      .site
      .conflict_policy
      .unwrap_or_else(|| self.settings.get().default_conflict_policy)
  }

  pub(super) async fn ask_conflict(&self, prompt: ConflictPrompt) -> VResult<ConflictAnswer> {
    let (tx, rx) = oneshot::channel();
    let item_id = prompt.item_id.clone();
    let session_id = prompt.session_id.clone();
    self.conflicts.lock().insert(item_id.clone(), tx);
    let _ = self.app.emit("conflict-prompt", prompt);
    let answer = tokio::time::timeout(CONFLICT_TIMEOUT, rx)
      .await
      .map_err(|_| VError::Timeout("conflict prompt unanswered".into()))?
      .map_err(|_| VError::Cancelled)?;
    if answer.apply_to_all {
      self
        .apply_all
        .lock()
        .insert(session_id, answer.action.as_policy());
    }
    Ok(answer)
  }

  pub(super) fn changed(&self) {
    self.dirty.store(true, Ordering::Relaxed);
    self.snapshot_dirty.store(true, Ordering::Relaxed);
    self.notify.notify_one();
  }

  fn persist(&self) {
    let snapshot = self.snapshot();
    let stored = QueueSnapshot {
      items: snapshot
        .items
        .into_iter()
        .filter(|i| i.is_pending() || i.status == TransferStatus::Failed)
        .collect(),
      history: snapshot.history,
    };
    if let Ok(text) = serde_json::to_string(&stored) {
      let _ = std::fs::write(crate::paths::queue_file(), text);
    }
  }

  fn emit_snapshot(&self) {
    let _ = self.app.emit("queue-changed", self.snapshot());
  }

  fn emit_stats(&self) {
    let _ = self.app.emit("queue-stats", self.stats());
  }

  async fn scheduler_loop(self: Arc<Self>) {
    loop {
      tokio::select! {
          _ = self.notify.notified() => {}
          _ = tokio::time::sleep(TICK) => {}
      }
      self.dispatch();
      if self.snapshot_dirty.swap(false, Ordering::Relaxed) {
        self.emit_snapshot();
      }
      if self.dirty.swap(false, Ordering::Relaxed) {
        self.persist();
      }
      self.emit_stats();
      self.clear_apply_all_when_idle();
    }
  }

  fn clear_apply_all_when_idle(&self) {
    let idle = !self.items.lock().iter().any(|i| i.is_pending());
    if idle {
      self.apply_all.lock().clear();
    }
  }

  fn dispatch(self: &Arc<Self>) {
    let now = chrono::Utc::now().timestamp_millis();
    let sessions = self.sessions.all();
    let mut to_start: Vec<String> = Vec::new();
    {
      let mut items = self.items.lock();
      for session in &sessions {
        let active = items
          .iter()
          .filter(|i| {
            i.session_id.as_deref() == Some(&session.id) && i.status == TransferStatus::Active
          })
          .count();
        let free = session.max_transfers().saturating_sub(active);
        if free == 0 {
          continue;
        }
        let mut candidates: Vec<usize> = items
          .iter()
          .enumerate()
          .filter(|(_, i)| {
            i.session_id.as_deref() == Some(&session.id)
              && i.status == TransferStatus::Queued
              && i.retry_after.map(|t| t <= now).unwrap_or(true)
          })
          .map(|(index, _)| index)
          .collect();
        candidates.sort_by_key(|&index| std::cmp::Reverse(items[index].priority));
        for index in candidates.into_iter().take(free) {
          items[index].status = TransferStatus::Active;
          items[index].retry_after = None;
          to_start.push(items[index].id.clone());
        }
      }
    }
    for id in to_start {
      let control = Control {
        token: CancellationToken::new(),
        pausing: Arc::new(AtomicBool::new(false)),
      };
      self.controls.lock().insert(id.clone(), control.clone());
      tokio::spawn(worker::run(self.clone(), id, control));
      self.snapshot_dirty.store(true, Ordering::Relaxed);
    }
  }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProgressEvent {
  id: String,
  transferred: u64,
  speed_bps: u64,
  size: Option<u64>,
}
