pub mod askpass;
mod commands;
mod credentials;
mod edit;
pub mod error;
mod icons;
mod local_fs;
pub mod logging;
mod paths;
pub mod protocol;
mod session;
mod settings;
mod sites;
mod sync;
mod transfer;

use commands::AppState;
use std::sync::Arc;
use tauri::Manager;

pub fn run() {
  if askpass::maybe_run_askpass_client() {
    return;
  }
  let _log_guard = logging::init_file_logging();
  let _ = rustls::crypto::ring::default_provider().install_default();

  tauri::Builder::default()
    .plugin(tauri_plugin_dialog::init())
    .plugin(tauri_plugin_opener::init())
    .setup(|app| {
      let handle = app.handle().clone();
      let credentials: Arc<dyn credentials::CredentialStore> =
        Arc::new(credentials::PlatformCredentialStore::default());
      let sites = Arc::new(sites::SiteStore::load(credentials));
      let settings = Arc::new(settings::SettingsStore::load());
      let broker = Arc::new(askpass::PromptBroker::default());
      let trust = protocol::ftp::tls::TrustStore::load();
      let sessions = Arc::new(session::SessionManager::new(
        handle.clone(),
        broker.clone(),
        trust.clone(),
        settings.clone(),
      ));
      let queue = transfer::TransferQueue::new(handle.clone(), sessions.clone(), settings.clone());
      let editor = Arc::new(edit::EditManager::new(handle.clone()));
      let icons = Arc::new(icons::IconCache::default());
      app.manage(AppState {
        sites,
        settings,
        sessions,
        queue,
        broker,
        trust,
        editor,
        icons,
      });
      Ok(())
    })
    .on_window_event(|window, event| {
      if let tauri::WindowEvent::Destroyed = event {
        let state = window.state::<AppState>();
        let sessions = state.sessions.clone();
        tauri::async_runtime::block_on(async move {
          sessions.disconnect_all().await;
        });
      }
    })
    .invoke_handler(tauri::generate_handler![
      commands::system_info,
      commands::settings_get,
      commands::settings_save,
      commands::sites_get,
      commands::sites_save,
      commands::site_password_get,
      commands::site_password_set,
      commands::sites_import_filezilla,
      commands::sites_import_winscp,
      commands::recent_get,
      commands::recent_clear,
      commands::session_connect,
      commands::session_disconnect,
      commands::prompt_answer,
      commands::hostkey_forget,
      commands::certificate_trust,
      commands::remote_list,
      commands::remote_realpath,
      commands::remote_stat,
      commands::remote_mkdir,
      commands::remote_touch,
      commands::remote_rename,
      commands::remote_chmod,
      commands::remote_delete,
      commands::remote_resolve_link,
      commands::remote_edit_open,
      commands::local_list,
      commands::local_home,
      commands::local_mkdir,
      commands::local_touch,
      commands::local_rename,
      commands::local_delete,
      commands::local_resolve_link,
      commands::queue_add,
      commands::queue_snapshot,
      commands::queue_stats,
      commands::queue_pause,
      commands::queue_resume,
      commands::queue_remove,
      commands::queue_move,
      commands::queue_priority,
      commands::queue_pause_all,
      commands::queue_resume_all,
      commands::queue_retry_failed,
      commands::queue_remove_failed,
      commands::queue_clear_history,
      commands::conflict_answer,
      commands::sync_compare,
      commands::open_path,
      commands::file_icons,
    ])
    .run(tauri::generate_context!())
    .expect("failed to start Vaultra");
}
