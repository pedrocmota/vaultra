use crate::error::{VError, VResult};
use crate::protocol::basename;
use crate::session::Session;
use notify::{EventKind, RecursiveMode, Watcher};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;

const DEBOUNCE: Duration = Duration::from_millis(1500);
const BUFFER_SIZE: usize = 128 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditedFileChanged {
  pub session_id: String,
  pub site_id: String,
  pub remote_path: String,
  pub local_path: String,
}

#[derive(Clone)]
struct WatchedFile {
  session_id: String,
  site_id: String,
  remote_path: String,
  last_notified: Instant,
}

pub struct EditManager {
  app: AppHandle,
  watched: Arc<parking_lot::Mutex<HashMap<PathBuf, WatchedFile>>>,
  watcher: parking_lot::Mutex<Option<notify::RecommendedWatcher>>,
}

impl EditManager {
  pub fn new(app: AppHandle) -> Self {
    Self {
      app,
      watched: Arc::new(parking_lot::Mutex::new(HashMap::new())),
      watcher: parking_lot::Mutex::new(None),
    }
  }

  pub async fn open(&self, session: Arc<Session>, remote_path: &str) -> VResult<String> {
    let local_path = self.download(&session, remote_path).await?;
    tauri_plugin_opener::open_path(&local_path, None::<&str>)
      .map_err(|e| VError::Other(format!("failed to open file: {e}")))?;
    self.watch(&local_path, &session, remote_path)?;
    Ok(local_path.to_string_lossy().into_owned())
  }

  async fn download(&self, session: &Arc<Session>, remote_path: &str) -> VResult<PathBuf> {
    let digest = hex::encode(Sha256::digest(
      format!("{}{remote_path}", session.site.id).as_bytes(),
    ));
    let dir = crate::paths::edit_temp_dir().join(&digest[..16]);
    std::fs::create_dir_all(&dir)?;
    let local_path = dir.join(basename(remote_path));
    let lease = session.transfer_client().await?;
    let mut reader = lease.client.open_read(remote_path, 0).await?;
    let mut file = tokio::fs::File::create(&local_path).await?;
    let mut buffer = vec![0u8; BUFFER_SIZE];
    loop {
      let n = reader.read(&mut buffer).await?;
      if n == 0 {
        break;
      }
      file.write_all(&buffer[..n]).await?;
    }
    file.flush().await?;
    reader.finish().await?;
    session
      .log
      .status(format!("Opened {remote_path} for editing"));
    Ok(local_path)
  }

  fn watch(&self, local_path: &Path, session: &Arc<Session>, remote_path: &str) -> VResult<()> {
    self.watched.lock().insert(
      local_path.to_path_buf(),
      WatchedFile {
        session_id: session.id.clone(),
        site_id: session.site.id.clone(),
        remote_path: remote_path.to_string(),
        last_notified: Instant::now(),
      },
    );
    let mut watcher = self.watcher.lock();
    if watcher.is_none() {
      *watcher = Some(self.build_watcher()?);
    }
    if let Some(w) = watcher.as_mut() {
      w.watch(local_path, RecursiveMode::NonRecursive)
        .map_err(|e| VError::Other(format!("failed to watch file: {e}")))?;
    }
    Ok(())
  }

  fn build_watcher(&self) -> VResult<notify::RecommendedWatcher> {
    let app = self.app.clone();
    let watched = self.watched.clone();
    notify::recommended_watcher(move |event: Result<notify::Event, notify::Error>| {
      let Ok(event) = event else { return };
      if !matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_)) {
        return;
      }
      for path in event.paths {
        let mut map = watched.lock();
        let Some(entry) = map.get_mut(&path) else {
          continue;
        };
        if entry.last_notified.elapsed() < DEBOUNCE {
          continue;
        }
        entry.last_notified = Instant::now();
        let _ = app.emit(
          "edited-file-changed",
          EditedFileChanged {
            session_id: entry.session_id.clone(),
            site_id: entry.site_id.clone(),
            remote_path: entry.remote_path.clone(),
            local_path: path.to_string_lossy().into_owned(),
          },
        );
      }
    })
    .map_err(|e| VError::Other(format!("failed to create file watcher: {e}")))
  }

  pub fn rebind_session(&self, site_id: &str, session_id: &str) {
    for entry in self
      .watched
      .lock()
      .values_mut()
      .filter(|w| w.site_id == site_id)
    {
      entry.session_id = session_id.to_string();
    }
  }
}
