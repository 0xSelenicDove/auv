//! Runner-owned store of captures that clients address by `CaptureRef`.
//!
//! Captures stay in the Runner that produced them; responses carry a
//! reference and metadata, and `GetCaptureImage` is the only call that moves
//! AUV-produced pixels to a client (see "Image Payloads" in `AGENTS.md`).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Environment variable overriding the store budget, in MiB.
pub(super) const BUDGET_MIB_ENV: &str = "AUV_CAPTURE_STORE_BUDGET_MIB";
/// Environment variable overriding the idle expiry, in seconds.
pub(super) const IDLE_SECONDS_ENV: &str = "AUV_CAPTURE_STORE_IDLE_SECONDS";

/// Store limits. Captures leave the store when they exceed the byte budget
/// (least recently used first) or stay unused longer than `idle`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CaptureStoreOptions {
  pub budget_bytes: usize,
  pub idle: Duration,
}

impl Default for CaptureStoreOptions {
  fn default() -> Self {
    // NOTICE(capture-store-defaults): 512 MiB holds about 20 Retina window
    // captures (~25 MB) or 6 display captures (~80 MB), enough for
    // capture -> OCR -> click loops. Ten idle minutes keep a long-lived local
    // Runner from holding pixels of finished workflows. The Runner cannot see
    // Runs (the daemon strips Run routing metadata), so expiry is time based.
    // TODO(capture-store-run-release): release a Run's captures when it stops
    // once Runners receive Run identity and stop notifications.
    Self {
      budget_bytes: 512 * 1024 * 1024,
      idle: Duration::from_secs(10 * 60),
    }
  }
}

impl CaptureStoreOptions {
  /// Defaults, overridden by `AUV_CAPTURE_STORE_BUDGET_MIB` and
  /// `AUV_CAPTURE_STORE_IDLE_SECONDS` when they hold positive integers.
  pub(super) fn from_env() -> Self {
    let mut options = Self::default();
    if let Some(mib) = positive_env(BUDGET_MIB_ENV) {
      options.budget_bytes = usize::try_from(mib).unwrap_or(usize::MAX).saturating_mul(1024 * 1024);
    }
    if let Some(seconds) = positive_env(IDLE_SECONDS_ENV) {
      options.idle = Duration::from_secs(seconds);
    }
    options
  }
}

fn positive_env(name: &str) -> Option<u64> {
  std::env::var(name).ok()?.trim().parse::<u64>().ok().filter(|value| *value > 0)
}

#[derive(Clone)]
pub(super) struct CaptureStore {
  options: CaptureStoreOptions,
  inner: Arc<Mutex<Inner>>,
}

#[derive(Default)]
struct Inner {
  entries: HashMap<String, Entry>,
  total_bytes: usize,
  next: u64,
}

struct Entry {
  capture: Arc<auv_driver::Capture>,
  bytes: usize,
  last_used: Instant,
}

impl CaptureStore {
  pub(super) fn new(options: CaptureStoreOptions) -> Self {
    Self {
      options,
      inner: Arc::new(Mutex::new(Inner::default())),
    }
  }

  /// Stores a capture and returns its reference ID. Older captures are evicted
  /// (least recently used first) until the store fits its budget; a single
  /// capture larger than the whole budget is still kept until the next insert.
  pub(super) fn insert(&self, capture: auv_driver::Capture) -> String {
    self.insert_at(capture, Instant::now())
  }

  fn insert_at(&self, capture: auv_driver::Capture, now: Instant) -> String {
    let bytes = capture.image.as_raw().len();
    let mut inner = self.inner.lock().expect("capture store lock");
    inner.expire(now, self.options.idle);
    while inner.total_bytes + bytes > self.options.budget_bytes && inner.evict_least_recent() {}
    inner.next += 1;
    // The process ID keeps references from a restarted Runner from colliding.
    let id = format!("cap-{}-{}", std::process::id(), inner.next);
    inner.total_bytes += bytes;
    inner.entries.insert(
      id.clone(),
      Entry {
        capture: Arc::new(capture),
        bytes,
        last_used: now,
      },
    );
    id
  }

  /// Returns a stored capture and marks it used, or `None` once it was
  /// evicted, expired, or never produced by this Runner.
  pub(super) fn get(&self, id: &str) -> Option<Arc<auv_driver::Capture>> {
    self.get_at(id, Instant::now())
  }

  fn get_at(&self, id: &str, now: Instant) -> Option<Arc<auv_driver::Capture>> {
    let mut inner = self.inner.lock().expect("capture store lock");
    inner.expire(now, self.options.idle);
    let entry = inner.entries.get_mut(id)?;
    entry.last_used = now;
    Some(Arc::clone(&entry.capture))
  }

  /// Drops captures idle longer than the expiry; called periodically so an
  /// unused Runner also gives its memory back.
  pub(super) fn sweep(&self) {
    self.inner.lock().expect("capture store lock").expire(Instant::now(), self.options.idle);
  }

  #[cfg(test)]
  fn total_bytes(&self) -> usize {
    self.inner.lock().expect("capture store lock").total_bytes
  }
}

impl Inner {
  fn expire(&mut self, now: Instant, idle: Duration) {
    let expired: Vec<String> =
      self.entries.iter().filter(|(_, entry)| now.saturating_duration_since(entry.last_used) > idle).map(|(id, _)| id.clone()).collect();
    for id in expired {
      self.remove(&id);
    }
  }

  /// Evicts the least recently used capture; false when the store is empty.
  fn evict_least_recent(&mut self) -> bool {
    let Some(id) = self.entries.iter().min_by_key(|(_, entry)| entry.last_used).map(|(id, _)| id.clone()) else {
      return false;
    };
    self.remove(&id);
    true
  }

  fn remove(&mut self, id: &str) {
    if let Some(entry) = self.entries.remove(id) {
      self.total_bytes -= entry.bytes;
    }
  }
}

#[cfg(test)]
#[path = "capture_store_test.rs"]
mod tests;
