//! Live verification binary for Minecraft AgentMemoryLoop (Step 7).
//!
//! Validates the closed-loop pipeline running against a live Minecraft client:
//! 1. Resolves Minecraft game window via `auv-driver` Window API.
//! 2. Tails live telemetry from Minecraft Fabric mod (`telemetry.jsonl`).
//! 3. Captures real game window frames (`session.window().capture()`).
//! 4. Executes 1Hz `AgentMemoryLoop.tick(&LiveCapture)` across 300 ticks (5 minutes).
//! 5. Evaluates Phase A (observation-only statistics) and conditional Phase B (bounded clicks).

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::thread::sleep;
use std::time::{Duration, Instant};

use image::DynamicImage;
use serde::{Deserialize, Serialize};

use auv_driver::WindowInput as _;
use auv_driver::geometry::WindowPoint;
use auv_game_minecraft::agent_memory_loop::{AgentMemoryLoop, AgentMemoryLoopConfig, LiveCapture, TickReport};
use auv_game_minecraft::ingest::read_latest_spatial_frame_from_tail;
use auv_game_minecraft::memory_action_wiring::{ActionExecutor, MemoryActionQuery, wire_memory_query_to_action};
use auv_game_minecraft::spatial_memory_store::SpatialMemoryStore;
use auv_game_minecraft::visual_perception::{BlockDetector, BlockDetectorConfig, DepthEstimator};

/// Live window action executor for Phase B.
struct LiveWindowActionExecutor<'a> {
  session: &'a auv_driver::LocalDriverSession,
  window: &'a auv_driver::Window,
}

impl ActionExecutor for LiveWindowActionExecutor<'_> {
  fn click(&self, point: WindowPoint) -> Result<auv_driver::InputActionResult, String> {
    self
      .session
      .window()
      .click(
        self.window,
        point,
        auv_driver::ClickOptions {
          policy: auv_driver::InputPolicy::ForegroundPreferred,
          click: auv_driver::Click::Single,
          window_strategy: auv_driver::WindowClickStrategy::default(),
        },
      )
      .map_err(|e| format!("live window click failed: {e}"))
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PhaseBClickRecord {
  tick: usize,
  target_label: String,
  projected_point: Option<[f64; 2]>,
  attempted: bool,
  refusal_reason: Option<String>,
  limits: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TickAuditEntry {
  tick: usize,
  elapsed_ms: u64,
  telemetry_timestamp_ms: u64,
  raycast_hit: bool,
  raycast_block: Option<String>,
  capture_source: String,
  detections: Vec<String>,
  landmarks_created: usize,
  landmarks_merged: usize,
  visual_skipped_reason: Option<String>,
  error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct FailureBreakdown {
  screenshot_failures: usize,
  telemetry_stalls: usize,
  telemetry_missing: usize,
  inference_errors: usize,
  timeouts_over_1000ms: usize,
}

#[derive(Debug, Serialize, Deserialize)]
struct LiveVerifySummary {
  phase: String,
  target_window_title: String,
  target_window_frame: String,
  model_path: String,
  depth_model_path: String,
  telemetry_path: String,
  total_ticks_requested: usize,
  total_ticks_completed: usize,
  successful_ticks: usize,
  failed_ticks: usize,
  failure_rate_percent: f64,
  failures: FailureBreakdown,
  latency_p50_ms: u64,
  latency_p95_ms: u64,
  latency_max_ms: u64,
  total_landmarks_in_store: usize,
  total_detections_count: usize,
  detections_by_class: HashMap<String, usize>,
  visual_landmarks_created_total: usize,
  raycast_landmarks_created_total: usize,
  spot_check_ticks: Vec<TickAuditEntry>,
  phase_b_clicks: Vec<PhaseBClickRecord>,
  verdict: String,
  verdict_reasons: Vec<String>,
}

struct Args {
  model_path: PathBuf,
  depth_model_path: PathBuf,
  telemetry_path: PathBuf,
  target_title: String,
  ticks: usize,
  interval_ms: u64,
  phase: String,
  max_clicks: usize,
  db_path: PathBuf,
  summary_out: PathBuf,
}

fn parse_args() -> Args {
  let mut model_path: Option<PathBuf> = None;
  let mut depth_model_path = PathBuf::from(r"F:\auv\.tmp\models\model-small.onnx");
  let mut telemetry_path = PathBuf::from(r"F:\pcl\.minecraft\versions\1.21.1-Fabric 0.16.10\auv\telemetry.jsonl");
  let mut target_title = "Minecraft".to_string();
  let mut ticks = 300;
  let mut interval_ms = 1000;
  let mut phase = "a".to_string();
  let mut max_clicks = 3;
  let mut db_path = PathBuf::from(r"F:\auv\.tmp\live_memory_store.json");
  let mut summary_out = PathBuf::from(r"F:\auv\.tmp\live_verify_report.json");

  let mut args_iter = env::args().skip(1);
  while let Some(arg) = args_iter.next() {
    match arg.as_str() {
      "--model" => {
        model_path = args_iter.next().map(PathBuf::from);
      }
      "--depth-model" => {
        if let Some(val) = args_iter.next() {
          depth_model_path = PathBuf::from(val);
        }
      }
      "--telemetry" => {
        if let Some(val) = args_iter.next() {
          telemetry_path = PathBuf::from(val);
        }
      }
      "--target-title" => {
        if let Some(val) = args_iter.next() {
          target_title = val;
        }
      }
      "--ticks" => {
        if let Some(val) = args_iter.next() {
          ticks = val.parse().unwrap_or(300);
        }
      }
      "--interval-ms" => {
        if let Some(val) = args_iter.next() {
          interval_ms = val.parse().unwrap_or(1000);
        }
      }
      "--phase" => {
        if let Some(val) = args_iter.next() {
          phase = val.to_lowercase();
        }
      }
      "--max-clicks" => {
        if let Some(val) = args_iter.next() {
          max_clicks = val.parse().unwrap_or(3);
        }
      }
      "--db-path" => {
        if let Some(val) = args_iter.next() {
          db_path = PathBuf::from(val);
        }
      }
      "--summary-out" => {
        if let Some(val) = args_iter.next() {
          summary_out = PathBuf::from(val);
        }
      }
      "--help" | "-h" => {
        println!("AUV Minecraft Live Verification Runner (Step 7)");
        println!("Usage: live_verify --model <ONNX_PATH> [OPTIONS]");
        println!();
        println!("Options:");
        println!("  --model <PATH>          Explicit path to block-detector-v1.onnx (REQUIRED)");
        println!("  --depth-model <PATH>    Path to MiDaS model-small.onnx (default: F:\\auv\\.tmp\\models\\model-small.onnx)");
        println!("  --telemetry <PATH>      Path to telemetry.jsonl (default: F:\\pcl\\...\\telemetry.jsonl)");
        println!("  --target-title <TITLE>  Target window title substring (default: Minecraft)");
        println!("  --ticks <N>             Number of ticks to execute (default: 300)");
        println!("  --interval-ms <MS>      Tick interval milliseconds (default: 1000, 1Hz)");
        println!("  --phase <a|b>           Verification phase: 'a' (observe) or 'b' (bounded clicks, default: a)");
        println!("  --max-clicks <N>        Max clicks in Phase B (default: 3)");
        println!("  --db-path <PATH>        SpatialMemoryStore persistence path");
        println!("  --summary-out <PATH>    Path to output summary JSON");
        std::process::exit(0);
      }
      other => {
        eprintln!("Unknown argument: {other}");
        std::process::exit(1);
      }
    }
  }

  // Model path must be explicitly provided and exist fail-fast per Brief
  let model_path = match model_path {
    Some(path) => {
      if !path.exists() {
        eprintln!("FATAL: Specified --model path does not exist on disk: {}", path.display());
        std::process::exit(1);
      }
      path
    }
    None => {
      eprintln!("FATAL: --model <PATH> is required. Relative CWD paths are forbidden. Please specify absolute path.");
      std::process::exit(1);
    }
  };

  if !depth_model_path.exists() {
    eprintln!("FATAL: Depth model path does not exist: {}", depth_model_path.display());
    std::process::exit(1);
  }

  if !telemetry_path.exists() {
    eprintln!("FATAL: Telemetry path does not exist: {}", telemetry_path.display());
    std::process::exit(1);
  }

  Args {
    model_path,
    depth_model_path,
    telemetry_path,
    target_title,
    ticks,
    interval_ms,
    phase,
    max_clicks,
    db_path,
    summary_out,
  }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
  let args = parse_args();

  println!("============================================================");
  println!("       AUV Minecraft Agent Live Verification (Step 7)       ");
  println!("============================================================");
  println!("Model:        {}", args.model_path.display());
  println!("Depth Model:  {}", args.depth_model_path.display());
  println!("Telemetry:    {}", args.telemetry_path.display());
  println!("Target Title: {}", args.target_title);
  println!("Target Ticks: {} (interval {} ms / 1Hz)", args.ticks, args.interval_ms);
  println!("Phase:        Phase {}", args.phase.to_uppercase());
  println!("============================================================\n");

  // 1. Locate Minecraft Window via auv-driver
  println!("[T0] Initializing local driver session...");
  let driver_session = auv_driver::open_local().map_err(|e| format!("Failed to open driver session: {e}"))?;

  println!("[T0] Enumerating desktop windows...");
  let windows = driver_session.window().list().map_err(|e| format!("Failed to list desktop windows: {e}"))?;

  let target_window = windows.iter().find(|w| {
    if let Some(title) = &w.title {
      if title.to_lowercase().contains(&args.target_title.to_lowercase()) {
        return true;
      }
    }
    if let Some(app) = &w.app_name {
      if app.to_lowercase().contains("minecraft") || app.to_lowercase().contains("javaw") {
        return true;
      }
    }
    false
  });

  let window = match target_window {
    Some(w) => {
      println!(
        "[T0] Found target window: ID={}, title={:?}, app={:?}, frame={:?}",
        w.reference.id,
        w.title.as_deref().unwrap_or("untitled"),
        w.app_name.as_deref().unwrap_or("unknown"),
        w.frame
      );
      w.clone()
    }
    None => {
      eprintln!("\nFATAL: Target window containing '{}' was NOT found on desktop!", args.target_title);
      eprintln!("Visible windows found ({}):", windows.len());
      for w in &windows {
        eprintln!(
          "  - PID {:?}, app: {:?}, title: {:?}",
          w.process_id,
          w.app_name.as_deref().unwrap_or("unknown"),
          w.title.as_deref().unwrap_or("untitled")
        );
      }
      eprintln!("\nPlease launch Minecraft Fabric 1.21.1 and enter world before running live_verify.");
      std::process::exit(1);
    }
  };

  // 2. Load Models
  println!("[T0] Loading BlockDetector from {}...", args.model_path.display());
  let mut detector_config = BlockDetectorConfig::default();
  detector_config.model_path = args.model_path.clone();
  let detector = BlockDetector::new(detector_config.clone()).map_err(|e| format!("Failed to load BlockDetector: {e}"))?;

  println!("[T0] Loading DepthEstimator from {}...", args.depth_model_path.display());
  let depth_estimator = DepthEstimator::new(&args.depth_model_path).map_err(|e| format!("Failed to load DepthEstimator: {e}"))?;

  // 3. Initialize Memory Store & Agent Memory Loop
  if args.db_path.exists() {
    let _ = fs::remove_file(&args.db_path);
  }
  let store = SpatialMemoryStore::open(&args.db_path).map_err(|e| format!("Failed to initialize SpatialMemoryStore: {e}"))?;

  let mut loop_config = AgentMemoryLoopConfig::default();
  loop_config.tick_interval_millis = args.interval_ms;
  loop_config.require_mod_telemetry = true;
  loop_config.yolo_confidence_threshold = 0.50;

  let mut agent_loop = AgentMemoryLoop::new(store, loop_config).with_models(detector, depth_estimator);

  // 4. Run Execution Loop
  println!("\n[Live Runner] Starting live tick loop ({} ticks)...", args.ticks);
  println!("| Tick | Ingest (ms) | Telemetry Time | Hit? | Hit Block | Detections | Merged | Created | Visual Reason |");
  println!("|------|-------------|----------------|------|-----------|------------|--------|---------|---------------|");

  let mut audit_entries = Vec::with_capacity(args.ticks);
  let mut latencies_ms = Vec::with_capacity(args.ticks);
  let mut class_distribution: HashMap<String, usize> = HashMap::new();
  let mut phase_b_clicks: Vec<PhaseBClickRecord> = Vec::new();

  let mut screenshot_failures = 0;
  let mut telemetry_stalls = 0;
  let mut telemetry_missing = 0;
  let mut inference_errors = 0;
  let mut timeouts_over_1000ms = 0;

  let mut last_seen_telemetry_ts: u64 = 0;
  let mut visual_landmarks_created_total = 0;
  let mut raycast_landmarks_created_total = 0;
  let mut total_detections_count = 0;

  for tick_idx in 1..=args.ticks {
    let tick_start = Instant::now();
    let mut tick_error = None;

    // A. Read Telemetry Tail
    let frame_res = read_latest_spatial_frame_from_tail(&args.telemetry_path);
    let frame = match frame_res {
      Ok(Some(f)) => {
        if f.monotonic_timestamp_ms == last_seen_telemetry_ts {
          telemetry_stalls += 1;
        } else {
          last_seen_telemetry_ts = f.monotonic_timestamp_ms;
        }
        Some(f)
      }
      Ok(None) => {
        telemetry_missing += 1;
        tick_error = Some("telemetry file empty".to_string());
        None
      }
      Err(err) => {
        telemetry_missing += 1;
        tick_error = Some(format!("telemetry read error: {err}"));
        None
      }
    };

    // B. Capture Window
    let (screenshot, capture_src) = match driver_session.window().capture(&window) {
      Ok(cap) => {
        let is_black = cap.image.pixels().take(1000).all(|p| p.0[0] == 0 && p.0[1] == 0 && p.0[2] == 0);
        if is_black && cap.image.pixels().all(|p| p.0[0] == 0 && p.0[1] == 0 && p.0[2] == 0) {
          screenshot_failures += 1;
          (None, "black_capture".to_string())
        } else {
          (Some(DynamicImage::ImageRgba8(cap.image)), "driver.window_capture".to_string())
        }
      }
      Err(err) => {
        screenshot_failures += 1;
        (None, format!("capture_error: {err}"))
      }
    };

    // C. Execute Tick if Telemetry Available
    let mut tick_report = TickReport::default();
    let mut tick_detections: Vec<String> = Vec::new();

    if let Some(f) = &frame {
      let live_capture = LiveCapture::new(
        format!("live-tick-{tick_idx:04}"),
        f.monotonic_timestamp_ms,
        Some(f.player_pose),
        f.raycast_hit.clone(),
        screenshot.clone(),
      )
      .with_viewport(f.viewport)
      .with_vertical_fov(70.0);

      match agent_loop.tick(&live_capture) {
        Ok(rep) => {
          tick_report = rep;
          raycast_landmarks_created_total += tick_report.raycast_landmarks_created;
          visual_landmarks_created_total += tick_report.visual_landmarks_created;
          for det in &tick_report.detections {
            total_detections_count += 1;
            let class_name = det.split(':').next().unwrap_or(det);
            *class_distribution.entry(class_name.to_string()).or_insert(0) += 1;
            tick_detections.push(det.clone());
          }
        }
        Err(err) => {
          inference_errors += 1;
          tick_error = Some(format!("loop tick error: {err}"));
        }
      }

      // D. Phase B: Bounded click if requested
      let attempted_clicks = phase_b_clicks.iter().filter(|c| c.attempted).count();
      if args.phase == "b" && attempted_clicks < args.max_clicks {
        let action_query = MemoryActionQuery::new("grass_block", f.player_pose).with_frame(f.clone()).with_viewport(f.viewport);
        let executor = LiveWindowActionExecutor {
          session: &driver_session,
          window: &window,
        };
        let outcome = wire_memory_query_to_action(agent_loop.store(), &action_query, &executor);
        if outcome.attempted {
          println!(">>> [Phase B] Action dispatched! Clicked point: {:?}", outcome.window_point);
          phase_b_clicks.push(PhaseBClickRecord {
            tick: tick_idx,
            target_label: "grass_block".to_string(),
            projected_point: outcome.window_point.map(|p| [p.point().x, p.point().y]),
            attempted: true,
            refusal_reason: None,
            limits: outcome.known_limits,
          });
        } else if let Some(refusal) = &outcome.refusal_reason {
          if phase_b_clicks.len() < args.max_clicks + 5 {
            phase_b_clicks.push(PhaseBClickRecord {
              tick: tick_idx,
              target_label: "grass_block".to_string(),
              projected_point: outcome.window_point.map(|p| [p.point().x, p.point().y]),
              attempted: false,
              refusal_reason: Some(refusal.clone()),
              limits: outcome.known_limits,
            });
          }
        }
      }
    }

    let elapsed = tick_start.elapsed().as_millis() as u64;
    latencies_ms.push(elapsed);
    if elapsed > 1000 {
      timeouts_over_1000ms += 1;
    }

    let hit_str =
      frame.as_ref().and_then(|f| f.raycast_hit.as_ref()).map(|h| h.block_id.replace("minecraft:", "")).unwrap_or_else(|| "-".to_string());
    let det_str = if tick_detections.is_empty() {
      "-".to_string()
    } else {
      tick_detections.join(",")
    };
    let skipped_str = tick_report.visual_skipped_reason.as_deref().unwrap_or("-");

    println!(
      "| {:4} | {:11} | {:14} | {:4} | {:9} | {:10} | {:6} | {:7} | {:13} |",
      tick_idx,
      format!("{}ms", elapsed),
      frame.as_ref().map(|f| f.monotonic_timestamp_ms.to_string()).unwrap_or_else(|| "-".to_string()),
      if frame.as_ref().and_then(|f| f.raycast_hit.as_ref()).is_some() {
        "HIT"
      } else {
        "none"
      },
      hit_str,
      det_str,
      tick_report.landmarks_merged,
      tick_report.landmarks_created,
      skipped_str,
    );

    audit_entries.push(TickAuditEntry {
      tick: tick_idx,
      elapsed_ms: elapsed,
      telemetry_timestamp_ms: frame.as_ref().map(|f| f.monotonic_timestamp_ms).unwrap_or(0),
      raycast_hit: frame.as_ref().and_then(|f| f.raycast_hit.as_ref()).is_some(),
      raycast_block: frame.as_ref().and_then(|f| f.raycast_hit.as_ref()).map(|h| h.block_id.clone()),
      capture_source: capture_src,
      detections: tick_detections,
      landmarks_created: tick_report.landmarks_created,
      landmarks_merged: tick_report.landmarks_merged,
      visual_skipped_reason: tick_report.visual_skipped_reason,
      error: tick_error,
    });

    // Pacing at target interval
    let sleep_dur = args.interval_ms.saturating_sub(elapsed);
    if sleep_dur > 0 && tick_idx < args.ticks {
      sleep(Duration::from_millis(sleep_dur));
    }
  }

  // 5. Compute Statistics & Latency Percentiles
  let mut sorted_latencies = latencies_ms.clone();
  sorted_latencies.sort_unstable();

  let p50_ms = if sorted_latencies.is_empty() {
    0
  } else {
    sorted_latencies[sorted_latencies.len() * 50 / 100]
  };
  let p95_ms = if sorted_latencies.is_empty() {
    0
  } else {
    sorted_latencies[sorted_latencies.len() * 95 / 100]
  };
  let max_ms = sorted_latencies.last().copied().unwrap_or(0);

  let failed_ticks = screenshot_failures + telemetry_missing + inference_errors;
  let successful_ticks = args.ticks.saturating_sub(failed_ticks);
  let failure_rate_percent = if args.ticks > 0 {
    (failed_ticks as f64 / args.ticks as f64) * 100.0
  } else {
    0.0
  };

  // Spot check ticks that detected grass_block
  let mut spot_check_ticks: Vec<TickAuditEntry> =
    audit_entries.iter().filter(|e| e.detections.iter().any(|d| d.contains("grass_block"))).take(3).cloned().collect();
  if spot_check_ticks.len() < 3 {
    // Fill up to 3 with any ticks
    for e in &audit_entries {
      if spot_check_ticks.len() >= 3 {
        break;
      }
      if !spot_check_ticks.iter().any(|s| s.tick == e.tick) {
        spot_check_ticks.push(e.clone());
      }
    }
  }

  // 6. Verdict Evaluation
  let mut verdict_reasons = Vec::new();
  let mut is_go = true;

  if failure_rate_percent >= 5.0 {
    is_go = false;
    verdict_reasons.push(format!("Failure rate {:.2}% exceeded 5% red line", failure_rate_percent));
  }
  if p95_ms > 1000 {
    is_go = false;
    verdict_reasons.push(format!("p95 latency {}ms exceeded 1000ms 1Hz budget", p95_ms));
  }
  if visual_landmarks_created_total == 0 && total_detections_count > 0 && agent_loop.store().len() == 0 {
    is_go = false;
    verdict_reasons.push("Zero visual landmarks created in store despite detections present".to_string());
  }

  let verdict = if is_go {
    "GO".to_string()
  } else {
    "NO-GO".to_string()
  };

  let summary = LiveVerifySummary {
    phase: format!("Phase {}", args.phase.to_uppercase()),
    target_window_title: window.title.unwrap_or_else(|| "unknown".to_string()),
    target_window_frame: format!("{:?}", window.frame),
    model_path: args.model_path.to_string_lossy().to_string(),
    depth_model_path: args.depth_model_path.to_string_lossy().to_string(),
    telemetry_path: args.telemetry_path.to_string_lossy().to_string(),
    total_ticks_requested: args.ticks,
    total_ticks_completed: audit_entries.len(),
    successful_ticks,
    failed_ticks,
    failure_rate_percent,
    failures: FailureBreakdown {
      screenshot_failures,
      telemetry_stalls,
      telemetry_missing,
      inference_errors,
      timeouts_over_1000ms,
    },
    latency_p50_ms: p50_ms,
    latency_p95_ms: p95_ms,
    latency_max_ms: max_ms,
    total_landmarks_in_store: agent_loop.store().len(),
    total_detections_count,
    detections_by_class: class_distribution,
    visual_landmarks_created_total,
    raycast_landmarks_created_total,
    spot_check_ticks,
    phase_b_clicks,
    verdict: verdict.clone(),
    verdict_reasons,
  };

  // Write summary JSON
  let json_data = serde_json::to_string_pretty(&summary)?;
  fs::write(&args.summary_out, json_data)?;
  println!("\n=== Execution Summary Saved to: {} ===", args.summary_out.display());

  println!("\n============================================================");
  println!("                     STEP 7 AUDIT REPORT                    ");
  println!("============================================================");
  println!("Verdict:        {}", summary.verdict);
  println!("Ticks Total:    {} (Success: {}, Failed: {})", summary.total_ticks_completed, summary.successful_ticks, summary.failed_ticks);
  println!("Failure Rate:   {:.2}% (< 5% threshold)", summary.failure_rate_percent);
  println!("Latency p50:    {} ms", summary.latency_p50_ms);
  println!("Latency p95:    {} ms (< 1000 ms budget)", summary.latency_p95_ms);
  println!("Latency Max:    {} ms", summary.latency_max_ms);
  println!(
    "Landmarks:      {} in store (Visual: {}, Raycast: {})",
    summary.total_landmarks_in_store, summary.visual_landmarks_created_total, summary.raycast_landmarks_created_total
  );
  println!("Detections:     {} total", summary.total_detections_count);
  for (cls, count) in &summary.detections_by_class {
    println!("  - {}: {}", cls, count);
  }
  println!("Failures Breakdown:");
  println!("  - Screenshot: {}", summary.failures.screenshot_failures);
  println!("  - Telemetry Missing: {}", summary.failures.telemetry_missing);
  println!("  - Telemetry Stalls:  {}", summary.failures.telemetry_stalls);
  println!("  - Model Inference:   {}", summary.failures.inference_errors);
  println!("  - Over 1000ms:       {}", summary.failures.timeouts_over_1000ms);
  if !summary.phase_b_clicks.is_empty() {
    println!("Phase B Click Records ({}):", summary.phase_b_clicks.len());
    for c in &summary.phase_b_clicks {
      println!("  - Tick {}: attempted={}, point={:?}, refusal={:?}", c.tick, c.attempted, c.projected_point, c.refusal_reason);
    }
  }
  println!("============================================================\n");

  Ok(())
}
