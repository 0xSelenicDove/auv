#[cfg(target_os = "macos")]
use super::binding::ffi::{NativeWindowCaptureRequest, NativeWindowCaptureResponse, capture_window_image, window_ax_size};
use super::types::AuvResult;

#[cfg(target_os = "macos")]
static SCREEN_CAPTURE_OWNER: std::sync::Mutex<Option<std::fs::File>> = std::sync::Mutex::new(None);

/// ScreenCaptureKit's connection is process-lived, even between screenshots.
#[cfg(target_os = "macos")]
pub(crate) fn claim_screen_capture() -> AuvResult<()> {
  retain_capture_owner(&SCREEN_CAPTURE_OWNER, &std::env::temp_dir().join("auv-macos-screen-capture-owner.lock"))
}

#[cfg(target_os = "macos")]
fn retain_capture_owner(owner: &std::sync::Mutex<Option<std::fs::File>>, path: &std::path::Path) -> AuvResult<()> {
  use std::os::unix::fs::OpenOptionsExt;
  let mut owner = owner.lock().map_err(|error| format!("screen capture ownership lock failed: {error}"))?;
  if owner.is_some() {
    return Ok(());
  }
  let file = std::fs::OpenOptions::new()
    .read(true)
    .write(true)
    .create(true)
    .truncate(false)
    .mode(0o600)
    .open(path)
    .map_err(|error| format!("screen capture ownership file failed: {error}"))?;
  match file.try_lock() {
    Ok(()) => {
      // NOTICE: two live AUV capture clients reproducibly caused replayd
      // connection-reset loops and 40-second failures, even with one idle.
      // Keep ownership until process exit: SCScreenshotManager has no public
      // connection-close API. Never unlink an advisory lock's shared inode.
      // ponytail: one owner per user; relax when concurrent-client probes pass.
      // See `docs/ai/references/driver/2026-10-07-runner-capture-stall-fix.md`.
      *owner = Some(file);
      Ok(())
    }
    Err(std::fs::TryLockError::WouldBlock) => Err(
      "another AUV process owns macOS screen capture; reuse its Runner with --device-id or --run, or wait for that process to exit".into(),
    ),
    Err(std::fs::TryLockError::Error(error)) => Err(format!("screen capture ownership failed: {error}")),
  }
}

#[derive(Clone, Debug, PartialEq)]
pub struct NativeWindowCapture {
  pub image_width: i64,
  pub image_height: i64,
  /// Window frame in points at capture time, as ScreenCaptureKit saw it.
  pub window_frame: auv_driver_common::Rect,
  pub rgba_bytes: Vec<u8>,
}

#[cfg(target_os = "macos")]
/// Captures a window at its backing resolution, or at one pixel per point
/// when `logical`.
pub fn capture_window_rgba(window_id: i64, logical: bool) -> AuvResult<NativeWindowCapture> {
  decode_window_capture_response(capture_window_image(NativeWindowCaptureRequest { window_id, logical }))
}

#[cfg(not(target_os = "macos"))]
pub fn capture_window_rgba(_window_id: i64, _logical: bool) -> AuvResult<NativeWindowCapture> {
  Err("macOS native window capture is unsupported on this target".to_string())
}

#[cfg(target_os = "macos")]
fn decode_window_capture_response(response: NativeWindowCaptureResponse) -> AuvResult<NativeWindowCapture> {
  if response.error_message.is_some() {
    return super::error::native_result("capture_window_image", None, response.error_message, response.recovery_hint);
  }
  let expected_len = response
    .image_width
    .checked_mul(response.image_height)
    .and_then(|pixels| pixels.checked_mul(4))
    .ok_or_else(|| "native window capture dimensions overflowed".to_string())?;
  if expected_len < 0 || response.rgba_bytes.len() != expected_len as usize {
    return Err(format!(
      "native window capture returned {} RGBA bytes for {}x{} image; expected {}",
      response.rgba_bytes.len(),
      response.image_width,
      response.image_height,
      expected_len
    ));
  }
  Ok(NativeWindowCapture {
    image_width: response.image_width,
    image_height: response.image_height,
    window_frame: auv_driver_common::Rect::new(response.window_x, response.window_y, response.window_width, response.window_height),
    rgba_bytes: response.rgba_bytes,
  })
}

/// A window's size and minimized state as its application reports them over
/// Accessibility.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowAxSize {
  pub size: auv_driver_common::Size,
  pub minimized: bool,
}

/// `None` when the window has no AX element or Accessibility is not granted.
#[cfg(target_os = "macos")]
pub fn window_ax_size_for(pid: u32, window_number: i64) -> Option<WindowAxSize> {
  let response = window_ax_size(i64::from(pid), window_number);
  response.found.then(|| WindowAxSize {
    size: auv_driver_common::Size::new(response.width, response.height),
    minimized: response.minimized,
  })
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
  // ROOT CAUSE:
  // Releasing a lock after a frame would admit a second client while the
  // first process's ScreenCaptureKit connection remained alive.
  #[test]
  fn screen_capture_owner_survives_frames_and_releases_with_its_process_handle() {
    let path = std::env::temp_dir().join(format!(
      "auv-capture-owner-test-{}-{}.lock",
      std::process::id(),
      std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    let first = std::sync::Mutex::new(None);
    let second = std::sync::Mutex::new(None);
    super::retain_capture_owner(&first, &path).unwrap();
    super::retain_capture_owner(&first, &path).unwrap();
    assert!(super::retain_capture_owner(&second, &path).unwrap_err().contains("reuse its Runner"));
    drop(first);
    super::retain_capture_owner(&second, &path).unwrap();
    drop(second);
    std::fs::remove_file(path).unwrap();
  }
  // Live capture regression for the canvas benchmark failures. Exercise the
  // native boundary without input, OCR, tracing or overlapping recovery.
  #[test]
  #[ignore = "requires a capturable synthetic window; set AUV_CAPTURE_TEST_WINDOW_ID"]
  fn repeated_window_capture_completes_without_backend_failure() {
    super::claim_screen_capture().unwrap();
    let id = std::env::var("AUV_CAPTURE_TEST_WINDOW_ID").unwrap().parse().unwrap();
    // Match first-party callers: initialize WindowServer via window resolution.
    super::super::window::list_windows(super::super::window::ListWindowsOptions::app(256, "local.auv.RepeatedSearchFixture")).unwrap();
    std::thread::scope(|scope| {
      for _ in 0..2 {
        scope.spawn(|| {
          for iteration in 0..32 {
            let started = std::time::Instant::now();
            let capture = super::capture_window_rgba(id, false).unwrap_or_else(|error| panic!("capture {iteration}: {error}"));
            assert!(capture.image_width > 0 && capture.image_height > 0);
            assert_eq!(capture.rgba_bytes.len() as i64, capture.image_width * capture.image_height * 4);
            eprintln!("capture {iteration}: {:.3}s", started.elapsed().as_secs_f64());
          }
        });
      }
    });
  }
}
