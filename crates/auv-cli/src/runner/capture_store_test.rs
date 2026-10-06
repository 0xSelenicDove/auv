use std::time::{Duration, Instant};

use super::*;

/// A capture of `pixels` RGBA pixels (4 bytes each).
fn capture(pixels: u32) -> auv_driver::Capture {
  auv_driver::Capture {
    origin: None,
    image: image::RgbaImage::new(pixels, 1),
    bounds: auv_driver::Rect::new(0.0, 0.0, f64::from(pixels), 1.0),
    scale_factor: 1.0,
    backend: "test".to_string(),
    fallback_reason: None,
  }
}

fn store(budget_bytes: usize, idle: Duration) -> CaptureStore {
  CaptureStore::new(CaptureStoreOptions { budget_bytes, idle })
}

#[test]
fn evicts_the_least_recently_used_capture_when_over_budget() {
  let store = store(100, Duration::from_secs(60));
  let start = Instant::now();
  let first = store.insert_at(capture(10), start);
  let second = store.insert_at(capture(10), start + Duration::from_secs(1));
  // Reading `first` makes `second` the least recently used.
  assert!(store.get_at(&first, start + Duration::from_secs(2)).is_some());

  let third = store.insert_at(capture(10), start + Duration::from_secs(3));

  assert!(store.get_at(&first, start + Duration::from_secs(4)).is_some());
  assert!(store.get_at(&second, start + Duration::from_secs(4)).is_none());
  assert!(store.get_at(&third, start + Duration::from_secs(4)).is_some());
  assert_eq!(store.total_bytes(), 80);
}

#[test]
fn expires_captures_idle_longer_than_the_expiry() {
  let store = store(1_000, Duration::from_secs(60));
  let start = Instant::now();
  let stale = store.insert_at(capture(10), start);
  let fresh = store.insert_at(capture(10), start + Duration::from_secs(50));

  assert!(store.get_at(&stale, start + Duration::from_secs(61)).is_none());
  assert!(store.get_at(&fresh, start + Duration::from_secs(61)).is_some());
  assert_eq!(store.total_bytes(), 40);
}

#[test]
fn keeps_a_single_capture_larger_than_the_budget_until_the_next_insert() {
  let store = store(16, Duration::from_secs(60));
  let start = Instant::now();
  let large = store.insert_at(capture(10), start);
  assert!(store.get_at(&large, start).is_some());

  let next = store.insert_at(capture(2), start + Duration::from_secs(1));

  assert!(store.get_at(&large, start + Duration::from_secs(1)).is_none());
  assert!(store.get_at(&next, start + Duration::from_secs(1)).is_some());
  assert_eq!(store.total_bytes(), 8);
}

#[test]
fn unknown_references_are_not_found() {
  let store = store(100, Duration::from_secs(60));
  assert!(store.get("cap-0-0").is_none());
}
