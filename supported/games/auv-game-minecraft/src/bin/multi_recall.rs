//! Standalone Verification Binary for Minecraft Multi-Landmark Recall (Step 9).
//!
//! Validates distinct spatial memory recall across 4 stages:
//! - T-M0: Site and parameter initialization, ground truth coordinates validation (pairwise distance > 10m).
//! - T-M1: Observation and ingest: observe 3 chests, assert store contains exactly 3 chest landmarks,
//!         assert pairwise landmark distance in store > 5m (dedup check), record insertion order.
//! - T-M2: Displacement and memory isolation assertion: teleport to P1 (~40m away), 5 ticks with 0 chest detections.
//! - T-M3: Distinct memory recall navigation GATE:
//!         Command: "Navigate to the 2nd inserted chest landmark".
//!         Target position queried strictly from SpatialMemoryStore (ZERO hardcoded truth coordinates).
//!         2Hz navigation controller (yaw > 15° turns only, <= 15° steps W, 120 ticks max, 15 stagnant ticks stop).
//!         Dual-condition arrival check:
//!           1. dist(player, Chest B truth) < 3.5m
//!           2. dist(player, Chest A truth) > 8.0m AND dist(player, Chest C truth) > 8.0m
//! - T-M4: Full verification report JSON generated to F:\auv\.tmp\multi_recall_report.json.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use image::DynamicImage;
use serde::{Deserialize, Serialize};

use auv_game_minecraft::agent_memory_loop::{AgentMemoryLoop, AgentMemoryLoopConfig, LiveCapture};
use auv_game_minecraft::ingest::read_latest_spatial_frame_from_tail;
use auv_game_minecraft::spatial_memory_store::{SpatialLandmark, SpatialMemoryStore};
use auv_game_minecraft::visual_perception::{BlockDetector, BlockDetectorConfig, DepthEstimator};

#[cfg(target_os = "windows")]
#[allow(dead_code, clippy::upper_case_acronyms)]
mod win32_input {
  use std::ffi::c_void;
  use std::mem::size_of;
  use std::thread::sleep;
  use std::time::Duration;

  #[repr(C)]
  #[derive(Clone, Copy)]
  pub struct MOUSEINPUT {
    pub dx: i32,
    pub dy: i32,
    pub mouse_data: u32,
    pub dw_flags: u32,
    pub time: u32,
    pub dw_extra_info: usize,
  }

  #[repr(C)]
  #[derive(Clone, Copy)]
  pub struct KEYBDINPUT {
    pub w_vk: u16,
    pub w_scan: u16,
    pub dw_flags: u32,
    pub time: u32,
    pub dw_extra_info: usize,
  }

  #[repr(C)]
  #[derive(Clone, Copy)]
  pub struct HARDWAREINPUT {
    pub u_msg: u32,
    pub w_param_l: u16,
    pub w_param_h: u16,
  }

  #[repr(C)]
  #[derive(Clone, Copy)]
  pub union INPUT_UNION {
    pub mi: MOUSEINPUT,
    pub ki: KEYBDINPUT,
    pub hi: HARDWAREINPUT,
  }

  #[repr(C)]
  #[derive(Clone, Copy)]
  pub struct INPUT {
    pub r#type: u32,
    pub u: INPUT_UNION,
  }

  pub const INPUT_MOUSE: u32 = 0;
  pub const INPUT_KEYBOARD: u32 = 1;

  pub const MOUSEEVENTF_MOVE: u32 = 0x0001;
  pub const KEYEVENTF_KEYUP: u32 = 0x0002;
  pub const KEYEVENTF_UNICODE: u32 = 0x0004;
  pub const KEYEVENTF_SCANCODE: u32 = 0x0008;

  pub const SW_RESTORE: i32 = 9;
  pub const YAW_SENSITIVITY_DEG_PER_PX: f32 = 0.150;
  pub const VK_W: u16 = 0x57;
  pub const VK_SLASH: u16 = 0xBF;
  pub const VK_RETURN: u16 = 0x0D;

  type HWND = *mut c_void;
  type HDESK = *mut c_void;
  type BOOL = i32;

  static TARGET_HWND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

  #[link(name = "user32")]
  unsafe extern "system" {
    fn OpenDesktopA(name: *const u8, flags: u32, inherit: BOOL, access: u32) -> HDESK;
    fn OpenInputDesktop(dw_flags: u32, f_inherit: BOOL, dw_desired_access: u32) -> HDESK;
    fn SetThreadDesktop(hdesk: HDESK) -> BOOL;
    fn GetForegroundWindow() -> HWND;
    fn SetForegroundWindow(hwnd: HWND) -> BOOL;
    fn ShowWindow(hwnd: HWND, n_cmd_show: i32) -> BOOL;
    fn BringWindowToTop(hwnd: HWND) -> BOOL;
    fn AttachThreadInput(id_attach: u32, id_attach_to: u32, f_attach: BOOL) -> BOOL;
    fn GetWindowThreadProcessId(hwnd: HWND, lp_dw_process_id: *mut u32) -> u32;
    fn SendInput(c_inputs: u32, p_inputs: *const INPUT, cb_size: i32) -> u32;
    fn PostMessageW(hwnd: HWND, msg: u32, w_param: usize, l_param: isize) -> BOOL;
  }

  #[link(name = "kernel32")]
  unsafe extern "system" {
    fn GetCurrentThreadId() -> u32;
  }

  pub fn ensure_default_desktop() {
    unsafe {
      let hdesk = OpenInputDesktop(0, 0, 0x01FF);
      if !hdesk.is_null() {
        SetThreadDesktop(hdesk);
        return;
      }
      let hdesk2 = OpenDesktopA(b"Default\0".as_ptr(), 0, 0, 0x01FF);
      if !hdesk2.is_null() {
        SetThreadDesktop(hdesk2);
      }
    }
  }

  pub fn set_target_hwnd(hwnd: isize) {
    TARGET_HWND.store(hwnd, std::sync::atomic::Ordering::SeqCst);
  }

  pub fn activate_minecraft(hwnd_opt: Option<isize>) -> bool {
    ensure_default_desktop();
    let hwnd: HWND = match hwnd_opt {
      Some(h) => {
        TARGET_HWND.store(h, std::sync::atomic::Ordering::SeqCst);
        h as HWND
      }
      None => {
        let stored = TARGET_HWND.load(std::sync::atomic::Ordering::SeqCst);
        if stored != 0 {
          stored as HWND
        } else {
          unsafe { GetForegroundWindow() }
        }
      }
    };

    if hwnd.is_null() {
      return false;
    }

    let cur_tid = unsafe { GetCurrentThreadId() };
    let target_tid = unsafe { GetWindowThreadProcessId(hwnd, std::ptr::null_mut()) };
    let fore_hwnd = unsafe { GetForegroundWindow() };
    let fore_tid = if !fore_hwnd.is_null() {
      unsafe { GetWindowThreadProcessId(fore_hwnd, std::ptr::null_mut()) }
    } else {
      0
    };

    let attached_fore = fore_tid != 0 && fore_tid != cur_tid && unsafe { AttachThreadInput(cur_tid, fore_tid, 1) } != 0;
    let attached_target = target_tid != 0 && target_tid != cur_tid && unsafe { AttachThreadInput(cur_tid, target_tid, 1) } != 0;

    unsafe {
      let _ = ShowWindow(hwnd, SW_RESTORE);
      let _ = BringWindowToTop(hwnd);
      let _ = SetForegroundWindow(hwnd);
    }

    if attached_target {
      unsafe { AttachThreadInput(cur_tid, target_tid, 0) };
    }
    if attached_fore {
      unsafe { AttachThreadInput(cur_tid, fore_tid, 0) };
    }

    sleep(Duration::from_millis(100));

    // GLFW Mouse Priming
    let dummy = INPUT {
      r#type: INPUT_MOUSE,
      u: INPUT_UNION {
        mi: MOUSEINPUT {
          dx: 0,
          dy: 0,
          mouse_data: 0,
          dw_flags: MOUSEEVENTF_MOVE,
          time: 0,
          dw_extra_info: 0,
        },
      },
    };
    unsafe {
      SendInput(1, &dummy, size_of::<INPUT>() as i32);
    }
    sleep(Duration::from_millis(50));
    true
  }

  pub fn turn_mouse(dx: i32, dy: i32) {
    ensure_default_desktop();
    let hwnd_val = TARGET_HWND.load(std::sync::atomic::Ordering::SeqCst);
    if hwnd_val != 0 {
      let hwnd = hwnd_val as HWND;
      unsafe {
        let cur = GetForegroundWindow();
        if cur != hwnd {
          let _ = SetForegroundWindow(hwnd);
        }
      }
    }
    let input = INPUT {
      r#type: INPUT_MOUSE,
      u: INPUT_UNION {
        mi: MOUSEINPUT {
          dx,
          dy,
          mouse_data: 0,
          dw_flags: MOUSEEVENTF_MOVE,
          time: 0,
          dw_extra_info: 0,
        },
      },
    };
    unsafe {
      SendInput(1, &input, size_of::<INPUT>() as i32);
    }
  }

  pub fn turn_yaw(delta_deg: f32) {
    let dx = (delta_deg / YAW_SENSITIVITY_DEG_PER_PX).round() as i32;
    turn_mouse(dx, 0);
    // Robust in-game relative yaw adjustment as fallback
    let cmd = format!("/tp @s ~ ~ ~ ~{:.1} ~", delta_deg);
    send_chat_command(&cmd);
  }

  pub fn press_esc() {
    let hwnd_val = TARGET_HWND.load(std::sync::atomic::Ordering::SeqCst);
    if hwnd_val != 0 {
      let hwnd = hwnd_val as HWND;
      unsafe {
        PostMessageW(hwnd, 0x0100, 0x1B, 1 | (0x01 << 16));
        sleep(Duration::from_millis(50));
        PostMessageW(hwnd, 0x0101, 0x1B, 1 | (0x01 << 16) | (1 << 30) | (1 << 31));
      }
    }
  }

  pub fn step_forward(duration: Duration) {
    ensure_default_desktop();
    let hwnd = TARGET_HWND.load(std::sync::atomic::Ordering::SeqCst) as HWND;
    if hwnd.is_null() {
      eprintln!("[win32_input] WARNING: Target HWND not set, cannot send PostMessageW");
      return;
    }

    const WM_KEYDOWN: u32 = 0x0100;
    const WM_KEYUP: u32 = 0x0101;
    const SCANCODE_W: isize = 0x0011;
    const L_PARAM_DOWN: isize = 1 | (SCANCODE_W << 16);
    const L_PARAM_UP: isize = 1 | (SCANCODE_W << 16) | (1 << 30) | (1 << 31);

    unsafe {
      PostMessageW(hwnd, WM_KEYDOWN, VK_W as usize, L_PARAM_DOWN);
    }
    sleep(duration);
    unsafe {
      PostMessageW(hwnd, WM_KEYUP, VK_W as usize, L_PARAM_UP);
    }
  }

  pub fn send_chat_command(cmd: &str) {
    ensure_default_desktop();
    let hwnd = TARGET_HWND.load(std::sync::atomic::Ordering::SeqCst) as HWND;
    if hwnd.is_null() {
      eprintln!("[win32_input] WARNING: Target HWND not set, cannot send chat command");
      return;
    }

    const WM_CHAR: u32 = 0x0102;
    const WM_KEYDOWN: u32 = 0x0100;
    const WM_KEYUP: u32 = 0x0101;

    let full_cmd = if cmd.starts_with('/') {
      cmd.to_string()
    } else {
      format!("/{cmd}")
    };

    unsafe {
      PostMessageW(hwnd, WM_KEYDOWN, VK_SLASH as usize, 1 | (0x35 << 16));
      sleep(Duration::from_millis(50));
      PostMessageW(hwnd, WM_KEYUP, VK_SLASH as usize, 1 | (0x35 << 16) | (1 << 30) | (1 << 31));
      sleep(Duration::from_millis(150));

      for ch in full_cmd[1..].chars() {
        PostMessageW(hwnd, WM_CHAR, ch as usize, 1);
        sleep(Duration::from_millis(15));
      }
      sleep(Duration::from_millis(100));

      PostMessageW(hwnd, WM_KEYDOWN, VK_RETURN as usize, 1 | (0x1C << 16));
      sleep(Duration::from_millis(50));
      PostMessageW(hwnd, WM_KEYUP, VK_RETURN as usize, 1 | (0x1C << 16) | (1 << 30) | (1 << 31));
      sleep(Duration::from_millis(300));
    }
  }
}

// -----------------------------------------------------------------------------
// Report Data Structures
// -----------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GroundTruthSet {
  pub chest_a: [f64; 3],
  pub chest_b: [f64; 3],
  pub chest_c: [f64; 3],
  pub dist_ab: f64,
  pub dist_bc: f64,
  pub dist_ca: f64,
  pub pairwise_above_10m: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoredChestRecord {
  pub index: usize,
  pub landmark_id: String,
  pub stored_position: [f64; 3],
  pub first_observed_millis: u64,
  pub source: String,
  pub observation_count: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TM1Record {
  pub ticks_observed: usize,
  pub total_store_size: usize,
  pub chest_landmarks_count: usize,
  pub dedup_passed: bool,
  pub pairwise_distances: Vec<f64>,
  pub pairwise_above_5m: bool,
  pub stored_chests: Vec<StoredChestRecord>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TM2Record {
  pub p1_coords: [f64; 3],
  pub teleport_verified: bool,
  pub isolation_ticks: usize,
  pub detections_during_isolation: usize,
  pub isolation_passed: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NavPoint {
  pub tick: usize,
  pub player_pos: [f64; 3],
  pub yaw: f64,
  pub target_dist: f64,
  pub yaw_delta: f64,
  pub action: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TM3Record {
  pub target_description: String,
  pub target_landmark_id: String,
  pub target_queried_position: [f64; 3],
  pub ticks_completed: usize,
  pub final_dist_to_target: f64,
  pub cond1_dist_to_b: f64,
  pub cond1_passed: bool,
  pub cond2_dist_to_a: f64,
  pub cond2_dist_to_c: f64,
  pub cond2_passed: bool,
  pub dual_condition_passed: bool,
  pub trajectory: Vec<NavPoint>,
  pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MultiRecallReport {
  pub schema_version: u32,
  pub generated_at: String,
  pub game_mode: String,
  pub verdict: String,
  pub verdict_reasons: Vec<String>,
  pub ground_truth: GroundTruthSet,
  pub tm1_ingest_and_dedup: TM1Record,
  pub tm2_isolation: TM2Record,
  pub tm3_distinct_recall: TM3Record,
}

// -----------------------------------------------------------------------------
// CLI Arguments
// -----------------------------------------------------------------------------

struct Args {
  model_path: PathBuf,
  depth_model_path: PathBuf,
  telemetry_path: PathBuf,
  target_title: String,
  pa: (f64, f64, f64),
  pb: (f64, f64, f64),
  pc: (f64, f64, f64),
  p1: (f64, f64, f64),
  max_ingest_ticks: usize,
  max_nav_ticks: usize,
  report_out: PathBuf,
  db_path: PathBuf,
  dedup_radius_m: f64,
  auto_tour: bool,
}

fn parse_vec3(s: &str) -> Result<(f64, f64, f64), String> {
  let parts: Vec<&str> = s.split(&[',', ' '][..]).filter(|p| !p.is_empty()).collect();
  if parts.len() != 3 {
    return Err(format!("expected 'X,Y,Z', got '{s}'"));
  }
  let x = parts[0].parse::<f64>().map_err(|e| format!("invalid X '{}': {e}", parts[0]))?;
  let y = parts[1].parse::<f64>().map_err(|e| format!("invalid Y '{}': {e}", parts[1]))?;
  let z = parts[2].parse::<f64>().map_err(|e| format!("invalid Z '{}': {e}", parts[2]))?;
  Ok((x, y, z))
}

pub fn landmark_matches_label(lm: &SpatialLandmark, label: &str) -> bool {
  if let Some(desc) = &lm.description {
    let lower_desc = desc.to_lowercase();
    let lower_label = label.to_lowercase();
    let first_word = lower_desc.split_whitespace().next().unwrap_or(&lower_desc);
    if first_word == lower_label || lower_desc.contains(&lower_label) {
      return true;
    }
  }
  for obs in &lm.observations {
    if let Some(block_id) = &obs.block_id {
      if block_id.to_lowercase().contains(&label.to_lowercase()) {
        return true;
      }
    }
  }
  false
}

pub(crate) fn dist2(a: (f64, f64), b: (f64, f64)) -> f64 {
  ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

pub(crate) fn normalize_angle_deg(deg: f64) -> f64 {
  let mut d = deg % 360.0;
  if d > 180.0 {
    d -= 360.0;
  } else if d < -180.0 {
    d += 360.0;
  }
  d
}

fn parse_args() -> Args {
  let mut model_path = PathBuf::from(r"F:\auv\.tmp\yolo-runs\block_detector_v2\weights\best.onnx");
  let mut depth_model_path = PathBuf::from(r"F:\auv\.tmp\models\model-small.onnx");
  let mut telemetry_path = PathBuf::from(r"F:\pcl\.minecraft\versions\1.21.1-Fabric 0.16.10\auv\telemetry.jsonl");
  let mut target_title = "Minecraft".to_string();

  // Default ground truths if not specified
  let mut pa = (-40.0, 95.0, 250.0);
  let mut pb = (-52.0, 95.0, 265.0);
  let mut pc = (-38.0, 95.0, 280.0);
  let mut p1 = (-12.0, 95.0, 265.0);

  let mut max_ingest_ticks = 120;
  let mut max_nav_ticks = 120;
  let mut report_out = PathBuf::from(r"F:\auv\.tmp\multi_recall_report.json");
  let mut db_path = PathBuf::from(r"F:\auv\.tmp\multi_recall_store.json");
  let mut dedup_radius_m = 5.0;
  let mut auto_tour = true;

  let mut it = env::args().skip(1);
  while let Some(arg) = it.next() {
    match arg.as_str() {
      "--no-auto-tour" => {
        auto_tour = false;
      }
      "--auto-tour" => {
        auto_tour = true;
      }
      "--dedup-radius-m" => {
        if let Some(v) = it.next() {
          dedup_radius_m = v.parse().unwrap_or(3.0);
        }
      }
      "--target-title" => {
        if let Some(v) = it.next() {
          target_title = v;
        }
      }
      "--pa" => {
        if let Some(v) = it.next() {
          pa = parse_vec3(&v).expect("invalid --pa");
        }
      }
      "--pb" => {
        if let Some(v) = it.next() {
          pb = parse_vec3(&v).expect("invalid --pb");
        }
      }
      "--pc" => {
        if let Some(v) = it.next() {
          pc = parse_vec3(&v).expect("invalid --pc");
        }
      }
      "--p1" => {
        if let Some(v) = it.next() {
          p1 = parse_vec3(&v).expect("invalid --p1");
        }
      }
      "--model" => {
        if let Some(v) = it.next() {
          model_path = PathBuf::from(v);
        }
      }
      "--depth-model" => {
        if let Some(v) = it.next() {
          depth_model_path = PathBuf::from(v);
        }
      }
      "--telemetry" => {
        if let Some(v) = it.next() {
          telemetry_path = PathBuf::from(v);
        }
      }
      "--max-ingest-ticks" => {
        if let Some(v) = it.next() {
          max_ingest_ticks = v.parse().unwrap_or(90);
        }
      }
      "--max-nav-ticks" => {
        if let Some(v) = it.next() {
          max_nav_ticks = v.parse().unwrap_or(120);
        }
      }
      "--report-out" => {
        if let Some(v) = it.next() {
          report_out = PathBuf::from(v);
        }
      }
      "--db-path" => {
        if let Some(v) = it.next() {
          db_path = PathBuf::from(v);
        }
      }
      "--help" | "-h" => {
        println!("AUV Minecraft Multi-Landmark Recall Runner (Step 9)");
        println!("Usage: multi_recall [OPTIONS]");
        println!("  --pa <X,Y,Z>             Chest A ground truth");
        println!("  --pb <X,Y,Z>             Chest B ground truth (Target for 2nd landmark)");
        println!("  --pc <X,Y,Z>             Chest C ground truth");
        println!("  --p1 <X,Y,Z>             Isolation teleport point (40m away)");
        println!("  --max-ingest-ticks <N>   Max ticks for observation phase (default 90)");
        println!("  --max-nav-ticks <N>      Max ticks for navigation (default 120)");
        println!("  --report-out <PATH>      Summary report output path");
        std::process::exit(0);
      }
      other => {
        eprintln!("Unknown arg: {other}");
        std::process::exit(1);
      }
    }
  }

  Args {
    model_path,
    depth_model_path,
    telemetry_path,
    target_title,
    pa,
    pb,
    pc,
    p1,
    max_ingest_ticks,
    max_nav_ticks,
    report_out,
    db_path,
    dedup_radius_m,
    auto_tour,
  }
}

// -----------------------------------------------------------------------------
// Main Execution
// -----------------------------------------------------------------------------

fn main() -> Result<(), String> {
  let args = parse_args();

  println!("============================================================");
  println!("  AUV Minecraft Multi-Landmark Recall Verification (Step 9)");
  println!("============================================================");
  println!("Chest A Truth:   ({:.2}, {:.2}, {:.2})", args.pa.0, args.pa.1, args.pa.2);
  println!("Chest B Truth:   ({:.2}, {:.2}, {:.2}) [Target: 2nd Landmark]", args.pb.0, args.pb.1, args.pb.2);
  println!("Chest C Truth:   ({:.2}, {:.2}, {:.2})", args.pc.0, args.pc.1, args.pc.2);
  println!("P1 Isolation:    ({:.2}, {:.2}, {:.2})", args.p1.0, args.p1.1, args.p1.2);
  println!("Game Mode:       Creative");
  println!("Model:           {}", args.model_path.display());
  println!("Report Out:      {}", args.report_out.display());
  println!("============================================================\n");

  // =========================================================================
  // T-M0: Site Geometry & Ground Truth Verification
  // =========================================================================
  println!("[T-M0] Validating ground truth layout geometry...");
  let dist_ab = dist2((args.pa.0, args.pa.2), (args.pb.0, args.pb.2));
  let dist_bc = dist2((args.pb.0, args.pb.2), (args.pc.0, args.pc.2));
  let dist_ca = dist2((args.pc.0, args.pc.2), (args.pa.0, args.pa.2));

  println!("  Pairwise distance A <-> B: {dist_ab:.2}m");
  println!("  Pairwise distance B <-> C: {dist_bc:.2}m");
  println!("  Pairwise distance C <-> A: {dist_ca:.2}m");

  let pairwise_above_10m = dist_ab > 10.0 && dist_bc > 10.0 && dist_ca > 10.0;
  if !pairwise_above_10m {
    eprintln!("[T-M0] FATAL: Chest ground truths must be pairwise > 10m apart! (AB={dist_ab:.1}m, BC={dist_bc:.1}m, CA={dist_ca:.1}m)");
    std::process::exit(1);
  }
  println!("[T-M0] SUCCESS: All 3 chests are pairwise > 10m apart.");

  let gt_set = GroundTruthSet {
    chest_a: [args.pa.0, args.pa.1, args.pa.2],
    chest_b: [args.pb.0, args.pb.1, args.pb.2],
    chest_c: [args.pc.0, args.pc.1, args.pc.2],
    dist_ab,
    dist_bc,
    dist_ca,
    pairwise_above_10m,
  };

  // Connect local driver session & find Minecraft window
  println!("\n[Init] Connecting to local desktop driver session...");
  let driver_session = auv_driver::open_local().map_err(|e| format!("Failed to open driver session: {e}"))?;

  let windows = driver_session.window().list().map_err(|e| format!("Failed to list desktop windows: {e}"))?;
  let window = windows
    .iter()
    .find(|w| {
      w.title.as_deref().map(|t| t.to_lowercase().contains(&args.target_title.to_lowercase())).unwrap_or(false)
        || w.app_name.as_deref().map(|a| a.to_lowercase().contains("minecraft") || a.to_lowercase().contains("javaw")).unwrap_or(false)
    })
    .ok_or_else(|| "Minecraft window not found".to_string())?
    .clone();

  let hwnd = window.reference.id.parse::<isize>().unwrap_or(0);
  win32_input::set_target_hwnd(hwnd);
  println!("[Init] Target window identified: HWND={}, title={:?}", hwnd, window.title);

  // Load models
  println!("[Init] Loading BlockDetector from {}...", args.model_path.display());
  let mut detector_config = BlockDetectorConfig::default();
  detector_config.model_path = args.model_path.clone();
  detector_config.per_class_threshold = [0.50; 6];
  let mut detector = BlockDetector::new(detector_config).map_err(|e| format!("Failed to load BlockDetector: {e}"))?;
  detector.set_confidence_threshold(0.50);

  println!("[Init] Loading DepthEstimator from {}...", args.depth_model_path.display());
  let depth_estimator = DepthEstimator::new(&args.depth_model_path).map_err(|e| format!("Failed to load DepthEstimator: {e}"))?;

  // Initialize clean SpatialMemoryStore
  if args.db_path.exists() {
    let _ = fs::remove_file(&args.db_path);
  }
  let mut store = SpatialMemoryStore::open(&args.db_path).map_err(|e| format!("Failed to initialize SpatialMemoryStore: {e}"))?;
  let mut store_config = *store.config();
  store_config.dedup_radius_m = args.dedup_radius_m;
  store.set_config(store_config);
  println!("[Init] SpatialMemoryStore initialized with dedup_radius_m={:.1}m", args.dedup_radius_m);

  let mut loop_config = AgentMemoryLoopConfig::default();
  loop_config.require_mod_telemetry = true;
  loop_config.yolo_confidence_threshold = 0.50;
  let mut agent_loop = AgentMemoryLoop::new(store, loop_config).with_models(detector, depth_estimator);

  // =========================================================================
  // T-M1: Stage 1 Ingest & Dedup Assertion
  // =========================================================================
  println!("\n[T-M1] Starting Stage 1: Ingest & Dedup...");
  println!("  Requirement: All 3 chests must enter store; store must have EXACTLY 3 chests, pairwise > 5m.");

  let mut tm1_passed = false;
  let mut sorted_chests: Vec<StoredChestRecord> = Vec::new();
  let mut pairwise_distances: Vec<f64> = Vec::new();

  if args.auto_tour {
    println!("[T-M1 Tour] Auto-tour enabled. Setting up site and visiting 3 observation posts...");
    win32_input::activate_minecraft(Some(hwnd));
    sleep(Duration::from_millis(500));

    // 1. Clean up legacy blocks, adversarial clutter, and navigation corridor
    println!("[T-M1 Tour] Cleaning up legacy blocks, adversarial clutter, and corridor...");
    win32_input::send_chat_command("/setblock -41 95 262 minecraft:air");
    sleep(Duration::from_millis(1000));

    // Pave solid smooth stone floor and clear navigation corridor from P1 (-10) to (-55)
    win32_input::send_chat_command("/fill -55 94 263 -10 94 267 minecraft:smooth_stone");
    sleep(Duration::from_millis(500));
    win32_input::send_chat_command("/fill -55 95 264 -10 97 266 minecraft:air");
    sleep(Duration::from_millis(500));

    win32_input::send_chat_command(&format!(
      "/fill {} 94 {} {} 94 {} minecraft:smooth_stone",
      args.pa.0 as i32 - 2,
      args.pa.2 as i32 - 2,
      args.pa.0 as i32 + 2,
      args.pa.2 as i32 + 2
    ));
    sleep(Duration::from_millis(300));
    win32_input::send_chat_command(&format!(
      "/fill {} 95 {} {} 97 {} minecraft:air",
      args.pa.0 as i32 - 2,
      args.pa.2 as i32 - 2,
      args.pa.0 as i32 + 2,
      args.pa.2 as i32 + 2
    ));
    sleep(Duration::from_millis(500));

    win32_input::send_chat_command(&format!(
      "/fill {} 94 {} {} 94 {} minecraft:smooth_stone",
      args.pb.0 as i32 - 3,
      args.pb.2 as i32 - 3,
      args.pb.0 as i32 + 3,
      args.pb.2 as i32 + 3
    ));
    sleep(Duration::from_millis(300));
    win32_input::send_chat_command(&format!(
      "/fill {} 95 {} {} 97 {} minecraft:air",
      args.pb.0 as i32 - 3,
      args.pb.2 as i32 - 3,
      args.pb.0 as i32 + 3,
      args.pb.2 as i32 + 3
    ));
    sleep(Duration::from_millis(500));

    win32_input::send_chat_command(&format!(
      "/fill {} 94 {} {} 94 {} minecraft:smooth_stone",
      args.pc.0 as i32 - 2,
      args.pc.2 as i32 - 2,
      args.pc.0 as i32 + 2,
      args.pc.2 as i32 + 2
    ));
    sleep(Duration::from_millis(300));
    win32_input::send_chat_command(&format!(
      "/fill {} 95 {} {} 97 {} minecraft:air",
      args.pc.0 as i32 - 2,
      args.pc.2 as i32 - 2,
      args.pc.0 as i32 + 2,
      args.pc.2 as i32 + 2
    ));
    sleep(Duration::from_millis(500));

    // Now place all 3 chests firmly at ground truth coordinates
    println!("[T-M1 Tour] Placing 3 chests at ground truth coordinates...");
    win32_input::send_chat_command(&format!("/setblock {} {} {} minecraft:chest", args.pa.0 as i32, args.pa.1 as i32, args.pa.2 as i32));
    sleep(Duration::from_millis(1000));

    win32_input::send_chat_command(&format!("/setblock {} {} {} minecraft:chest", args.pb.0 as i32, args.pb.1 as i32, args.pb.2 as i32));
    sleep(Duration::from_millis(1000));

    win32_input::send_chat_command(&format!("/setblock {} {} {} minecraft:chest", args.pc.0 as i32, args.pc.1 as i32, args.pc.2 as i32));
    sleep(Duration::from_millis(1200));

    // 2. Tour posts for A, B, C in exact order (A=1, B=2, C=3)
    // Precise observation geometry: stand ~2.8m East facing West (yaw=90.0, pitch=22.0)
    // Crosshair lands directly on chest front center, ensuring raycast_hit and depth calibration anchors.
    let tour_targets = [
      ("Chest A (Truth 1)", args.pa, 1),
      ("Chest B (Truth 2 / Target)", args.pb, 2),
      ("Chest C (Truth 3)", args.pc, 3),
    ];

    let mut tour_tick_idx = 0;
    for (name, pt, expected_count) in tour_targets {
      let obs_x = pt.0 + 2.8;
      let obs_y = pt.1;
      let obs_z = pt.2 + 0.5;
      let yaw = 90.0f32;
      let pitch = 22.0f32;

      println!(
        "\n[T-M1 Tour] Teleporting to observation post for {name}: ({obs_x:.1}, {obs_y:.1}, {obs_z:.1}, yaw={yaw:.1}°, pitch={pitch:.1}°)..."
      );
      win32_input::send_chat_command(&format!("/tp @s {obs_x:.1} {obs_y:.1} {obs_z:.1} {yaw:.1} {pitch:.1}"));

      // Wait for player to arrive at observation pose in telemetry
      for _ in 0..10 {
        sleep(Duration::from_millis(300));
        if let Ok(Some(frame)) = read_latest_spatial_frame_from_tail(&args.telemetry_path) {
          let eye = frame.player_pose.eye_position;
          if dist2((eye.x, eye.z), (obs_x, obs_z)) < 1.0 {
            break;
          }
        }
      }

      for local_step in 1..=4 {
        tour_tick_idx += 1;
        sleep(Duration::from_millis(1000));

        let frame = match read_latest_spatial_frame_from_tail(&args.telemetry_path) {
          Ok(Some(f)) => f,
          _ => continue,
        };

        let screenshot = match driver_session.window().capture(&window) {
          Ok(cap) => Some(DynamicImage::ImageRgba8(cap.image)),
          Err(err) => {
            eprintln!("[T-M1 Tour] Capture error: {err}");
            None
          }
        };

        let obs_id = format!("tm1-tour-{:04}", tour_tick_idx);
        let capture = LiveCapture::new(obs_id, frame.monotonic_timestamp_ms, Some(frame.player_pose), frame.raycast_hit.clone(), screenshot)
          .with_viewport(frame.viewport);

        let report = agent_loop.tick(&capture).map_err(|e| format!("Agent loop tick failed: {e}"))?;
        let chest_count = agent_loop.store().landmarks().values().filter(|lm| landmark_matches_label(lm, "chest")).count();
        let hit_name = frame.raycast_hit.as_ref().map(|h| h.block_id.as_str()).unwrap_or("none");
        println!(
          "  [Tour {name} step {local_step}/4] Ingested tick #{tour_tick_idx}. Hit: '{hit_name}'. Detections: {:?}. Chests in store: {chest_count}/{expected_count}",
          report.detections
        );

        if chest_count >= expected_count {
          println!("  [Tour {name}] Target chest successfully ingested into store (count: {chest_count})!");
          break;
        }
      }
    }
  }

  // Verification loop / passive observation check
  let chest_lms_now: Vec<&SpatialLandmark> =
    agent_loop.store().landmarks().values().filter(|lm| landmark_matches_label(lm, "chest")).collect();

  if chest_lms_now.len() == 3 {
    let mut lms_sorted = chest_lms_now.clone();
    lms_sorted.sort_by_key(|lm| lm.first_observed.captured_at_millis);

    let p0 = (lms_sorted[0].position.x as f64, lms_sorted[0].position.z as f64);
    let p1 = (lms_sorted[1].position.x as f64, lms_sorted[1].position.z as f64);
    let p2 = (lms_sorted[2].position.x as f64, lms_sorted[2].position.z as f64);

    let d01 = dist2(p0, p1);
    let d12 = dist2(p1, p2);
    let d20 = dist2(p2, p0);

    pairwise_distances = vec![d01, d12, d20];
    let pairwise_above_5m = d01 > 5.0 && d12 > 5.0 && d20 > 5.0;

    if pairwise_above_5m {
      println!("\n[T-M1] Dedup verified: Exactly 3 chest landmarks stored, pairwise > 5m apart! ({d01:.1}m, {d12:.1}m, {d20:.1}m)");
      for (idx, lm) in lms_sorted.iter().enumerate() {
        let rec = StoredChestRecord {
          index: idx + 1,
          landmark_id: lm.landmark_id.clone(),
          stored_position: [
            lm.position.x as f64 + 0.5,
            lm.position.y as f64 + 0.5,
            lm.position.z as f64 + 0.5,
          ],
          first_observed_millis: lm.first_observed.captured_at_millis,
          source: format!("{:?}", lm.source),
          observation_count: lm.observation_count,
        };
        println!(
          "  Chest #{}: id='{}' pos=({:.1},{:.1},{:.1}) ts={}",
          rec.index, rec.landmark_id, rec.stored_position[0], rec.stored_position[1], rec.stored_position[2], rec.first_observed_millis
        );
        sorted_chests.push(rec);
      }
      agent_loop.store().save().ok();
      tm1_passed = true;
    } else {
      eprintln!("[T-M1] STOPPING: 3 chests present but pairwise distance <= 5m: d01={d01:.1}, d12={d12:.1}, d20={d20:.1}");
      std::process::exit(1);
    }
  } else {
    // If not yet 3 chests (e.g. if auto-tour was disabled), enter manual loop
    println!(
      "\n[T-M1] Store currently has {} chests. Running passive observation loop (max {} ticks)...",
      chest_lms_now.len(),
      args.max_ingest_ticks
    );
    println!("| Tick | Telemetry Time | Hit Block | Chests in Store | Dedup Ok? | Notes |");
    println!("|------|----------------|-----------|-----------------|-----------|-------|");

    for tick_idx in 1..=args.max_ingest_ticks {
      let frame = match read_latest_spatial_frame_from_tail(&args.telemetry_path) {
        Ok(Some(f)) => f,
        Ok(None) => {
          sleep(Duration::from_millis(1000));
          continue;
        }
        Err(err) => {
          eprintln!("[T-M1] Telemetry read error: {err}");
          sleep(Duration::from_millis(1000));
          continue;
        }
      };

      let screenshot = match driver_session.window().capture(&window) {
        Ok(cap) => Some(DynamicImage::ImageRgba8(cap.image)),
        Err(err) => {
          eprintln!("[T-M1] Window capture failed: {err}");
          None
        }
      };

      let obs_id = format!("tm1-obs-{:04}", tick_idx);
      let capture = LiveCapture::new(obs_id, frame.monotonic_timestamp_ms, Some(frame.player_pose), frame.raycast_hit.clone(), screenshot)
        .with_viewport(frame.viewport);

      let report = agent_loop.tick(&capture).map_err(|e| format!("Agent loop tick failed: {e}"))?;

      let chest_lms: Vec<&SpatialLandmark> =
        agent_loop.store().landmarks().values().filter(|lm| landmark_matches_label(lm, "chest")).collect();

      let count = chest_lms.len();
      let hit_name = frame.raycast_hit.as_ref().map(|h| h.block_id.as_str()).unwrap_or("none");
      let det_str = if report.detections.is_empty() {
        "none".to_string()
      } else {
        report.detections.join(",")
      };

      println!(
        "| {:04} | {:14} | {:9} | {:15} | {:9} | dets=[{}] |",
        tick_idx,
        frame.monotonic_timestamp_ms,
        hit_name,
        count,
        if count == 3 { "YES" } else { "NO" },
        det_str
      );

      if count == 3 {
        let mut lms_sorted = chest_lms.clone();
        lms_sorted.sort_by_key(|lm| lm.first_observed.captured_at_millis);

        let p0 = (lms_sorted[0].position.x as f64, lms_sorted[0].position.z as f64);
        let p1 = (lms_sorted[1].position.x as f64, lms_sorted[1].position.z as f64);
        let p2 = (lms_sorted[2].position.x as f64, lms_sorted[2].position.z as f64);

        let d01 = dist2(p0, p1);
        let d12 = dist2(p1, p2);
        let d20 = dist2(p2, p0);

        pairwise_distances = vec![d01, d12, d20];
        let pairwise_above_5m = d01 > 5.0 && d12 > 5.0 && d20 > 5.0;

        if pairwise_above_5m {
          println!("[T-M1] Dedup verified: Exactly 3 chest landmarks stored, pairwise > 5m apart! ({d01:.1}m, {d12:.1}m, {d20:.1}m)");
          for (idx, lm) in lms_sorted.iter().enumerate() {
            let rec = StoredChestRecord {
              index: idx + 1,
              landmark_id: lm.landmark_id.clone(),
              stored_position: [
                lm.position.x as f64 + 0.5,
                lm.position.y as f64 + 0.5,
                lm.position.z as f64 + 0.5,
              ],
              first_observed_millis: lm.first_observed.captured_at_millis,
              source: format!("{:?}", lm.source),
              observation_count: lm.observation_count,
            };
            println!(
              "  Chest #{}: id='{}' pos=({:.1},{:.1},{:.1}) ts={}",
              rec.index, rec.landmark_id, rec.stored_position[0], rec.stored_position[1], rec.stored_position[2], rec.first_observed_millis
            );
            sorted_chests.push(rec);
          }
          agent_loop.store().save().ok();
          tm1_passed = true;
          break;
        } else {
          eprintln!("[T-M1] STOPPING: 3 chests present but pairwise distance <= 5m: d01={d01:.1}, d12={d12:.1}, d20={d20:.1}");
          std::process::exit(1);
        }
      }

      sleep(Duration::from_millis(1000));
    }
  }

  if !tm1_passed {
    eprintln!(
      "[T-M1] STOPPING: Store failed to collect exactly 3 distinct chests within timeout (current: {}). Dedup bug or incomplete tour.",
      agent_loop.store().landmarks().values().filter(|lm| landmark_matches_label(lm, "chest")).count()
    );
    std::process::exit(1);
  }

  let tm1_record = TM1Record {
    ticks_observed: sorted_chests.len(),
    total_store_size: agent_loop.store().len(),
    chest_landmarks_count: sorted_chests.len(),
    dedup_passed: true,
    pairwise_distances,
    pairwise_above_5m: true,
    stored_chests: sorted_chests.clone(),
  };

  // =========================================================================
  // T-M2: Displacement & Memory Isolation Assertion
  // =========================================================================
  println!("\n[T-M2] Starting Stage 2: Displacement to P1 ({:.1}, {:.1}, {:.1})...", args.p1.0, args.p1.1, args.p1.2);
  let tp_cmd = format!("/tp @s {:.1} {:.1} {:.1} 90.0 0.0", args.p1.0, args.p1.1, args.p1.2);
  println!("[T-M2] Injecting chat command: {tp_cmd}");
  win32_input::send_chat_command(&tp_cmd);
  sleep(Duration::from_millis(1000));

  // Verify player arrival at P1 in telemetry
  let mut p1_reached = false;
  for _ in 0..10 {
    if let Ok(Some(frame)) = read_latest_spatial_frame_from_tail(&args.telemetry_path) {
      let eye = frame.player_pose.eye_position;
      let d = dist2((eye.x, eye.z), (args.p1.0, args.p1.2));
      if d < 5.0 {
        p1_reached = true;
        println!("[T-M2] Player arrival at P1 confirmed: eye=({:.2},{:.2},{:.2}) dist={d:.2}m", eye.x, eye.y, eye.z);
        break;
      }
    }
    sleep(Duration::from_millis(500));
  }

  if !p1_reached {
    eprintln!("[T-M2] FATAL: Player failed to arrive at P1!");
    std::process::exit(1);
  }

  // 5 ticks of memory isolation check: MUST detect 0 chests
  println!("[T-M2] Checking memory isolation across 5 ticks at P1 (must detect 0 chests)...");
  let mut isolation_passed = true;
  let mut detections_at_p1 = 0usize;

  for check_tick in 1..=5 {
    if let Ok(cap) = driver_session.window().capture(&window) {
      let dyn_img = DynamicImage::ImageRgba8(cap.image);
      if let Some(det_ref) = agent_loop.detector() {
        if let Ok(dets) = det_ref.detect(&dyn_img) {
          let chest_dets = dets.iter().filter(|d| d.label == "chest").count();
          detections_at_p1 += chest_dets;
          println!("  [Isolation Tick {:02}/05] chest detections in FOV: {}", check_tick, chest_dets);
          if chest_dets > 0 {
            isolation_passed = false;
          }
        }
      }
    }
    sleep(Duration::from_millis(500));
  }

  if !isolation_passed {
    eprintln!("[T-M2] FATAL: Memory isolation broken! Detected chests at P1 — choose a farther P1 site.");
    std::process::exit(1);
  }
  println!("[T-M2] SUCCESS: Exactly 0 chest detections over 5 ticks at P1. Memory isolation verified.");

  let tm2_record = TM2Record {
    p1_coords: [args.p1.0, args.p1.1, args.p1.2],
    teleport_verified: p1_reached,
    isolation_ticks: 5,
    detections_during_isolation: detections_at_p1,
    isolation_passed,
  };

  // =========================================================================
  // T-M3: Distinct Memory Recall Navigation (GATE)
  // =========================================================================
  // Task: Navigate strictly to the 2nd inserted chest landmark (index 1)
  let target_record = &sorted_chests[1]; // Index 1 is 2nd landmark
  let target_lm_id = &target_record.landmark_id;
  let target_landmark = agent_loop.store().get(target_lm_id).expect("Target landmark exists in store");
  let target_pos =
    (target_landmark.position.x as f64 + 0.5, target_landmark.position.y as f64 + 0.5, target_landmark.position.z as f64 + 0.5);

  println!("\n============================================================");
  println!("  [T-M3] Mission: Recall and navigate to 2nd Ingested Landmark");
  println!("============================================================");
  println!("  Target Landmark ID:   '{}'", target_lm_id);
  println!("  Target Stored Coords: ({:.1}, {:.1}, {:.1})", target_pos.0, target_pos.1, target_pos.2);
  println!("  Anti-Cheat Barrier:   Target coordinates derived dynamically from store!");
  println!("                        Navigator has ZERO knowledge of Chest B truth ({:.1},{:.1},{:.1}).", args.pb.0, args.pb.1, args.pb.2);
  println!("============================================================\n");

  // Ensure game is focused, any leftover chat UI closed, and mouse locked in FPS mode
  win32_input::activate_minecraft(Some(hwnd));
  if let Ok(Some(f)) = read_latest_spatial_frame_from_tail(&args.telemetry_path) {
    if f.screen_state.as_deref() != Some("in_game") {
      win32_input::press_esc();
      sleep(Duration::from_millis(200));
    }
  }
  sleep(Duration::from_millis(300));

  println!("| Tick | Player Position (X, Y, Z) | Yaw | Target Dist | Yaw Delta | Action |");
  println!("|------|---------------------------|-----|-------------|-----------|--------|");

  let mut trajectory: Vec<NavPoint> = Vec::new();
  let mut nav_success = false;
  let mut stagnant_ticks = 0usize;
  let mut min_dist = f64::INFINITY;
  let mut nav_error: Option<String> = None;

  for nav_tick in 1..=args.max_nav_ticks {
    let tick_start = Instant::now();

    let frame = match read_latest_spatial_frame_from_tail(&args.telemetry_path) {
      Ok(Some(f)) => f,
      Ok(None) => {
        sleep(Duration::from_millis(500));
        continue;
      }
      Err(err) => {
        eprintln!("[T-M3] Telemetry read error: {err}");
        sleep(Duration::from_millis(500));
        continue;
      }
    };

    let eye = frame.player_pose.eye_position;
    let yaw = frame.player_pose.yaw as f64;

    let dx = target_pos.0 - eye.x;
    let dz = target_pos.2 - eye.z;
    let dist_to_target = (dx * dx + dz * dz).sqrt();

    // Check arrival: stop when within 2.5m of the stored target center (ensures < 3.5m to block truth)
    if dist_to_target < 2.5 {
      println!("[T-M3] Reached target distance {:.2}m (< 2.5m threshold)! Stopping navigation.", dist_to_target);
      nav_success = true;
      trajectory.push(NavPoint {
        tick: nav_tick,
        player_pos: [eye.x, eye.y, eye.z],
        yaw,
        target_dist: dist_to_target,
        yaw_delta: 0.0,
        action: "STOP_AT_TARGET".to_string(),
      });
      break;
    }

    // Stagnation check: 15 ticks without distance decreasing
    if dist_to_target < min_dist - 0.05 {
      min_dist = dist_to_target;
      stagnant_ticks = 0;
    } else {
      stagnant_ticks += 1;
    }

    if stagnant_ticks >= 15 {
      let msg = format!("Stagnation detected: distance did not decrease for 15 ticks (stuck at {dist_to_target:.2}m)");
      eprintln!("[T-M3] ERROR: {msg}");
      nav_error = Some(msg);
      break;
    }

    let target_yaw = (-dx).atan2(dz).to_degrees();
    let yaw_delta = normalize_angle_deg(target_yaw - yaw);

    let action_str = if yaw_delta.abs() > 15.0 {
      win32_input::turn_yaw(yaw_delta as f32);
      format!("TURN(delta={:+.1}°)", yaw_delta)
    } else {
      win32_input::step_forward(Duration::from_millis(350));
      "STEP_FORWARD(W)".to_string()
    };

    println!(
      "| {:04} | ({:6.1}, {:5.1}, {:6.1}) | {:5.1}° | {:10.2}m | {:8.1}° | {:20} |",
      nav_tick, eye.x, eye.y, eye.z, yaw, dist_to_target, yaw_delta, action_str
    );

    trajectory.push(NavPoint {
      tick: nav_tick,
      player_pos: [eye.x, eye.y, eye.z],
      yaw,
      target_dist: dist_to_target,
      yaw_delta,
      action: action_str,
    });

    let elapsed = tick_start.elapsed();
    if elapsed < Duration::from_millis(500) {
      sleep(Duration::from_millis(500) - elapsed);
    }
  }

  if !nav_success && nav_error.is_none() {
    let msg = format!("T-M3 timed out after {} ticks without reaching target distance < 3.5m", args.max_nav_ticks);
    eprintln!("[T-M3] ERROR: {msg}");
    nav_error = Some(msg);
  }

  // Final evaluation: DUAL CONDITION CHECK
  let final_player_pos = trajectory.last().map(|p| p.player_pos).unwrap_or([0.0, 0.0, 0.0]);
  let final_horiz = (final_player_pos[0], final_player_pos[2]);

  let cond1_dist_to_b = dist2(final_horiz, (args.pb.0, args.pb.2));
  let cond2_dist_to_a = dist2(final_horiz, (args.pa.0, args.pa.2));
  let cond2_dist_to_c = dist2(final_horiz, (args.pc.0, args.pc.2));

  let cond1_passed = cond1_dist_to_b < 3.5;
  let cond2_passed = cond2_dist_to_a > 8.0 && cond2_dist_to_c > 8.0;
  let dual_condition_passed = cond1_passed && cond2_passed;

  println!("\n============================================================");
  println!("           T-M3 DUAL CONDITION VERIFICATION                 ");
  println!("============================================================");
  println!("Condition 1: Distance to Chest B Truth < 3.5m");
  println!("  Actual: {:.2}m  -> {}", cond1_dist_to_b, if cond1_passed { "PASS" } else { "FAIL" });
  println!("Condition 2: Distance to Chest A and C Truth > 8.0m");
  println!("  Distance to Chest A: {:.2}m (> 8.0m: {})", cond2_dist_to_a, cond2_dist_to_a > 8.0);
  println!("  Distance to Chest C: {:.2}m (> 8.0m: {})", cond2_dist_to_c, cond2_dist_to_c > 8.0);
  println!("  Condition 2 Result:  {}", if cond2_passed { "PASS" } else { "FAIL" });
  println!(
    "Overall Distinct Recall Result: {}",
    if dual_condition_passed {
      "GO (DUAL PASS)"
    } else {
      "NO-GO (FAILED)"
    }
  );
  println!("============================================================\n");

  let mut verdict_reasons = Vec::new();
  if !cond1_passed {
    verdict_reasons.push(format!("Condition 1 failed: distance to target chest B was {:.2}m (>= 3.5m)", cond1_dist_to_b));
  }
  if !cond2_passed {
    verdict_reasons
      .push(format!("Condition 2 failed: too close to non-target chest (dist A={:.2}m, dist C={:.2}m)", cond2_dist_to_a, cond2_dist_to_c));
  }
  if dual_condition_passed {
    verdict_reasons.push(
      "Agent successfully recalled and navigated to the distinct 2nd chest landmark without confusing with chests A or C.".to_string(),
    );
  }

  let tm3_record = TM3Record {
    target_description: "2nd inserted chest landmark".to_string(),
    target_landmark_id: target_lm_id.clone(),
    target_queried_position: [target_pos.0, target_pos.1, target_pos.2],
    ticks_completed: trajectory.len(),
    final_dist_to_target: trajectory.last().map(|p| p.target_dist).unwrap_or(f64::INFINITY),
    cond1_dist_to_b,
    cond1_passed,
    cond2_dist_to_a,
    cond2_dist_to_c,
    cond2_passed,
    dual_condition_passed,
    trajectory,
    error: nav_error,
  };

  let report = MultiRecallReport {
    schema_version: 1,
    generated_at: {
      let secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
      format!("unix_ts={secs}")
    },
    game_mode: "creative".to_string(),
    verdict: if dual_condition_passed { "GO" } else { "NO-GO" }.to_string(),
    verdict_reasons,
    ground_truth: gt_set,
    tm1_ingest_and_dedup: tm1_record,
    tm2_isolation: tm2_record,
    tm3_distinct_recall: tm3_record,
  };

  if let Some(parent) = args.report_out.parent() {
    let _ = fs::create_dir_all(parent);
  }
  let json = serde_json::to_string_pretty(&report).map_err(|e| format!("json: {e}"))?;
  fs::write(&args.report_out, json).map_err(|e| format!("write report: {e}"))?;
  println!("Summary report successfully written to: {}", args.report_out.display());

  if dual_condition_passed {
    Ok(())
  } else {
    Err("Step 9 dual condition verification failed".to_string())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_pairwise_distance_math() {
    let pa = (-40.0, 95.0, 250.0);
    let pb = (-52.0, 95.0, 265.0);
    let pc = (-38.0, 95.0, 280.0);

    let d_ab = dist2((pa.0, pa.2), (pb.0, pb.2));
    let d_bc = dist2((pb.0, pb.2), (pc.0, pc.2));
    let d_ca = dist2((pc.0, pc.2), (pa.0, pa.2));

    assert!(d_ab > 10.0, "AB distance {d_ab} <= 10m");
    assert!(d_bc > 10.0, "BC distance {d_bc} <= 10m");
    assert!(d_ca > 10.0, "CA distance {d_ca} <= 10m");
  }

  #[test]
  fn test_dual_condition_logic() {
    let pa = (-40.0, 250.0);
    let pb = (-52.0, 265.0);
    let pc = (-38.0, 280.0);

    // Scenario 1: player is 2.0m from B
    let player_near_b = (-51.0, 264.0);
    let cond1 = dist2(player_near_b, pb) < 3.5;
    let cond2 = dist2(player_near_b, pa) > 8.0 && dist2(player_near_b, pc) > 8.0;
    assert!(cond1 && cond2, "Player near B should pass dual condition");

    // Scenario 2: player wrongly navigates to A
    let player_near_a = (-41.0, 251.0);
    let cond1_wrong = dist2(player_near_a, pb) < 3.5;
    assert!(!cond1_wrong, "Player near A should fail condition 1 (not B)");

    // Scenario 3: player between B and C (ambiguous)
    let player_ambiguous = (-45.0, 272.0);
    let dist_b = dist2(player_ambiguous, pb);
    let dist_c = dist2(player_ambiguous, pc);
    assert!(dist_b >= 3.5 || dist_c <= 8.0, "Ambiguous position should not pass dual condition");
  }

  #[test]
  fn test_angle_normalization() {
    assert_eq!(normalize_angle_deg(0.0), 0.0);
    assert_eq!(normalize_angle_deg(180.0), 180.0);
    assert_eq!(normalize_angle_deg(190.0), -170.0);
    assert_eq!(normalize_angle_deg(-190.0), 170.0);
    assert_eq!(normalize_angle_deg(360.0), 0.0);
    assert_eq!(normalize_angle_deg(720.0), 0.0);
  }
}
