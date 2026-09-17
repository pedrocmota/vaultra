mod conflict;
mod queue;
mod throttle;
mod worker;

pub use conflict::{ConflictAnswer, ConflictPolicy};
pub use queue::{QueueRequest, QueueSnapshot, QueueStats, TransferQueue};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
  Download,
  Upload,
  Copy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferStatus {
  Queued,
  Active,
  Paused,
  Failed,
  Done,
  Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferItem {
  pub id: String,
  pub session_id: Option<String>,
  pub site_id: String,
  pub site_name: String,
  pub direction: Direction,
  pub local_path: String,
  pub remote_path: String,
  pub is_dir: bool,
  pub size: Option<u64>,
  pub source_mtime: Option<i64>,
  pub transferred: u64,
  pub status: TransferStatus,
  pub priority: i32,
  pub error: Option<String>,
  pub attempts: u32,
  pub created_at: i64,
  pub finished_at: Option<i64>,
  pub speed_bps: u64,
  pub follow_symlink: bool,
  pub conflict_policy: Option<ConflictPolicy>,
  pub link_target: Option<String>,
  #[serde(default)]
  pub batch: Option<String>,
  #[serde(default)]
  pub target_session_id: Option<String>,
  #[serde(default)]
  pub target_path: Option<String>,
  #[serde(skip)]
  pub retry_after: Option<i64>,
}

impl TransferItem {
  pub fn copy_target(&self) -> &str {
    self.target_path.as_deref().unwrap_or_default()
  }

  pub fn is_pending(&self) -> bool {
    matches!(
      self.status,
      TransferStatus::Queued | TransferStatus::Active | TransferStatus::Paused
    )
  }
}
