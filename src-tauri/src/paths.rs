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

pub fn drag_temp_root() -> PathBuf {
  std::env::temp_dir().join("Vaultra").join("drag")
}

pub fn new_drag_temp_dir() -> std::io::Result<PathBuf> {
  let dir = drag_temp_root().join(uuid::Uuid::new_v4().simple().to_string());
  std::fs::create_dir_all(&dir)?;
  Ok(dir)
}

pub fn clear_drag_temp() {
  let _ = std::fs::remove_dir_all(drag_temp_root());
}

pub fn edit_temp_dir() -> PathBuf {
  let dir = std::env::temp_dir().join("Vaultra").join("edit");
  let _ = std::fs::create_dir_all(&dir);
  dir
}
