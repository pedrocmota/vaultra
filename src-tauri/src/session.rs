use crate::askpass::{PromptBroker, PromptHandler, PromptKind};
use crate::error::{VError, VResult};
use crate::logging::Logger;
use crate::protocol::ftp::client::{FtpOptions, TransferMode};
use crate::protocol::ftp::tls::TrustStore;
use crate::protocol::ftp::FtpClient;
use crate::protocol::sftp::{self, SshOptions};
use crate::protocol::{parent, Protocol, RemoteClient, RemoteEntry};
use crate::settings::SettingsStore;
use crate::sites::{LogonType, SiteConfig};
use async_trait::async_trait;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::AppHandle;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
  pub id: String,
  pub site: SiteConfig,
  pub initial_dir: String,
  pub supported_hashes: Vec<String>,
}

pub struct ConnectRequest {
  pub site: SiteConfig,
  pub password: Option<String>,
  pub accept_new_hostkey: bool,
}

struct SessionPromptHandler {
  app: AppHandle,
  broker: Arc<PromptBroker>,
  session_id: String,
  password: parking_lot::Mutex<Option<String>>,
  password_uses: AtomicU32,
  log: Arc<Logger>,
}

#[async_trait]
impl PromptHandler for SessionPromptHandler {
  async fn on_prompt(&self, kind: PromptKind, text: String) -> Option<String> {
    match kind {
      PromptKind::Info => {
        self.log.status(text);
        Some(String::new())
      }
      PromptKind::Secret => {
        let looks_like_password = text.to_ascii_lowercase().contains("password");
        let stored = self.password.lock().clone();
        if looks_like_password
          && stored.is_some()
          && self.password_uses.fetch_add(1, Ordering::Relaxed) == 0
        {
          return stored;
        }
        let answer = self
          .broker
          .ask(&self.app, &self.session_id, kind, text)
          .await?;
        if looks_like_password {
          *self.password.lock() = Some(answer.clone());
        }
        Some(answer)
      }
      PromptKind::Confirm | PromptKind::HostKey => {
        let answer = self
          .broker
          .ask(&self.app, &self.session_id, kind, text)
          .await;
        Some(if answer.as_deref() == Some("yes") {
          "yes".into()
        } else {
          "no".into()
        })
      }
    }
  }
}

struct ListingCache {
  ttl: Duration,
  entries: parking_lot::Mutex<HashMap<String, (Instant, Vec<RemoteEntry>)>>,
}

impl ListingCache {
  fn new(ttl: Duration) -> Self {
    Self {
      ttl,
      entries: parking_lot::Mutex::new(HashMap::new()),
    }
  }

  fn get(&self, path: &str) -> Option<Vec<RemoteEntry>> {
    let entries = self.entries.lock();
    let (stamp, listing) = entries.get(path)?;
    (stamp.elapsed() < self.ttl).then(|| listing.clone())
  }

  fn put(&self, path: &str, listing: Vec<RemoteEntry>) {
    self
      .entries
      .lock()
      .insert(path.to_string(), (Instant::now(), listing));
  }

  fn invalidate(&self, path: &str) {
    self.entries.lock().remove(path);
  }
}

pub struct Session {
  pub id: String,
  pub site: SiteConfig,
  pub log: Arc<Logger>,
  primary: Arc<dyn RemoteClient>,
  password: parking_lot::Mutex<Option<String>>,
  idle_pool: Mutex<Vec<Arc<dyn RemoteClient>>>,
  pooled_count: AtomicUsize,
  trust: Arc<TrustStore>,
  cache: ListingCache,
  keepalive_task: parking_lot::Mutex<Option<tokio::task::JoinHandle<()>>>,
}

pub struct TransferLease {
  pub client: Arc<dyn RemoteClient>,
  session: Arc<Session>,
  pooled: bool,
}

impl Drop for TransferLease {
  fn drop(&mut self) {
    if !self.pooled {
      return;
    }
    let session = self.session.clone();
    let client = self.client.clone();
    tokio::spawn(async move {
      session.idle_pool.lock().await.push(client);
    });
  }
}

impl Session {
  pub fn max_transfers(&self) -> usize {
    self.site.max_connections.max(1) as usize
  }

  pub fn client(&self) -> Arc<dyn RemoteClient> {
    self.primary.clone()
  }

  pub async fn transfer_client(self: &Arc<Self>) -> VResult<TransferLease> {
    if self.primary.supports_multiplexing() {
      return Ok(TransferLease {
        client: self.primary.clone(),
        session: self.clone(),
        pooled: false,
      });
    }
    if let Some(client) = self.idle_pool.lock().await.pop() {
      return Ok(TransferLease {
        client,
        session: self.clone(),
        pooled: true,
      });
    }
    let password = self.password.lock().clone().unwrap_or_default();
    let log = self.log.clone();
    log.status("Opening additional connection for transfer");
    let client = connect_ftp(&self.site, password, self.trust.clone(), log).await?;
    self.pooled_count.fetch_add(1, Ordering::Relaxed);
    Ok(TransferLease {
      client,
      session: self.clone(),
      pooled: true,
    })
  }

  pub async fn list(&self, path: &str, force: bool) -> VResult<Vec<RemoteEntry>> {
    if !force {
      if let Some(cached) = self.cache.get(path) {
        self.log.status(format!(
          "Directory listing of \"{path}\" served from cache ({} entries)",
          cached.len()
        ));
        return Ok(cached);
      }
    }
    self
      .log
      .status(format!("Retrieving directory listing of \"{path}\""));
    let listing = match self.primary.list(path).await {
      Ok(listing) => listing,
      Err(error) => {
        self.log.error(format!(
          "Failed to retrieve directory listing of \"{path}\": {error}"
        ));
        return Err(error);
      }
    };
    self.log.status(format!(
      "Directory listing of \"{path}\" successful ({} entries)",
      listing.len()
    ));
    self.cache.put(path, listing.clone());
    Ok(listing)
  }

  pub fn invalidate(&self, path: &str) {
    self.cache.invalidate(path);
    self.cache.invalidate(&parent(path));
  }

  pub async fn close(&self) {
    if let Some(task) = self.keepalive_task.lock().take() {
      task.abort();
    }
    let pooled: Vec<_> = self.idle_pool.lock().await.drain(..).collect();
    for client in pooled {
      client.disconnect().await;
    }
    self.primary.disconnect().await;
  }
}

pub struct SessionManager {
  app: AppHandle,
  broker: Arc<PromptBroker>,
  trust: Arc<TrustStore>,
  settings: Arc<SettingsStore>,
  sessions: parking_lot::RwLock<HashMap<String, Arc<Session>>>,
}

impl SessionManager {
  pub fn new(
    app: AppHandle,
    broker: Arc<PromptBroker>,
    trust: Arc<TrustStore>,
    settings: Arc<SettingsStore>,
  ) -> Self {
    Self {
      app,
      broker,
      trust,
      settings,
      sessions: parking_lot::RwLock::new(HashMap::new()),
    }
  }

  pub fn get(&self, id: &str) -> VResult<Arc<Session>> {
    self
      .sessions
      .read()
      .get(id)
      .cloned()
      .ok_or(VError::InvalidSession)
  }

  pub fn all(&self) -> Vec<Arc<Session>> {
    self.sessions.read().values().cloned().collect()
  }

  pub async fn connect(&self, request: ConnectRequest) -> VResult<SessionInfo> {
    let session_id = uuid::Uuid::new_v4().to_string();
    let log = Logger::new(Some(self.app.clone()), Some(session_id.clone()));
    let site = request.site;
    let password = request.password;
    let client: Arc<dyn RemoteClient> = match site.protocol {
      Protocol::Sftp => {
        let handler = Arc::new(SessionPromptHandler {
          app: self.app.clone(),
          broker: self.broker.clone(),
          session_id: session_id.clone(),
          password: parking_lot::Mutex::new(password.clone()),
          password_uses: AtomicU32::new(0),
          log: log.clone(),
        });
        let options = ssh_options(&site, request.accept_new_hostkey);
        Arc::new(sftp::connect(options, log.clone(), handler).await?)
      }
      _ => {
        connect_ftp(
          &site,
          password.clone().unwrap_or_default(),
          self.trust.clone(),
          log.clone(),
        )
        .await?
      }
    };
    let initial_dir = match site.remote_dir.trim() {
      "" => client.initial_dir().await?,
      dir => match client.realpath(dir).await {
        Ok(resolved) => resolved,
        Err(e) => {
          log.error(format!(
            "Default remote directory unavailable ({e}); using home"
          ));
          client.initial_dir().await?
        }
      },
    };
    let supported_hashes = client.supported_hashes();
    let ttl = Duration::from_secs(self.settings.get().cache_ttl_secs.max(1));
    let session = Arc::new(Session {
      id: session_id.clone(),
      site: site.clone(),
      log: log.clone(),
      primary: client,
      password: parking_lot::Mutex::new(password),
      idle_pool: Mutex::new(Vec::new()),
      pooled_count: AtomicUsize::new(0),
      trust: self.trust.clone(),
      cache: ListingCache::new(ttl),
      keepalive_task: parking_lot::Mutex::new(None),
    });
    spawn_keepalive(&session);
    self.sessions.write().insert(session_id.clone(), session);
    Ok(SessionInfo {
      id: session_id,
      site,
      initial_dir,
      supported_hashes,
    })
  }

  pub async fn disconnect(&self, id: &str) {
    let removed = self.sessions.write().remove(id);
    if let Some(session) = removed {
      session.close().await;
    }
  }

  pub async fn disconnect_all(&self) {
    let all: Vec<_> = self.sessions.write().drain().map(|(_, s)| s).collect();
    for session in all {
      session.close().await;
    }
  }
}

fn spawn_keepalive(session: &Arc<Session>) {
  let interval = session.site.keepalive_secs;
  if interval == 0 {
    return;
  }
  let weak = Arc::downgrade(session);
  let task = tokio::spawn(async move {
    let mut ticker = tokio::time::interval(Duration::from_secs(interval as u64));
    ticker.tick().await;
    loop {
      ticker.tick().await;
      let Some(session) = weak.upgrade() else { break };
      if let Err(e) = session.primary.keepalive().await {
        session.log.error(format!("Keepalive failed: {e}"));
      }
    }
  });
  *session.keepalive_task.lock() = Some(task);
}

fn ssh_options(site: &SiteConfig, accept_new_hostkey: bool) -> SshOptions {
  let key_only = site.logon_type == LogonType::KeyFile;
  SshOptions {
    host: site.host.trim().to_string(),
    port: site.effective_port(),
    user: (!site.user.is_empty()).then(|| site.user.clone()),
    identity_file: (!site.sftp.key_path.trim().is_empty()).then(|| site.sftp.key_path.clone()),
    proxy_jump: (!site.sftp.proxy_jump.trim().is_empty()).then(|| site.sftp.proxy_jump.clone()),
    extra_options: site.sftp.extra_options.clone(),
    keepalive_secs: site.keepalive_secs,
    connect_timeout_secs: site.timeout_secs,
    auto_accept_hostkey: accept_new_hostkey,
    disable_password: key_only && !site.sftp.use_agent,
  }
}

async fn connect_ftp(
  site: &SiteConfig,
  password: String,
  trust: Arc<TrustStore>,
  log: Arc<Logger>,
) -> VResult<Arc<dyn RemoteClient>> {
  let (user, password) = match site.logon_type {
    LogonType::Anonymous => (String::new(), String::new()),
    _ => (site.user.clone(), password),
  };
  let options = FtpOptions {
    protocol: site.protocol,
    host: site.host.trim().to_string(),
    port: site.effective_port(),
    user,
    password,
    mode: match site.transfer_mode {
      TransferMode::Default => TransferMode::Passive,
      other => other,
    },
    ascii: site.ascii_mode,
    encoding: site.encoding.clone(),
    timeout_secs: site.timeout_secs as u64,
    trust,
  };
  Ok(Arc::new(FtpClient::connect(options, log).await?))
}
