use std::path::PathBuf;

pub fn data_dir() -> PathBuf {
  let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
  let dir = base.join("Vaultra");
  let _ = std::fs::create_dir_all(&dir);
  dir
}

pub fn sites_file() -> PathBuf {
  data_dir().join("sites.json")
}

pub fn settings_file() -> PathBuf {
  data_dir().join("settings.json")
}

pub fn queue_file() -> PathBuf {
  data_dir().join("queue.json")
}

pub fn trusted_certs_file() -> PathBuf {
  data_dir().join("trusted_certs.json")
}

pub fn recent_file() -> PathBuf {
  data_dir().join("recent.json")
}

pub fn edit_temp_dir() -> PathBuf {
  let dir = std::env::temp_dir().join("Vaultra").join("edit");
  let _ = std::fs::create_dir_all(&dir);
  dir
}
