//! Opt-in timings for the disposable CanvasFixture used by the scroll pilots.
//! Start it at the top manually; this test never launches or activates an app.

#![cfg(target_os = "macos")]

use std::time::{Duration, Instant};

use auv_driver::{Capture, DriverResult, InputActionResult, InputPolicy, Scroll, ScrollOptions, WindowPoint};
use auv_scan::{
  ScrollUntilCondition, ScrollUntilDecision, ScrollUntilObserve, ScrollUntilRequest, ScrollUntilStep, ScrollUntilStopReason,
  ScrollUntilSurface, WindowScrollUntilSurface, scroll_until,
};

// The existing external surface is the clock boundary; production stays untouched.
struct TimedSurface<'a> {
  inner: WindowScrollUntilSurface<'a>,
  input_ms: Vec<f64>,
  wait_ms: Vec<f64>,
  capture_ms: Vec<f64>,
  ocr_ms: Vec<f64>,
}

impl ScrollUntilSurface for TimedSurface<'_> {
  fn scroll(&mut self, step: &ScrollUntilStep) -> DriverResult<(InputActionResult, Scroll)> {
    let start = Instant::now();
    let result = self.inner.scroll(step);
    self.input_ms.push(start.elapsed().as_secs_f64() * 1000.0);
    result
  }

  fn capture(&mut self) -> DriverResult<Capture> {
    let start = Instant::now();
    let result = self.inner.capture();
    self.capture_ms.push(start.elapsed().as_secs_f64() * 1000.0);
    result
  }

  fn recognize_text(&mut self, capture: &Capture) -> DriverResult<auv_driver::TextRecognition> {
    let start = Instant::now();
    let result = self.inner.recognize_text(capture);
    self.ocr_ms.push(start.elapsed().as_secs_f64() * 1000.0);
    result
  }

  fn wait(&mut self, duration: Duration) -> DriverResult<()> {
    let start = Instant::now();
    let result = self.inner.wait(duration);
    self.wait_ms.push(start.elapsed().as_secs_f64() * 1000.0);
    result
  }
}

#[test]
#[ignore = "requires CanvasFixture at the top and AUV_SCROLL_PROFILE_OUTPUT; never activates it"]
fn profile_canvas_deep_scroll_stages() {
  let output = std::path::PathBuf::from(std::env::var_os("AUV_SCROLL_PROFILE_OUTPUT").expect("set a task-local output JSON path"));
  let session = auv_driver::open_local().unwrap();
  let windows: Vec<_> = session
    .window()
    .list()
    .unwrap()
    .into_iter()
    .filter(|window| {
      window.app_bundle_id.as_deref() == Some("local.auv.CanvasFixture")
        && window.title.as_deref() == Some("AUV Canvas Ledger - Synthetic Benchmark")
    })
    .collect();
  assert_eq!(windows.len(), 1, "requires one disposable fixture window");
  let mut surface = TimedSurface {
    inner: WindowScrollUntilSurface::new(
      &session,
      windows[0].clone(),
      WindowPoint::new(450.0, 350.0),
      ScrollOptions {
        policy: InputPolicy::BackgroundOnly,
        ..ScrollOptions::default()
      },
    ),
    input_ms: Vec::new(),
    wait_ms: Vec::new(),
    capture_ms: Vec::new(),
    ocr_ms: Vec::new(),
  };
  let request = ScrollUntilRequest {
    step: ScrollUntilStep::Instant {
      delta: Scroll::new(0.0, 420.0),
    },
    condition: ScrollUntilCondition::TextVisible {
      query: "Kestrel handoff".into(),
    },
    max_steps: 40,
    settle: Duration::from_millis(300),
    no_motion_confirmations: 2,
    motion_region: None,
    observe: ScrollUntilObserve { text: false },
  };
  let mut final_capture = None;
  let start = Instant::now();
  let result = scroll_until(&mut surface, &request, &mut |observation| {
    final_capture = Some(observation.capture);
    Ok(ScrollUntilDecision::Continue)
  })
  .unwrap();
  let total_ms = start.elapsed().as_secs_f64() * 1000.0;
  // NOTICE: The remainder includes pixel crop/copy/comparison, text matching,
  // observer bookkeeping and capture drops. It is not a pure CPU pixel timer.
  let boundaries_ms: f64 = surface.input_ms.iter().chain(&surface.wait_ms).chain(&surface.capture_ms).chain(&surface.ocr_ms).sum();
  let capture = final_capture.unwrap();
  let evidence = output.with_extension("png");
  capture.image.save(&evidence).unwrap(); // Encoding and filesystem IO excluded.
  let metrics = serde_json::json!({
    "profile": if cfg!(debug_assertions) { "debug" } else { "release" },
    "total_ms": total_ms,
    "input_ms": surface.input_ms,
    "wait_ms": surface.wait_ms,
    "capture_ms": surface.capture_ms,
    "ocr_ms": surface.ocr_ms,
    "loop_remainder_ms": total_ms - boundaries_ms,
    "capture_backend": capture.backend,
    "capture_fallback_reason": capture.fallback_reason,
    "pixel_dimensions": capture.image.dimensions(),
    "result": result,
  });
  std::fs::write(output, serde_json::to_vec_pretty(&metrics).unwrap()).unwrap();
  assert_eq!(result.reason, ScrollUntilStopReason::TextVisible);
  assert_eq!(result.steps, 22, "reset the fixture to the top before profiling");
  assert_eq!(result.text_match.unwrap().text, "Kestrel handoff | VK-7392 | Ready for review");
  assert_eq!(result.action.unwrap().focus_disturbance, auv_driver::DisturbanceLevel::None);
}
