use crate::askpass::PromptBroker;
use crate::edit::EditManager;
use crate::error::{VError, VResult};
use crate::icons::IconCache;
use crate::local_fs::{self, LocalEntry};
use crate::protocol::ftp::tls::TrustStore;
use crate::protocol::sftp;
use crate::protocol::{self, EntryKind, RemoteEntry};
use crate::session::{ConnectRequest, SessionInfo, SessionManager};
use crate::settings::{AppSettings, SettingsStore};
use crate::sites::{RecentConnection, SiteConfig, SiteStore, SiteTree};
use crate::sync::{self, DiffEntry, SyncRequest};
use crate::transfer::{ConflictAnswer, QueueRequest, QueueSnapshot, QueueStats, TransferQueue};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;

pub struct AppState {
  pub sites: Arc<SiteStore>,
  pub settings: Arc<SettingsStore>,
  pub sessions: Arc<SessionManager>,
  pub queue: Arc<TransferQueue>,
  pub broker: Arc<PromptBroker>,
  pub trust: Arc<TrustStore>,
  pub editor: Arc<EditManager>,
  pub icons: Arc<IconCache>,
}

type App<'a> = State<'a, AppState>;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemInfo {
  pub version: String,
  pub openssh: Option<String>,
  pub home_dir: String,
  pub data_dir: String,
  pub log_dir: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Listing {
  pub path: String,
  pub entries: Vec<RemoteEntry>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalListing {
  pub path: String,
  pub parent: Option<String>,
  pub entries: Vec<LocalEntry>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathTarget {
  pub path: String,
  pub is_dir: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkTarget {
  pub target: String,
  pub is_dir: bool,
}

#[tauri::command]
pub async fn system_info(app: tauri::AppHandle) -> SystemInfo {
  SystemInfo {
    version: app.package_info().version.to_string(),
    openssh: sftp::ssh_version().await,
    home_dir: local_fs::home_dir(),
    data_dir: crate::paths::data_dir().to_string_lossy().into_owned(),
    log_dir: crate::logging::log_dir().to_string_lossy().into_owned(),
  }
}

#[tauri::command]
pub fn settings_get(state: App<'_>) -> AppSettings {
  state.settings.get()
}

#[tauri::command]
pub fn settings_save(state: App<'_>, settings: AppSettings) -> VResult<()> {
  state
    .queue
    .set_bandwidth_limit_kbps(settings.bandwidth_limit_kbps);
  state.settings.save(settings)
}

#[tauri::command]
pub fn sites_get(state: App<'_>) -> SiteTree {
  state.sites.tree()
}

#[tauri::command]
pub fn sites_save(state: App<'_>, tree: SiteTree) -> VResult<()> {
  state.sites.save_tree(tree)
}

#[tauri::command]
pub fn site_password_get(state: App<'_>, site_id: String) -> VResult<Option<String>> {
  state.sites.password(&site_id)
}

#[tauri::command]
pub fn site_password_set(
  state: App<'_>,
  site_id: String,
  user: String,
  password: String,
) -> VResult<()> {
  state.sites.set_password(&site_id, &user, &password)
}

#[tauri::command]
pub fn sites_import_filezilla(state: App<'_>, path: String) -> VResult<SiteTree> {
  let imported = crate::sites::import::import_filezilla(std::path::Path::new(&path))?;
  merge_import(&state, imported)
}

#[tauri::command]
pub fn sites_import_winscp(state: App<'_>) -> VResult<SiteTree> {
  let imported = crate::sites::import::import_winscp()?;
  merge_import(&state, imported)
}

fn merge_import(
  state: &AppState,
  imported: crate::sites::import::ImportResult,
) -> VResult<SiteTree> {
  let mut tree = state.sites.tree();
  tree.root.extend(imported.nodes);
  for (site_id, user, password) in imported.passwords {
    let _ = state.sites.set_password(&site_id, &user, &password);
  }
  state.sites.save_tree(tree.clone())?;
  Ok(tree)
}

#[tauri::command]
pub fn recent_get(state: App<'_>) -> Vec<RecentConnection> {
  state.sites.recent()
}

#[tauri::command]
pub fn recent_clear(state: App<'_>) -> VResult<()> {
  state.sites.clear_recent()
}

#[tauri::command]
pub async fn session_connect(
  state: App<'_>,
  site: SiteConfig,
  password: Option<String>,
  accept_new_hostkey: Option<bool>,
  remember: Option<bool>,
) -> VResult<SessionInfo> {
  let password = match password {
    Some(p) => Some(p),
    None => state.sites.password(&site.id)?,
  };
  let info = state
    .sessions
    .connect(ConnectRequest {
      site: site.clone(),
      password,
      accept_new_hostkey: accept_new_hostkey.unwrap_or(false),
    })
    .await?;
  if remember.unwrap_or(true) {
    let _ = state.sites.remember_recent(&site);
  }
  state.queue.bind_session(&site.id, &info.id);
  state.editor.rebind_session(&site.id, &info.id);
  Ok(info)
}

#[tauri::command]
pub async fn session_disconnect(state: App<'_>, session_id: String) -> VResult<()> {
  state.queue.unbind_session(&session_id);
  state.sessions.disconnect(&session_id).await;
  Ok(())
}

#[tauri::command]
pub fn prompt_answer(state: App<'_>, prompt_id: String, answer: Option<String>) -> bool {
  state.broker.answer(&prompt_id, answer)
}

#[tauri::command]
pub async fn hostkey_forget(host: String, port: u16) -> VResult<()> {
  sftp::forget_host_key(&host, port).await
}

#[tauri::command]
pub fn certificate_trust(state: App<'_>, fingerprint: String, remember: bool) {
  state.trust.trust(&fingerprint, remember);
}

#[tauri::command]
pub async fn remote_list(
  state: App<'_>,
  session_id: String,
  path: String,
  force: Option<bool>,
) -> VResult<Listing> {
  let session = state.sessions.get(&session_id)?;
  let client = session.client();
  let resolved = if path.trim().is_empty() {
    client.initial_dir().await?
  } else {
    client.realpath(&path).await?
  };
  let entries = session.list(&resolved, force.unwrap_or(false)).await?;
  Ok(Listing {
    path: resolved,
    entries,
  })
}

#[tauri::command]
pub async fn remote_realpath(state: App<'_>, session_id: String, path: String) -> VResult<String> {
  state
    .sessions
    .get(&session_id)?
    .client()
    .realpath(&path)
    .await
}

#[tauri::command]
pub async fn remote_stat(state: App<'_>, session_id: String, path: String) -> VResult<RemoteEntry> {
  state.sessions.get(&session_id)?.client().stat(&path).await
}

#[tauri::command]
pub async fn remote_mkdir(state: App<'_>, session_id: String, path: String) -> VResult<()> {
  let session = state.sessions.get(&session_id)?;
  session.client().mkdir(&path).await?;
  session.invalidate(&path);
  Ok(())
}

#[tauri::command]
pub async fn remote_touch(state: App<'_>, session_id: String, path: String) -> VResult<()> {
  let session = state.sessions.get(&session_id)?;
  session.client().touch(&path).await?;
  session.invalidate(&path);
  Ok(())
}

#[tauri::command]
pub async fn remote_rename(
  state: App<'_>,
  session_id: String,
  from: String,
  to: String,
) -> VResult<()> {
  let session = state.sessions.get(&session_id)?;
  session.client().rename(&from, &to).await?;
  session.invalidate(&from);
  session.invalidate(&to);
  Ok(())
}

#[tauri::command]
pub async fn remote_chmod(
  state: App<'_>,
  session_id: String,
  paths: Vec<String>,
  mode: u32,
) -> VResult<()> {
  let session = state.sessions.get(&session_id)?;
  let client = session.client();
  for path in paths {
    client.chmod(&path, mode).await?;
    session.invalidate(&path);
  }
  Ok(())
}

#[tauri::command]
pub async fn remote_delete(
  state: App<'_>,
  session_id: String,
  targets: Vec<PathTarget>,
) -> VResult<()> {
  let session = state.sessions.get(&session_id)?;
  for target in targets {
    if target.is_dir {
      delete_remote_tree(&session, &target.path).await?;
    } else {
      session.client().remove(&target.path).await?;
    }
    session.invalidate(&target.path);
  }
  Ok(())
}

async fn delete_remote_tree(session: &Arc<crate::session::Session>, path: &str) -> VResult<()> {
  let client = session.client();
  let mut stack = vec![path.to_string()];
  let mut dirs_to_remove = Vec::new();
  while let Some(dir) = stack.pop() {
    for entry in client.list(&dir).await? {
      if entry.kind == EntryKind::Dir {
        stack.push(entry.path);
      } else {
        client.remove(&entry.path).await?;
      }
    }
    dirs_to_remove.push(dir);
  }
  for dir in dirs_to_remove.into_iter().rev() {
    client.rmdir(&dir).await?;
    session.invalidate(&dir);
  }
  Ok(())
}

#[tauri::command]
pub async fn remote_resolve_link(
  state: App<'_>,
  session_id: String,
  path: String,
) -> VResult<LinkTarget> {
  let session = state.sessions.get(&session_id)?;
  let client = session.client();
  let raw = client.readlink(&path).await?;
  let resolved = protocol::resolve_link_target(&path, &raw);
  let target = client.realpath(&resolved).await.unwrap_or(resolved);
  let is_dir = client
    .stat(&target)
    .await
    .map(|e| e.kind == EntryKind::Dir)
    .unwrap_or(false);
  Ok(LinkTarget { target, is_dir })
}

#[tauri::command]
pub async fn remote_edit_open(state: App<'_>, session_id: String, path: String) -> VResult<String> {
  let session = state.sessions.get(&session_id)?;
  state.editor.open(session, &path).await
}

#[tauri::command]
pub fn local_list(
  state: App<'_>,
  path: String,
  show_hidden: Option<bool>,
) -> VResult<LocalListing> {
  let show_hidden = show_hidden.unwrap_or_else(|| state.settings.get().show_hidden);
  if path.trim().is_empty() {
    return Ok(LocalListing {
      path: String::new(),
      parent: None,
      entries: local_fs::list_drives(),
    });
  }
  let canonical = std::fs::canonicalize(&path)
    .map(|p| local_fs::strip_verbatim(&p.to_string_lossy()))
    .unwrap_or_else(|_| path.clone());
  let entries = local_fs::list_dir(&canonical, show_hidden)?;
  Ok(LocalListing {
    parent: local_fs::parent_of(&canonical),
    path: canonical,
    entries,
  })
}

#[tauri::command]
pub fn local_home() -> String {
  local_fs::home_dir()
}

#[tauri::command]
pub fn local_mkdir(path: String) -> VResult<()> {
  local_fs::mkdir(&path)
}

#[tauri::command]
pub fn local_touch(path: String) -> VResult<()> {
  local_fs::create_file(&path)
}

#[tauri::command]
pub fn local_rename(from: String, to: String) -> VResult<()> {
  local_fs::rename(&from, &to)
}

#[tauri::command]
pub fn local_delete(paths: Vec<String>) -> VResult<()> {
  for path in paths {
    local_fs::delete(&path)?;
  }
  Ok(())
}

#[tauri::command]
pub fn local_resolve_link(path: String) -> VResult<LinkTarget> {
  let target = local_fs::resolve_link(&path)?;
  let is_dir = std::fs::metadata(&target)
    .map(|m| m.is_dir())
    .unwrap_or(false);
  Ok(LinkTarget { target, is_dir })
}

#[tauri::command]
pub fn queue_add(state: App<'_>, requests: Vec<QueueRequest>) -> VResult<Vec<String>> {
  state.queue.add(requests)
}

#[tauri::command]
pub fn queue_snapshot(state: App<'_>) -> QueueSnapshot {
  state.queue.snapshot()
}

#[tauri::command]
pub fn queue_stats(state: App<'_>) -> QueueStats {
  state.queue.stats()
}

#[tauri::command]
pub fn queue_pause(state: App<'_>, id: String) {
  state.queue.pause(&id);
}

#[tauri::command]
pub fn queue_resume(state: App<'_>, id: String) {
  state.queue.resume(&id);
}

#[tauri::command]
pub fn queue_remove(state: App<'_>, id: String) {
  state.queue.remove(&id);
}

#[tauri::command]
pub fn queue_move(state: App<'_>, id: String, index: usize) {
  state.queue.move_item(&id, index);
}

#[tauri::command]
pub fn queue_priority(state: App<'_>, id: String, priority: i32) {
  state.queue.set_priority(&id, priority);
}

#[tauri::command]
pub fn queue_pause_all(state: App<'_>) {
  state.queue.pause_all();
}

#[tauri::command]
pub fn queue_resume_all(state: App<'_>) {
  state.queue.resume_all();
}

#[tauri::command]
pub fn queue_retry_failed(state: App<'_>) {
  state.queue.retry_failed();
}

#[tauri::command]
pub fn queue_remove_failed(state: App<'_>) {
  state.queue.remove_failed();
}

#[tauri::command]
pub fn queue_clear(state: App<'_>) {
  state.queue.clear();
}

#[tauri::command]
pub fn queue_clear_history(state: App<'_>) {
  state.queue.clear_history();
}

#[tauri::command]
pub fn conflict_answer(state: App<'_>, item_id: String, answer: ConflictAnswer) -> bool {
  state.queue.answer_conflict(&item_id, answer)
}

#[tauri::command]
pub async fn sync_compare(state: App<'_>, request: SyncRequest) -> VResult<Vec<DiffEntry>> {
  let session = state.sessions.get(&request.session_id)?;
  sync::compare(session, request).await
}

#[tauri::command]
pub async fn file_icons(
  state: App<'_>,
  keys: Vec<String>,
) -> VResult<std::collections::HashMap<String, Option<String>>> {
  let cache = state.icons.clone();
  tokio::task::spawn_blocking(move || cache.resolve(&keys))
    .await
    .map_err(|e| VError::Other(e.to_string()))
}

#[tauri::command]
pub fn open_path(path: String) -> VResult<()> {
  tauri_plugin_opener::open_path(path, None::<&str>).map_err(|e| VError::Other(e.to_string()))
}
