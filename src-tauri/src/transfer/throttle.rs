use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

const BURST_SECS: f64 = 0.5;

pub struct Throttle {
  limit_bps: AtomicU64,
  state: parking_lot::Mutex<Bucket>,
}

struct Bucket {
  last: Instant,
  tokens: f64,
}

impl Throttle {
  pub fn new(limit_kbps: u64) -> Self {
    Self {
      limit_bps: AtomicU64::new(limit_kbps * 1024),
      state: parking_lot::Mutex::new(Bucket {
        last: Instant::now(),
        tokens: 0.0,
      }),
    }
  }

  pub fn set_limit_kbps(&self, limit_kbps: u64) {
    self.limit_bps.store(limit_kbps * 1024, Ordering::Relaxed);
  }

  pub async fn consume(&self, bytes: usize) {
    let limit = self.limit_bps.load(Ordering::Relaxed);
    if limit == 0 {
      return;
    }
    let wait = {
      let mut bucket = self.state.lock();
      let rate = limit as f64;
      let now = Instant::now();
      let elapsed = now.duration_since(bucket.last).as_secs_f64();
      bucket.last = now;
      bucket.tokens = (bucket.tokens + elapsed * rate).min(rate * BURST_SECS);
      let needed = bytes as f64;
      if bucket.tokens >= needed {
        bucket.tokens -= needed;
        None
      } else {
        let deficit = needed - bucket.tokens;
        bucket.tokens = 0.0;
        Some(Duration::from_secs_f64(deficit / rate))
      }
    };
    if let Some(delay) = wait {
      tokio::time::sleep(delay).await;
    }
  }
}
