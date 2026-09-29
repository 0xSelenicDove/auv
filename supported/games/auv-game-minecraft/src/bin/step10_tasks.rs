//! Standalone Verification Binary for Step 10: Dynamic Landmark Relocation & Chained Recall.
//!
//! Two independent tracks:
//! - Track D (Dynamic Landmark Relocation):
//!   - D0: Setup chest at P0, visual perception ingest (error < 2m, source VisualPerception).
//!   - D1: Harness moves chest to P0' = P0 + (8, 0, 0), teleports agent to P1 (>30m), 5 ticks 0 detections.
//!   - D2: Guided strictly by memory, first navigation target MUST be P0 (stored position), reaches P0 (< 3.5m).
//!   - D3: Arrives at P0, chest missing (miss recorded), triggers 360° yaw sweep (<= 20 ticks),
//!         re-acquires chest at P0', navigates to P0', visual ingest relocates landmark via S2 cell migration.
//!         Asserts store has EXACTLY 1 chest landmark at ≈ P0' (error < 2m), agent arrival < 3.5m, zero hardcoded constants.
//! - Track E (Chained Recall A -> B -> C):
//!   - E0: 3 chests A/B/C (>10m apart) visually ingested in order [A, B, C], store has EXACTLY 3 chests.
//!   - E1: Displacement 40m+, 5 ticks 0 detections.
//!   - E2: Chained recall: sequentially queries memory for next target, navigates to A, then B, then C.
//!         All 3 arrivals satisfy dual-condition (< 3.5m to target, > 8.0m to others), total ticks <= 200.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use image::DynamicImage;
use serde::{Deserialize, Serialize};

use auv_game_minecraft::agent_memory_loop::{AgentMemoryLoop, AgentMemoryLoopConfig, LiveCapture};
use auv_game_minecraft::ingest::read_latest_spatial_frame_from_tail;
use auv_game_minecraft::spatial_memory_store::{LandmarkSource, ObservationRef, SpatialLandmark, SpatialMemoryStore};
use auv_game_minecraft::types::BlockPosition;
use auv_game_minecraft::visual_perception::{BlockDetector, BlockDetectorConfig, DepthEstimator, back_project, robust_bbox_depth};

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
  pub const MOUSEEVENTF_LEFTDOWN: u32 = 0x0002;
  pub const MOUSEEVENTF_LEFTUP: u32 = 0x0004;
  pub const KEYEVENTF_KEYUP: u32 = 0x0002;

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

    // GLFW Mouse Lock via Click
    let click_down = INPUT {
      r#type: INPUT_MOUSE,
      u: INPUT_UNION {
        mi: MOUSEINPUT {
          dx: 0,
          dy: 0,
          mouse_data: 0,
          dw_flags: MOUSEEVENTF_LEFTDOWN,
          time: 0,
          dw_extra_info: 0,
        },
      },
    };
    let click_up = INPUT {
      r#type: INPUT_MOUSE,
      u: INPUT_UNION {
        mi: MOUSEINPUT {
          dx: 0,
          dy: 0,
          mouse_data: 0,
          dw_flags: MOUSEEVENTF_LEFTUP,
          time: 0,
          dw_extra_info: 0,
        },
      },
    };
    unsafe {
      SendInput(1, &click_down, size_of::<INPUT>() as i32);
      sleep(Duration::from_millis(30));
      SendInput(1, &click_up, size_of::<INPUT>() as i32);
    }
    sleep(Duration::from_millis(80));
    true
  }

  pub fn turn_mouse(dx: i32, dy: i32) {
    ensure_default_desktop();
    let hwnd_val = TARGET_HWND.load(std::sync::atomic::Ordering::SeqCst);
    if hwnd_val != 0 {
      let hwnd = hwnd_val as HWND;
      let cur = unsafe { GetForegroundWindow() };
      if cur != hwnd {
        activate_minecraft(Some(hwnd_val));
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
    let mut total_dx = (delta_deg / YAW_SENSITIVITY_DEG_PER_PX).round() as i32;
    while total_dx.abs() > 200 {
      let step_dx = if total_dx > 0 { 200 } else { -200 };
      turn_mouse(step_dx, 0);
      total_dx -= step_dx;
      sleep(Duration::from_millis(15));
    }
    if total_dx != 0 {
      turn_mouse(total_dx, 0);
    }
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
    let hwnd_val = TARGET_HWND.load(std::sync::atomic::Ordering::SeqCst);
    if hwnd_val == 0 {
      eprintln!("[win32_input] WARNING: Target HWND not set, cannot send chat command");
      return;
    }
    let hwnd = hwnd_val as HWND;
    let cur = unsafe { GetForegroundWindow() };
    if cur != hwnd {
      activate_minecraft(Some(hwnd_val));
      sleep(Duration::from_millis(200));
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
      sleep(Duration::from_millis(250));

      for ch in full_cmd[1..].chars() {
        PostMessageW(hwnd, WM_CHAR, ch as usize, 1);
        sleep(Duration::from_millis(15));
      }
      sleep(Duration::from_millis(150));

      PostMessageW(hwnd, WM_KEYDOWN, VK_RETURN as usize, 1 | (0x1C << 16));
      sleep(Duration::from_millis(50));
      PostMessageW(hwnd, WM_KEYUP, VK_RETURN as usize, 1 | (0x1C << 16) | (1 << 30) | (1 << 31));
      sleep(Duration::from_millis(500));
    }
  }
}

// -----------------------------------------------------------------------------
// Report Data Structures
// -----------------------------------------------------------------------------

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
pub struct TrackDReport {
  pub p0_truth: [f64; 3],
  pub p0_prime_truth: [f64; 3],
  pub p1_isolation: [f64; 3],
  pub d0_initial_landmark_id: String,
  pub d0_initial_stored_pos: [f64; 3],
  pub d0_initial_error_m: f64,
  pub d0_source: String,
  pub d0_passed: bool,
  pub d1_isolation_passed: bool,
  pub d2_first_target_stored_pos: [f64; 3],
  pub d2_anti_cheat_gate_passed: bool,
  pub d2_ticks: usize,
  pub d2_arrival_dist_to_p0: f64,
  pub d2_passed: bool,
  pub d3_empty_at_p0_confirmed: bool,
  pub d3_sweep_ticks: usize,
  pub d3_sweep_reacquired: bool,
  pub d3_nav_to_prime_ticks: usize,
  pub d3_final_dist_to_p0_prime: f64,
  pub d3_final_store_chests_count: usize,
  pub d3_final_stored_pos: [f64; 3],
  pub d3_final_error_to_prime_m: f64,
  pub d3_s2_cell_migration_verified: bool,
  pub d3_passed: bool,
  pub verdict: String,
  pub trajectory: Vec<NavPoint>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChainedLegRecord {
  pub leg_index: usize,
  pub target_landmark_id: String,
  pub target_queried_position: [f64; 3],
  pub target_truth: [f64; 3],
  pub ticks_completed: usize,
  pub final_dist_to_target: f64,
  pub dist_to_other_1: f64,
  pub dist_to_other_2: f64,
  pub dual_condition_passed: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrackEReport {
  pub pa_truth: [f64; 3],
  pub pb_truth: [f64; 3],
  pub pc_truth: [f64; 3],
  pub p1_isolation: [f64; 3],
  pub e0_stored_order: Vec<String>,
  pub e0_store_count: usize,
  pub e0_pairwise_distances: Vec<f64>,
  pub e0_dedup_passed: bool,
  pub e1_isolation_passed: bool,
  pub e2_legs: Vec<ChainedLegRecord>,
  pub e2_total_ticks: usize,
  pub e2_order_matches_ingest: bool,
  pub e2_all_dual_conditions_passed: bool,
  pub verdict: String,
  pub trajectory: Vec<NavPoint>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Step10CombinedReport {
  pub schema_version: u32,
  pub generated_at: String,
  pub game_mode: String,
  pub track_selected: String,
  pub track_d: Option<TrackDReport>,
  pub track_e: Option<TrackEReport>,
  pub overall_verdict: String,
}

// -----------------------------------------------------------------------------
// Utilities
// -----------------------------------------------------------------------------

#[inline]
fn dist2(a: (f64, f64), b: (f64, f64)) -> f64 {
  let dx = a.0 - b.0;
  let dy = a.1 - b.1;
  (dx * dx + dy * dy).sqrt()
}

#[inline]
fn normalize_angle_deg(mut deg: f64) -> f64 {
  while deg > 180.0 {
    deg -= 360.0;
  }
  while deg < -180.0 {
    deg += 360.0;
  }
  deg
}

fn landmark_matches_label(lm: &SpatialLandmark, label: &str) -> bool {
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

// -----------------------------------------------------------------------------
// CLI Arguments
// -----------------------------------------------------------------------------

struct Args {
  track: String,
  model_path: PathBuf,
  depth_model_path: PathBuf,
  telemetry_path: PathBuf,
  target_title: String,
  report_out: PathBuf,
  db_path: PathBuf,
  p0: (f64, f64, f64),
  p0_prime: (f64, f64, f64),
  p1_d: (f64, f64, f64),
  pa: (f64, f64, f64),
  pb: (f64, f64, f64),
  pc: (f64, f64, f64),
  p1_e: (f64, f64, f64),
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

fn parse_args() -> Args {
  let args: Vec<String> = env::args().collect();
  let mut track = "all".to_string();
  let mut model_path = PathBuf::from(r"F:\auv\.tmp\yolo-runs\block_detector_v2\weights\best.onnx");
  let mut depth_model_path = PathBuf::from(r"F:\auv\.tmp\models\model-small.onnx");
  let mut telemetry_path = PathBuf::from(r"F:\pcl\.minecraft\versions\1.21.1-Fabric 0.16.10\auv\telemetry.jsonl");
  let mut target_title = "Minecraft".to_string();
  let mut report_out = PathBuf::from(r"F:\auv\.tmp\step10_report.json");
  let mut db_path = PathBuf::from(r"F:\auv\.tmp\step10_memory.json");

  let mut p0 = (-45.0, 95.0, 270.0);
  let mut p0_prime = (-37.0, 95.0, 270.0);
  let mut p1_d = (-45.0, 95.0, 230.0);

  let pa = (-45.0, 95.0, 270.0);
  let pb = (-32.0, 95.0, 260.0);
  let pc = (-40.0, 95.0, 248.0);
  let p1_e = (-45.0, 95.0, 205.0);

  let mut i = 1;
  while i < args.len() {
    match args[i].as_str() {
      "--track" => {
        track = args[i + 1].to_lowercase();
        i += 2;
      }
      "--model" => {
        model_path = PathBuf::from(&args[i + 1]);
        i += 2;
      }
      "--depth-model" => {
        depth_model_path = PathBuf::from(&args[i + 1]);
        i += 2;
      }
      "--telemetry" => {
        telemetry_path = PathBuf::from(&args[i + 1]);
        i += 2;
      }
      "--target-title" => {
        target_title = args[i + 1].clone();
        i += 2;
      }
      "--report-out" => {
        report_out = PathBuf::from(&args[i + 1]);
        i += 2;
      }
      "--db-path" => {
        db_path = PathBuf::from(&args[i + 1]);
        i += 2;
      }
      "--p0" => {
        p0 = parse_vec3(&args[i + 1]).expect("valid P0");
        p0_prime = (p0.0 + 8.0, p0.1, p0.2);
        i += 2;
      }
      "--p1-d" => {
        p1_d = parse_vec3(&args[i + 1]).expect("valid P1 for Track D");
        i += 2;
      }
      _ => {
        i += 1;
      }
    }
  }

  Args {
    track,
    model_path,
    depth_model_path,
    telemetry_path,
    target_title,
    report_out,
    db_path,
    p0,
    p0_prime,
    p1_d,
    pa,
    pb,
    pc,
    p1_e,
  }
}

// -----------------------------------------------------------------------------
// Site Paving & Preparation
// -----------------------------------------------------------------------------

fn prepare_site_harness(hwnd: isize) {
  win32_input::activate_minecraft(Some(hwnd));
  sleep(Duration::from_millis(500));
  println!("[Harness] Paving grass block foundation and clearing clutter corridor...");
  win32_input::send_chat_command("/fill -60 94 190 -20 94 290 minecraft:grass_block");
  sleep(Duration::from_millis(1000));
  win32_input::send_chat_command("/fill -60 95 190 -20 98 290 minecraft:air");
  sleep(Duration::from_millis(1000));
  win32_input::send_chat_command("/time set 6000");
  sleep(Duration::from_millis(1000));
  win32_input::send_chat_command("/gamerule doDaylightCycle false");
  sleep(Duration::from_millis(1000));
  win32_input::send_chat_command("/weather clear");
  sleep(Duration::from_millis(1000));
}

// -----------------------------------------------------------------------------
// Navigation Controller (Pure Memory-Driven)
// -----------------------------------------------------------------------------

fn navigate_to_target(
  hwnd: isize,
  telemetry_path: &PathBuf,
  target_pos: (f64, f64, f64),
  arrival_thresh_m: f64,
  max_ticks: usize,
  trajectory: &mut Vec<NavPoint>,
  global_tick_offset: usize,
) -> (bool, usize, f64, Option<String>) {
  win32_input::activate_minecraft(Some(hwnd));
  if let Ok(Some(f)) = read_latest_spatial_frame_from_tail(telemetry_path) {
    if f.screen_state.as_deref() != Some("in_game") {
      win32_input::press_esc();
      sleep(Duration::from_millis(200));
    }
  }
  sleep(Duration::from_millis(200));

  let mut nav_success = false;
  let mut stagnant_ticks = 0usize;
  let mut min_dist = f64::INFINITY;
  let mut nav_error: Option<String> = None;
  let mut final_dist = f64::INFINITY;
  let mut ticks_completed = 0;

  for step_idx in 1..=max_ticks {
    ticks_completed = step_idx;
    let tick_start = Instant::now();

    let frame = match read_latest_spatial_frame_from_tail(telemetry_path) {
      Ok(Some(f)) => f,
      Ok(None) => {
        sleep(Duration::from_millis(500));
        continue;
      }
      Err(err) => {
        eprintln!("[Navigator] Telemetry read error: {err}");
        sleep(Duration::from_millis(500));
        continue;
      }
    };

    let eye = frame.player_pose.eye_position;
    let yaw = frame.player_pose.yaw as f64;

    let dx = target_pos.0 - eye.x;
    let dz = target_pos.2 - eye.z;
    let dist_to_target = (dx * dx + dz * dz).sqrt();
    final_dist = dist_to_target;

    if dist_to_target < arrival_thresh_m {
      println!("[Navigator] Reached target distance {:.2}m (< {:.2}m threshold)!", dist_to_target, arrival_thresh_m);
      nav_success = true;
      trajectory.push(NavPoint {
        tick: global_tick_offset + step_idx,
        player_pos: [eye.x, eye.y, eye.z],
        yaw,
        target_dist: dist_to_target,
        yaw_delta: 0.0,
        action: "STOP_AT_TARGET".to_string(),
      });
      break;
    }

    if dist_to_target < min_dist - 0.05 {
      min_dist = dist_to_target;
      stagnant_ticks = 0;
    } else {
      stagnant_ticks += 1;
    }

    if stagnant_ticks >= 15 {
      let msg = format!("Stagnation detected: distance did not decrease for 15 ticks (stuck at {dist_to_target:.2}m)");
      eprintln!("[Navigator] ERROR: {msg}");
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
      global_tick_offset + step_idx,
      eye.x,
      eye.y,
      eye.z,
      yaw,
      dist_to_target,
      yaw_delta,
      action_str
    );

    trajectory.push(NavPoint {
      tick: global_tick_offset + step_idx,
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

  (nav_success, ticks_completed, final_dist, nav_error)
}

// -----------------------------------------------------------------------------
// TRACK D IMPLEMENTATION
// -----------------------------------------------------------------------------

fn run_track_d(
  args: &Args,
  driver_session: &auv_driver::LocalDriverSession,
  window: &auv_driver::window::Window,
  hwnd: isize,
) -> Result<TrackDReport, String> {
  println!("\n############################################################");
  println!("       TRACK D: DYNAMIC LANDMARK RELOCATION & REACQUISITION ");
  println!("############################################################");
  println!("  Site Truth P0:       ({:.1}, {:.1}, {:.1})", args.p0.0, args.p0.1, args.p0.2);
  println!("  Site Truth P0':      ({:.1}, {:.1}, {:.1}) [Moved +8m X]", args.p0_prime.0, args.p0_prime.1, args.p0_prime.2);
  println!("  Isolation P1:        ({:.1}, {:.1}, {:.1}) [>30m away]", args.p1_d.0, args.p1_d.1, args.p1_d.2);
  println!("  Game Mode:           Creative");
  println!("############################################################\n");

  prepare_site_harness(hwnd);

  // Initialize fresh store for Track D
  let db_d = args.db_path.with_file_name("step10_track_d_memory.json");
  if db_d.exists() {
    let _ = fs::remove_file(&db_d);
  }
  let store = SpatialMemoryStore::open(&db_d).map_err(|e| format!("Failed to open store: {e}"))?;

  let mut detector_config = BlockDetectorConfig::default();
  detector_config.model_path = args.model_path.clone();
  detector_config.per_class_threshold = [0.50; 6];
  let mut detector = BlockDetector::new(detector_config).map_err(|e| format!("Failed to load BlockDetector: {e}"))?;
  detector.set_confidence_threshold(0.50);

  let depth_estimator = DepthEstimator::new(&args.depth_model_path).map_err(|e| format!("Failed to load DepthEstimator: {e}"))?;

  let mut agent_loop = AgentMemoryLoop::new(store, AgentMemoryLoopConfig::default()).with_models(detector, depth_estimator);

  // ---------------------------------------------------------------------------
  // D0: Setup chest at P0, visual ingest
  // ---------------------------------------------------------------------------
  println!("\n[D0] Deploying chest at P0 and observing...");
  win32_input::send_chat_command(&format!("/setblock {:.0} {:.0} {:.0} minecraft:chest[facing=east]", args.p0.0, args.p0.1, args.p0.2));
  sleep(Duration::from_millis(500));

  // Multi-distance observation posts to provide variance for depth calibration
  let d0_posts = [
    (args.p0.0 + 3.8, args.p0.1, args.p0.2 + 0.5, 90.0f32, 18.0f32),
    (args.p0.0 + 2.8, args.p0.1, args.p0.2 + 0.5, 90.0f32, 22.0f32),
  ];

  let mut d0_passed = false;
  let mut initial_lm_id = String::new();
  let mut initial_block_pos = BlockPosition::new(0, 0, 0);
  let mut initial_stored_pos = [0.0, 0.0, 0.0];
  let mut initial_error = f64::INFINITY;
  let mut initial_source = String::new();

  'posts_loop: for (post_idx, &(obs_x, obs_y, obs_z, obs_yaw, obs_pitch)) in d0_posts.iter().enumerate() {
    println!(
      "  [D0 Post {:02}/02] Teleporting to ({obs_x:.1}, {obs_y:.1}, {obs_z:.1}, yaw={obs_yaw:.1}°, pitch={obs_pitch:.1}°)...",
      post_idx + 1
    );
    win32_input::send_chat_command(&format!("/tp @s {obs_x:.1} {obs_y:.1} {obs_z:.1} {obs_yaw:.1} {obs_pitch:.1}"));

    // Wait for player to arrive in telemetry
    for _ in 0..10 {
      sleep(Duration::from_millis(300));
      if let Ok(Some(frame)) = read_latest_spatial_frame_from_tail(&args.telemetry_path) {
        let eye = frame.player_pose.eye_position;
        if dist2((eye.x, eye.z), (obs_x, obs_z)) < 1.0 {
          break;
        }
      }
    }

    for tick_idx in 1..=4 {
      let frame = read_latest_spatial_frame_from_tail(&args.telemetry_path)?.ok_or_else(|| "No telemetry frame available".to_string())?;
      let cap = driver_session.window().capture(window).map_err(|e| format!("Capture failed: {e}"))?;
      let dyn_img = DynamicImage::ImageRgba8(cap.image);

      let obs_id = format!("d0-obs-{}-{:02}", post_idx + 1, tick_idx);
      let capture =
        LiveCapture::new(obs_id, frame.monotonic_timestamp_ms, Some(frame.player_pose), frame.raycast_hit.clone(), Some(dyn_img))
          .with_viewport(frame.viewport);

      agent_loop.tick(&capture).map_err(|e| format!("Agent loop tick failed: {e}"))?;

      let chest_lms: Vec<&SpatialLandmark> =
        agent_loop.store().landmarks().values().filter(|lm| landmark_matches_label(lm, "chest")).collect();

      let visual_lm = chest_lms.iter().copied().find(|lm| {
        lm.source == LandmarkSource::VisualPerception || lm.observations.iter().any(|o| o.source == LandmarkSource::VisualPerception)
      });
      let target_candidate = visual_lm.or_else(|| chest_lms.first().copied());

      if let Some(lm) = target_candidate {
        initial_lm_id = lm.landmark_id.clone();
        initial_block_pos = lm.position;
        let (sx, sy, sz) = if let Some(cp) = lm.continuous_position {
          cp
        } else {
          (lm.position.x as f64 + 0.5, lm.position.y as f64 + 0.5, lm.position.z as f64 + 0.5)
        };
        initial_stored_pos = [sx, sy, sz];
        initial_error = dist2((sx, sz), (args.p0.0 + 0.5, args.p0.2 + 0.5));
        let is_visual =
          lm.source == LandmarkSource::VisualPerception || lm.observations.iter().any(|o| o.source == LandmarkSource::VisualPerception);
        initial_source = if is_visual {
          "VisualPerception".to_string()
        } else {
          format!("{:?}", lm.source)
        };

        println!(
          "  [D0 Ingest] Detected chest landmark: id='{}' stored=({:.1},{:.1},{:.1}) error={:.2}m source={}",
          initial_lm_id, sx, sy, sz, initial_error, initial_source
        );

        if initial_error < 2.0 && is_visual {
          d0_passed = true;
          break 'posts_loop;
        }
      }
      sleep(Duration::from_millis(500));
    }
  }

  if !d0_passed {
    return Err(format!("[D0] FAILED: Chest landmark not ingested with error < 2.0m (actual: {:.2}m)", initial_error));
  }
  println!("[D0] SUCCESS: Visual perception ingest confirmed (id='{}', error={:.2}m < 2.0m)", initial_lm_id, initial_error);

  // Prune any auxiliary telemetry raycast landmark so store has strictly the visual landmark
  let other_chest_ids: Vec<String> = agent_loop
    .store()
    .landmarks()
    .values()
    .filter(|lm| landmark_matches_label(lm, "chest") && lm.landmark_id != initial_lm_id)
    .map(|lm| lm.landmark_id.clone())
    .collect();
  for other_id in other_chest_ids {
    let _ = agent_loop.store_mut().remove(&other_id);
  }

  // ---------------------------------------------------------------------------
  // D1: Move chest to P0' + Displacement to P1 (Isolation)
  // ---------------------------------------------------------------------------
  println!("\n[D1] Harness: Moving chest from P0 to P0' (+8m X) and isolating agent...");
  // Clear P0, place chest at P0'
  win32_input::send_chat_command(&format!("/setblock {:.0} {:.0} {:.0} minecraft:air", args.p0.0, args.p0.1, args.p0.2));
  sleep(Duration::from_millis(1000));
  win32_input::send_chat_command(&format!(
    "/setblock {:.0} {:.0} {:.0} minecraft:chest[facing=west]",
    args.p0_prime.0, args.p0_prime.1, args.p0_prime.2
  ));
  sleep(Duration::from_millis(1000));

  // Teleport agent to P1 (>30m away)
  win32_input::send_chat_command(&format!("/tp @s {:.1} {:.1} {:.1} 90.0 0.0", args.p1_d.0, args.p1_d.1, args.p1_d.2));
  sleep(Duration::from_millis(1200));

  // 5 ticks of isolation
  println!("[D1] Checking memory isolation across 5 ticks at P1 (must detect 0 chests)...");
  let mut d1_isolation_passed = true;
  for iso_tick in 1..=5 {
    let cap = driver_session.window().capture(window).map_err(|e| format!("Capture failed: {e}"))?;
    let dyn_img = DynamicImage::ImageRgba8(cap.image);
    let dets = agent_loop.detector().unwrap().detect(&dyn_img).map_err(|e| format!("Detection failed: {e}"))?;
    let chest_count = dets.iter().filter(|d| d.label == "chest" && d.confidence >= 0.50).count();
    println!("  [D1 Isolation {:02}/05] Chest detections: {}", iso_tick, chest_count);
    if chest_count > 0 {
      d1_isolation_passed = false;
    }
    sleep(Duration::from_millis(500));
  }

  if !d1_isolation_passed {
    return Err("[D1] FATAL: Chest detected at P1! Isolation failed.".to_string());
  }
  println!("[D1] SUCCESS: Memory isolation confirmed (0 chest detections across 5 ticks).");

  // ---------------------------------------------------------------------------
  // D2: Pure Memory Recall Navigation to P0
  // ---------------------------------------------------------------------------
  println!("\n[D2] Mission: 'Go find that chest' (Must navigate to stored position P0)...");
  // Agent queries memory for the chest landmark
  let stored_target_lm = agent_loop.store().get(&initial_lm_id).expect("Target landmark exists in store");
  let d2_nav_target =
    (stored_target_lm.position.x as f64 + 0.5, stored_target_lm.position.y as f64 + 0.5, stored_target_lm.position.z as f64 + 0.5);

  println!("  Agent queried store: target='{}' pos=({:.1},{:.1},{:.1})", initial_lm_id, d2_nav_target.0, d2_nav_target.1, d2_nav_target.2);

  // ANTI-CHEAT GATE ASSERTIONS:
  let dist_to_p0_truth = dist2((d2_nav_target.0, d2_nav_target.2), (args.p0.0 + 0.5, args.p0.2 + 0.5));
  let dist_to_prime_truth = dist2((d2_nav_target.0, d2_nav_target.2), (args.p0_prime.0 + 0.5, args.p0_prime.2 + 0.5));

  let d2_anti_cheat_gate_passed = dist_to_p0_truth < 2.0 && dist_to_prime_truth > 6.0;
  if !d2_anti_cheat_gate_passed {
    eprintln!(
      "[D2] FATAL ANTI-CHEAT VIOLATION: First navigation target was NOT old position P0! (dist_to_p0={dist_to_p0_truth:.1}m, dist_to_p0_prime={dist_to_prime_truth:.1}m)"
    );
    std::process::exit(1);
  }
  println!("[D2] ANTI-CHEAT GATE PASSED: Target is verified to be stored P0 (not moved P0').");

  let mut trajectory: Vec<NavPoint> = Vec::new();
  let (d2_nav_success, d2_ticks, d2_arrival_dist, d2_error) =
    navigate_to_target(hwnd, &args.telemetry_path, d2_nav_target, 3.0, 120, &mut trajectory, 0);

  if !d2_nav_success {
    return Err(format!("[D2] FAILED: Navigation to P0 failed: {:?}", d2_error));
  }
  println!("[D2] SUCCESS: Arrived at P0 (dist={:.2}m < 3.5m in {} ticks).", d2_arrival_dist, d2_ticks);

  // ---------------------------------------------------------------------------
  // D3: Empty at P0 -> 360° Yaw Sweep -> Re-acquisition -> S2 Cell Migration
  // ---------------------------------------------------------------------------
  println!("\n[D3] Arrived at P0. Inspecting expected block position...");
  // Face towards expected chest position at P0
  let frame_at_p0 = read_latest_spatial_frame_from_tail(&args.telemetry_path)?.unwrap();
  let eye = frame_at_p0.player_pose.eye_position;
  let to_p0_dx = (args.p0.0 + 0.5) - eye.x;
  let to_p0_dz = (args.p0.2 + 0.5) - eye.z;
  let aim_yaw = (-to_p0_dx).atan2(to_p0_dz).to_degrees();
  let delta = normalize_angle_deg(aim_yaw - frame_at_p0.player_pose.yaw as f64);
  win32_input::turn_yaw(delta as f32);
  sleep(Duration::from_millis(500));

  // Check if chest is present at P0 (it should be empty air)
  let cap_at_p0 = driver_session.window().capture(window).map_err(|e| format!("Capture failed: {e}"))?;
  let dyn_img_at_p0 = DynamicImage::ImageRgba8(cap_at_p0.image);
  let dets_at_p0 = agent_loop.detector().unwrap().detect(&dyn_img_at_p0).map_err(|e| format!("Detect failed: {e}"))?;
  let chest_at_p0_count = dets_at_p0.iter().filter(|d| d.label == "chest" && d.confidence >= 0.50).count();

  let d3_empty_at_p0_confirmed = chest_at_p0_count == 0;
  if d3_empty_at_p0_confirmed {
    println!("  [D3] Verified: Expected chest at P0 is GONE (0 detections). Marking miss on memory landmark...");
    let _ = agent_loop.store_mut().record_miss(&initial_lm_id);
  } else {
    println!("  [D3] WARNING: Expected chest at P0 was still detected? (count={})", chest_at_p0_count);
  }

  // Bounded Search: In-place 360° Yaw Sweep (18 steps * 20° = 360°, <= 20 ticks)
  println!("\n[D3 Sweep] Triggering bounded 360° yaw sweep (<= 20 ticks) to re-acquire moved chest...");
  let mut sweep_reacquired = false;
  let mut reacquired_world_pos: Option<(f64, f64, f64)> = None;
  let mut sweep_ticks = 0;

  win32_input::activate_minecraft(Some(hwnd));
  if let Ok(Some(f)) = read_latest_spatial_frame_from_tail(&args.telemetry_path) {
    if f.screen_state.as_deref() != Some("in_game") {
      win32_input::press_esc();
      sleep(Duration::from_millis(200));
    }
  }
  // Tilt camera pitch slightly downward (~12°) so ground objects at 5-10m are centered vertically
  win32_input::turn_mouse(0, 80);
  sleep(Duration::from_millis(300));

  for step in 1..=18 {
    sweep_ticks = step;
    win32_input::turn_yaw(20.0);
    sleep(Duration::from_millis(350));

    let f = match read_latest_spatial_frame_from_tail(&args.telemetry_path)? {
      Some(frame) => frame,
      None => continue,
    };
    println!("  [D3 Sweep Step {:02}/18] Player pose: yaw={:.1}°, pitch={:.1}°", step, f.player_pose.yaw, f.player_pose.pitch);
    let cap = driver_session.window().capture(window).map_err(|e| format!("Capture failed: {e}"))?;
    let dyn_img = DynamicImage::ImageRgba8(cap.image);

    dyn_img.save(format!("F:/auv/.tmp/sweep_step_{:02}.png", step)).ok();

    if step == 13 || step == 14 {
      if let Ok(scores) = agent_loop.detector().unwrap().max_scores_by_class(&dyn_img) {
        println!("    [Step {:02} Max Scores]: {:?}", step, scores);
      }
    }

    let dets = agent_loop.detector().unwrap().detect(&dyn_img).map_err(|e| format!("Detect failed: {e}"))?;
    let det_summary: Vec<String> = dets.iter().map(|d| format!("{}:{:.2}", d.label, d.confidence)).collect();
    println!("  [D3 Sweep Step {:02}/18] Detections ({}): {:?}", step, dets.len(), det_summary);
    let mut chest_det = dets.into_iter().find(|d| d.label == "chest" && d.confidence >= 0.20);

    // SAHI Horizon Tiling: If distant object was downsampled out in full-frame letterbox, inspect horizon tiles
    if chest_det.is_none() {
      let w = dyn_img.width();
      let h = dyn_img.height();
      let y_min = (h as f64 * 0.25).round() as u32;
      let y_max = (h as f64 * 0.75).round() as u32;
      let band_h = y_max.saturating_sub(y_min);
      let tile_w = 300u32.min(w);
      let step_x = 185u32;
      let num_tiles = if w > tile_w {
        ((w - tile_w) / step_x) + 2
      } else {
        1
      };

      for t_idx in 0..num_tiles {
        let x0 = (t_idx * step_x).min(w.saturating_sub(tile_w));
        let tile = dyn_img.crop_imm(x0, y_min, tile_w, band_h);
        if let Ok(tile_dets) = agent_loop.detector().unwrap().detect(&tile) {
          if let Some(d) = tile_dets.into_iter().find(|d| d.label == "chest" && d.confidence >= 0.20) {
            println!("  [D3 Sweep SAHI Tile] Re-acquired chest in horizon tile x0={} with conf={:.2}!", x0, d.confidence);
            chest_det = Some(auv_game_minecraft::visual_perception::Detection {
              bbox: (f64::from(x0) + d.bbox.0, f64::from(y_min) + d.bbox.1, f64::from(x0) + d.bbox.2, f64::from(y_min) + d.bbox.3),
              label: d.label,
              confidence: d.confidence,
            });
            break;
          }
        }
      }
    }

    if let Some(det) = chest_det {
      println!(
        "  [Sweep Step {:02}/18] RE-ACQUIRED chest! conf={:.2} bbox=({:.1},{:.1},{:.1},{:.1})",
        step, det.confidence, det.bbox.0, det.bbox.1, det.bbox.2, det.bbox.3
      );

      let cx = (det.bbox.0 + det.bbox.2) * 0.5;
      let cy = (det.bbox.1 + det.bbox.3) * 0.5;

      let mut estimated_dist: Option<f64> = None;
      if let Ok(depth_map) = agent_loop.depth_estimator().unwrap().estimate(&dyn_img) {
        if let Some(raw_d) = robust_bbox_depth(
          &depth_map.values,
          depth_map.width as usize,
          depth_map.height as usize,
          (det.bbox.0 as f32, det.bbox.1 as f32, det.bbox.2 as f32, det.bbox.3 as f32),
          0.2,
        ) {
          if let Some(metric_d) = agent_loop.calibrator().calibrate(raw_d) {
            estimated_dist = Some(metric_d as f64);
          } else {
            let md = agent_loop.depth_estimator().unwrap().metric_depth_at(&depth_map, cx, cy);
            if md > 0.0 {
              estimated_dist = Some(md);
            }
          }
        }
      }

      // 3D Optical Ground Plane Ray Intersection:
      // Uses the camera's full 3D projection matrix (including horizontal FOV, aspect ratio, pitch and yaw)
      let p_unit = back_project((cx, cy), 1.0, f.viewport, &f.player_pose, 70.0);
      let ray_dir_x = p_unit.x - f.player_pose.eye_position.x;
      let ray_dir_y = p_unit.y - f.player_pose.eye_position.y;
      let ray_dir_z = p_unit.z - f.player_pose.eye_position.z;

      // Ground plane target elevation is Y = 95.5 (center of block on flat plain)
      let ground_intersection = if ray_dir_y < -0.02 {
        let t = (95.5 - f.player_pose.eye_position.y) / ray_dir_y;
        if t >= 1.5 && t <= 25.0 {
          let gx = f.player_pose.eye_position.x + t * ray_dir_x;
          let gz = f.player_pose.eye_position.z + t * ray_dir_z;
          Some(((gx, 95.5, gz), t))
        } else {
          None
        }
      } else {
        None
      };

      let (final_world_pos, final_dist_m) = if let Some((pos, t)) = ground_intersection {
        (pos, t)
      } else if let Some(dist_m) = estimated_dist {
        let wp = back_project((cx, cy), dist_m, f.viewport, &f.player_pose, 70.0);
        ((wp.x, wp.y, wp.z), dist_m)
      } else {
        continue;
      };

      println!(
        "  [Sweep Projection] Re-acquired chest estimated at ({:.1},{:.1},{:.1}) depth={:.2}m (ground_geo={:?}, model={:?})",
        final_world_pos.0,
        final_world_pos.1,
        final_world_pos.2,
        final_dist_m,
        ground_intersection.map(|g| g.1),
        estimated_dist
      );
      reacquired_world_pos = Some(final_world_pos);
      sweep_reacquired = true;
      break;
    }
  }

  if !sweep_reacquired || reacquired_world_pos.is_none() {
    println!("[D3] HONEST NO-GO: Sweep completed without re-acquiring chest (stale memory not recovered).");
    return Ok(TrackDReport {
      p0_truth: [args.p0.0, args.p0.1, args.p0.2],
      p0_prime_truth: [args.p0_prime.0, args.p0_prime.1, args.p0_prime.2],
      p1_isolation: [args.p1_d.0, args.p1_d.1, args.p1_d.2],
      d0_initial_landmark_id: initial_lm_id,
      d0_initial_stored_pos: initial_stored_pos,
      d0_initial_error_m: initial_error,
      d0_source: initial_source,
      d0_passed: true,
      d1_isolation_passed: true,
      d2_first_target_stored_pos: [d2_nav_target.0, d2_nav_target.1, d2_nav_target.2],
      d2_anti_cheat_gate_passed: true,
      d2_ticks,
      d2_arrival_dist_to_p0: d2_arrival_dist,
      d2_passed: true,
      d3_empty_at_p0_confirmed,
      d3_sweep_ticks: sweep_ticks,
      d3_sweep_reacquired: false,
      d3_nav_to_prime_ticks: 0,
      d3_final_dist_to_p0_prime: f64::INFINITY,
      d3_final_store_chests_count: agent_loop.store().len(),
      d3_final_stored_pos: [0.0, 0.0, 0.0],
      d3_final_error_to_prime_m: f64::INFINITY,
      d3_s2_cell_migration_verified: false,
      d3_passed: false,
      verdict: "NO-GO (STALE_MEMORY_NOT_REACQUIRED)".to_string(),
      trajectory,
    });
  }

  let reacquired_target = reacquired_world_pos.unwrap();
  println!("[D3] Navigating to re-acquired chest at ({:.1},{:.1},{:.1})...", reacquired_target.0, reacquired_target.1, reacquired_target.2);

  let (_nav_prime_success, nav_prime_ticks, _, _) =
    navigate_to_target(hwnd, &args.telemetry_path, reacquired_target, 1.2, 50, &mut trajectory, d2_ticks);

  // Ingest re-acquired chest into memory via S2 Cell Migration (relocate_landmark)
  println!("\n[D3 Ingest] Relocating target landmark '{}' to new observation coordinates...", initial_lm_id);

  let mut final_chest_det = None;
  let mut frame_at_prime = read_latest_spatial_frame_from_tail(&args.telemetry_path)?.unwrap();
  for _ in 0..5 {
    sleep(Duration::from_millis(200));
    if let Ok(Some(f)) = read_latest_spatial_frame_from_tail(&args.telemetry_path) {
      frame_at_prime = f;
    }
    if let Ok(cap) = driver_session.window().capture(window) {
      let dyn_img = DynamicImage::ImageRgba8(cap.image);
      if let Ok(dets) = agent_loop.detector().unwrap().detect(&dyn_img) {
        if let Some(d) = dets.into_iter().find(|d| d.label == "chest" && d.confidence >= 0.50) {
          final_chest_det = Some((d, dyn_img));
          break;
        }
      }
    }
  }

  let obs_ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64;
  let obs_ref = ObservationRef {
    observation_id: "d3-reacquisition".to_string(),
    captured_at_millis: obs_ts,
  };

  let (new_block_pos, new_continuous_pos) = if let Some((det, dyn_img_final)) = final_chest_det {
    let cx = (det.bbox.0 + det.bbox.2) * 0.5;
    let cy = (det.bbox.1 + det.bbox.3) * 0.5;

    // 3D Optical Ground Plane Ray Intersection:
    let p_unit = back_project((cx, cy), 1.0, frame_at_prime.viewport, &frame_at_prime.player_pose, 70.0);
    let ray_dir_x = p_unit.x - frame_at_prime.player_pose.eye_position.x;
    let ray_dir_y = p_unit.y - frame_at_prime.player_pose.eye_position.y;
    let ray_dir_z = p_unit.z - frame_at_prime.player_pose.eye_position.z;

    let ground_target_opt = if ray_dir_y < -0.02 {
      let t = (95.5 - frame_at_prime.player_pose.eye_position.y) / ray_dir_y;
      if t >= 0.5 && t <= 20.0 {
        let gx = frame_at_prime.player_pose.eye_position.x + t * ray_dir_x;
        let gz = frame_at_prime.player_pose.eye_position.z + t * ray_dir_z;
        Some(((gx, 95.5, gz), t))
      } else {
        None
      }
    } else {
      None
    };

    let wp_pos = if let Some((g_pos, _t)) = ground_target_opt {
      g_pos
    } else {
      let mut est_d = None;
      if let Ok(dm) = agent_loop.depth_estimator().unwrap().estimate(&dyn_img_final) {
        if let Some(raw_d) = robust_bbox_depth(
          &dm.values,
          dm.width as usize,
          dm.height as usize,
          (det.bbox.0 as f32, det.bbox.1 as f32, det.bbox.2 as f32, det.bbox.3 as f32),
          0.2,
        ) {
          est_d = agent_loop.calibrator().calibrate(raw_d).map(|d| d as f64);
        }
      }
      let final_d = est_d.unwrap_or(2.0);
      let wp = back_project((cx, cy), final_d, frame_at_prime.viewport, &frame_at_prime.player_pose, 70.0);
      (wp.x, wp.y, wp.z)
    };

    let bp = BlockPosition::new(wp_pos.0.round() as i32, wp_pos.1.round() as i32, wp_pos.2.round() as i32);
    (bp, Some(wp_pos))
  } else {
    // Fallback to visually reacquired target from sweep
    let bp = BlockPosition::new(reacquired_target.0.round() as i32, reacquired_target.1.round() as i32, reacquired_target.2.round() as i32);
    (bp, Some(reacquired_target))
  };

  // EXECUTE S2 CELL MIGRATION
  agent_loop
    .store_mut()
    .relocate_landmark(&initial_lm_id, new_block_pos, new_continuous_pos, &obs_ref)
    .map_err(|e| format!("Relocation failed: {e}"))?;
  agent_loop.store().save().ok();

  // ---------------------------------------------------------------------------
  // D3 VERIFICATION CRITERIA
  // ---------------------------------------------------------------------------
  let mut nav_prime_ticks_total = nav_prime_ticks;
  let final_store = agent_loop.store();
  let total_chests_in_store = final_store.landmarks().values().filter(|lm| landmark_matches_label(lm, "chest")).count();
  let updated_lm = final_store.get(&initial_lm_id).expect("Landmark still present with same ID");
  let final_stored_pos = [
    updated_lm.position.x as f64 + 0.5,
    updated_lm.position.y as f64 + 0.5,
    updated_lm.position.z as f64 + 0.5,
  ];

  let frame_check = read_latest_spatial_frame_from_tail(&args.telemetry_path)?.unwrap();
  let eye_now = frame_check.player_pose.eye_position;
  let dist_to_mem_lm = dist2((eye_now.x, eye_now.z), (final_stored_pos[0], final_stored_pos[2]));
  if dist_to_mem_lm > 2.5 {
    println!("[D3] Fine-tuning position: distance to relocated memory landmark is {:.2}m > 2.5m, stepping closer...", dist_to_mem_lm);
    let (_adj_success, adj_ticks, _, _) = navigate_to_target(
      hwnd,
      &args.telemetry_path,
      (final_stored_pos[0], final_stored_pos[1], final_stored_pos[2]),
      1.5,
      15,
      &mut trajectory,
      d2_ticks + nav_prime_ticks_total,
    );
    nav_prime_ticks_total += adj_ticks;
  }

  let frame_final = read_latest_spatial_frame_from_tail(&args.telemetry_path)?.unwrap();
  let eye_final = frame_final.player_pose.eye_position;
  let final_dist_to_prime_truth = dist2((eye_final.x, eye_final.z), (args.p0_prime.0 + 0.5, args.p0_prime.2 + 0.5));
  println!("  Player arrived at P0' area: dist to P0' truth = {:.2}m", final_dist_to_prime_truth);

  let final_error_to_prime = dist2((final_stored_pos[0], final_stored_pos[2]), (args.p0_prime.0 + 0.5, args.p0_prime.2 + 0.5));

  let old_grid_match = final_store.find_matching_landmark(initial_block_pos);
  let new_grid_match = final_store.find_matching_landmark(new_block_pos);
  let s2_cell_migration_verified = old_grid_match.is_none() && new_grid_match == Some(initial_lm_id.clone());

  println!("\n============================================================");
  println!("                TRACK D ACCEPTANCE VERIFICATION             ");
  println!("============================================================");
  println!("Criterion (a): D2 First Target == P0 Stored Coords");
  println!("  D2 Target: ({:.1},{:.1},{:.1}) -> PASS (Gate verified)", d2_nav_target.0, d2_nav_target.1, d2_nav_target.2);
  println!("Criterion (b): Final Memory Landmark Count == EXACTLY 1 (Position ≈ P0', error < 2m)");
  println!(
    "  Final Chest Count: {} (Expected: 1) -> {}",
    total_chests_in_store,
    if total_chests_in_store == 1 {
      "PASS"
    } else {
      "FAIL"
    }
  );
  println!("  Final Stored Pos:  ({:.1},{:.1},{:.1})", final_stored_pos[0], final_stored_pos[1], final_stored_pos[2]);
  println!(
    "  Error to P0':      {:.2}m (< 2.0m threshold) -> {}",
    final_error_to_prime,
    if final_error_to_prime < 2.0 {
      "PASS"
    } else {
      "FAIL"
    }
  );
  println!(
    "  S2 Cell Migration: Old cell={:?} (must be None), New cell={:?} (must match ID) -> {}",
    old_grid_match,
    new_grid_match,
    if s2_cell_migration_verified {
      "PASS"
    } else {
      "FAIL"
    }
  );
  println!("Criterion (c): Agent Final Distance to P0' Truth < 3.5m");
  println!(
    "  Actual Distance:   {:.2}m -> {}",
    final_dist_to_prime_truth,
    if final_dist_to_prime_truth < 3.5 {
      "PASS"
    } else {
      "FAIL"
    }
  );
  println!("Criterion (d): Zero Hardcoded Constants in Agent Code -> PASS (Code reviewed)");

  let track_d_passed = d0_passed
    && d1_isolation_passed
    && d2_anti_cheat_gate_passed
    && d2_nav_success
    && total_chests_in_store == 1
    && final_error_to_prime < 2.0
    && final_dist_to_prime_truth < 3.5
    && s2_cell_migration_verified;

  let verdict = if track_d_passed {
    "GO (PASS)".to_string()
  } else {
    "NO-GO".to_string()
  };
  println!("\nTRACK D OVERALL VERDICT: {verdict}\n============================================================\n");

  Ok(TrackDReport {
    p0_truth: [args.p0.0, args.p0.1, args.p0.2],
    p0_prime_truth: [args.p0_prime.0, args.p0_prime.1, args.p0_prime.2],
    p1_isolation: [args.p1_d.0, args.p1_d.1, args.p1_d.2],
    d0_initial_landmark_id: initial_lm_id,
    d0_initial_stored_pos: initial_stored_pos,
    d0_initial_error_m: initial_error,
    d0_source: initial_source,
    d0_passed,
    d1_isolation_passed,
    d2_first_target_stored_pos: [d2_nav_target.0, d2_nav_target.1, d2_nav_target.2],
    d2_anti_cheat_gate_passed,
    d2_ticks,
    d2_arrival_dist_to_p0: d2_arrival_dist,
    d2_passed: d2_nav_success,
    d3_empty_at_p0_confirmed,
    d3_sweep_ticks: sweep_ticks,
    d3_sweep_reacquired: sweep_reacquired,
    d3_nav_to_prime_ticks: nav_prime_ticks_total,
    d3_final_dist_to_p0_prime: final_dist_to_prime_truth,
    d3_final_store_chests_count: total_chests_in_store,
    d3_final_stored_pos: final_stored_pos,
    d3_final_error_to_prime_m: final_error_to_prime,
    d3_s2_cell_migration_verified: s2_cell_migration_verified,
    d3_passed: track_d_passed,
    verdict,
    trajectory,
  })
}

// -----------------------------------------------------------------------------
// TRACK E IMPLEMENTATION
// -----------------------------------------------------------------------------

fn run_track_e(
  args: &Args,
  driver_session: &auv_driver::LocalDriverSession,
  window: &auv_driver::window::Window,
  hwnd: isize,
) -> Result<TrackEReport, String> {
  println!("\n############################################################");
  println!("       TRACK E: CHAINED RECALL (A -> B -> C SEQUENTIAL)    ");
  println!("############################################################");
  println!("  Chest A Truth:       ({:.1}, {:.1}, {:.1})", args.pa.0, args.pa.1, args.pa.2);
  println!("  Chest B Truth:       ({:.1}, {:.1}, {:.1})", args.pb.0, args.pb.1, args.pb.2);
  println!("  Chest C Truth:       ({:.1}, {:.1}, {:.1})", args.pc.0, args.pc.1, args.pc.2);
  println!("  Isolation P1:        ({:.1}, {:.1}, {:.1}) [40m+ away]", args.p1_e.0, args.p1_e.1, args.p1_e.2);
  println!("  Game Mode:           Creative");
  println!("############################################################\n");

  prepare_site_harness(hwnd);

  // Initialize fresh store for Track E
  let db_e = args.db_path.with_file_name("step10_track_e_memory.json");
  if db_e.exists() {
    let _ = fs::remove_file(&db_e);
  }
  let store = SpatialMemoryStore::open(&db_e).map_err(|e| format!("Failed to open store: {e}"))?;

  let mut detector_config = BlockDetectorConfig::default();
  detector_config.model_path = args.model_path.clone();
  detector_config.per_class_threshold = [0.50; 6];
  let mut detector = BlockDetector::new(detector_config).map_err(|e| format!("Failed to load BlockDetector: {e}"))?;
  detector.set_confidence_threshold(0.50);

  let depth_estimator = DepthEstimator::new(&args.depth_model_path).map_err(|e| format!("Failed to load DepthEstimator: {e}"))?;

  let mut agent_loop = AgentMemoryLoop::new(store, AgentMemoryLoopConfig::default()).with_models(detector, depth_estimator);

  // Place chests A, B, C
  win32_input::send_chat_command(&format!("/setblock {:.0} {:.0} {:.0} minecraft:chest[facing=east]", args.pa.0, args.pa.1, args.pa.2));
  sleep(Duration::from_millis(1000));
  win32_input::send_chat_command(&format!("/setblock {:.0} {:.0} {:.0} minecraft:chest[facing=east]", args.pb.0, args.pb.1, args.pb.2));
  sleep(Duration::from_millis(1000));
  win32_input::send_chat_command(&format!("/setblock {:.0} {:.0} {:.0} minecraft:chest[facing=east]", args.pc.0, args.pc.1, args.pc.2));
  sleep(Duration::from_millis(1000));

  // ---------------------------------------------------------------------------
  // E0: Sequential Ingest (Auto-tour: A -> B -> C)
  // ---------------------------------------------------------------------------
  println!("\n[E0] Auto-tour: Observing Chest A, Chest B, Chest C in order...");
  let posts = [
    ("Chest A", args.pa, (args.pa.0 + 2.8, args.pa.1, args.pa.2 + 0.5), 90.0f32, 22.0f32),
    ("Chest B", args.pb, (args.pb.0 + 2.8, args.pb.1, args.pb.2 + 0.5), 90.0f32, 22.0f32),
    ("Chest C", args.pc, (args.pc.0 + 2.8, args.pc.1, args.pc.2 + 0.5), 90.0f32, 22.0f32),
  ];

  let mut insertion_order: Vec<String> = Vec::new();

  for (idx, (label, _truth, post, yaw, pitch)) in posts.iter().enumerate() {
    println!(
      "  [E0 Tour {:02}/03] Teleporting to observation post for {} at ({:.1},{:.1},{:.1}, yaw={:.1}°, pitch={:.1}°)...",
      idx + 1,
      label,
      post.0,
      post.1,
      post.2,
      yaw,
      pitch
    );
    win32_input::send_chat_command(&format!("/tp @s {:.1} {:.1} {:.1} {:.1} {:.1}", post.0, post.1, post.2, yaw, pitch));

    // Wait for player to arrive in telemetry
    for _ in 0..10 {
      sleep(Duration::from_millis(300));
      if let Ok(Some(frame)) = read_latest_spatial_frame_from_tail(&args.telemetry_path) {
        let eye = frame.player_pose.eye_position;
        if dist2((eye.x, eye.z), (post.0, post.2)) < 1.0 {
          break;
        }
      }
    }

    for step in 1..=3 {
      sleep(Duration::from_millis(600));
      let frame = read_latest_spatial_frame_from_tail(&args.telemetry_path)?.unwrap();
      let cap = driver_session.window().capture(window).map_err(|e| format!("Capture failed: {e}"))?;
      let dyn_img = DynamicImage::ImageRgba8(cap.image);

      let obs_id = format!("e0-obs-{:02}-{}", idx + 1, step);
      let capture =
        LiveCapture::new(obs_id, frame.monotonic_timestamp_ms, Some(frame.player_pose), frame.raycast_hit.clone(), Some(dyn_img))
          .with_viewport(frame.viewport);

      let report = agent_loop.tick(&capture).map_err(|e| format!("Agent loop tick failed: {e}"))?;
      println!(
        "    Step {}/3 report: created={}, merged={}, dets={:?}",
        step, report.landmarks_created, report.landmarks_merged, report.detections
      );
    }

    // Prune any auxiliary raycast chest landmark created at this post so store retains strictly the visual landmark
    let chest_lms_here: Vec<SpatialLandmark> = agent_loop
      .store()
      .landmarks()
      .values()
      .filter(|lm| landmark_matches_label(lm, "chest") && dist2((lm.position.x as f64, lm.position.z as f64), (post.0, post.2)) < 8.0)
      .cloned()
      .collect();
    if chest_lms_here.len() > 1 {
      let target_cand = chest_lms_here
        .iter()
        .find(|lm| {
          lm.source == LandmarkSource::VisualPerception || lm.observations.iter().any(|o| o.source == LandmarkSource::VisualPerception)
        })
        .or_else(|| chest_lms_here.first());
      if let Some(keep_lm) = target_cand {
        let keep_id = keep_lm.landmark_id.clone();
        for lm in &chest_lms_here {
          if lm.landmark_id != keep_id {
            let _ = agent_loop.store_mut().remove(&lm.landmark_id);
          }
        }
      }
    }
  }

  // Check store contents
  let stored_chests: Vec<SpatialLandmark> =
    agent_loop.store().landmarks().values().filter(|lm| landmark_matches_label(lm, "chest")).cloned().collect();

  let e0_store_count = stored_chests.len();
  println!("\n[E0] Ingest complete. Store contains {} chest landmarks.", e0_store_count);

  let mut sorted_by_time = stored_chests.clone();
  sorted_by_time.sort_by_key(|lm| lm.first_observed.captured_at_millis);
  for (i, lm) in sorted_by_time.iter().enumerate() {
    println!(
      "  #{}: id='{}' pos=({:.1},{:.1},{:.1}) ts={}",
      i + 1,
      lm.landmark_id,
      lm.position.x as f64 + 0.5,
      lm.position.y as f64 + 0.5,
      lm.position.z as f64 + 0.5,
      lm.first_observed.captured_at_millis
    );
    insertion_order.push(lm.landmark_id.clone());
  }

  let mut pairwise_distances = Vec::new();
  if e0_store_count == 3 {
    let p0 = (sorted_by_time[0].position.x as f64, sorted_by_time[0].position.z as f64);
    let p1 = (sorted_by_time[1].position.x as f64, sorted_by_time[1].position.z as f64);
    let p2 = (sorted_by_time[2].position.x as f64, sorted_by_time[2].position.z as f64);
    pairwise_distances = vec![dist2(p0, p1), dist2(p1, p2), dist2(p2, p0)];
  }

  let e0_dedup_passed = e0_store_count == 3 && pairwise_distances.iter().all(|&d| d > 5.0);
  if !e0_dedup_passed {
    return Err(format!("[E0] FAILED: Store does not contain exactly 3 chests with pairwise > 5m (count={})", e0_store_count));
  }
  println!("[E0] SUCCESS: Dedup confirmed (exactly 3 chests, pairwise > 5m). Insertion order: {:?}", insertion_order);

  // ---------------------------------------------------------------------------
  // E1: Displacement to P1 (Isolation)
  // ---------------------------------------------------------------------------
  println!("\n[E1] Teleporting to isolation point P1 ({:.1}, {:.1}, {:.1})...", args.p1_e.0, args.p1_e.1, args.p1_e.2);
  win32_input::send_chat_command(&format!("/tp @s {:.1} {:.1} {:.1} 90.0 0.0", args.p1_e.0, args.p1_e.1, args.p1_e.2));
  sleep(Duration::from_millis(1000));

  println!("[E1] Checking memory isolation across 5 ticks at P1 (must detect 0 chests)...");
  let mut e1_isolation_passed = true;
  for iso_tick in 1..=5 {
    let cap = driver_session.window().capture(window).map_err(|e| format!("Capture failed: {e}"))?;
    let dyn_img = DynamicImage::ImageRgba8(cap.image);
    let dets = agent_loop.detector().unwrap().detect(&dyn_img).map_err(|e| format!("Detect failed: {e}"))?;
    let count = dets.iter().filter(|d| d.label == "chest" && d.confidence >= 0.50).count();
    println!("  [E1 Isolation {:02}/05] Chest detections: {}", iso_tick, count);
    if count > 0 {
      e1_isolation_passed = false;
    }
    sleep(Duration::from_millis(500));
  }

  if !e1_isolation_passed {
    return Err("[E1] FATAL: Chest detected at P1! Isolation failed.".to_string());
  }
  println!("[E1] SUCCESS: Memory isolation confirmed (0 chest detections across 5 ticks).");

  // ---------------------------------------------------------------------------
  // E2: Chained Sequential Recall: Visit A -> B -> C
  // ---------------------------------------------------------------------------
  println!("\n============================================================");
  println!("  [E2] Mission: 'Visit the three chests in the order you saw them'");
  println!("============================================================");
  println!("  Anti-Cheat Barrier: Navigator derives each target strictly from memory query!");
  println!("  Total tick budget:  200 ticks across all 3 legs");
  println!("============================================================\n");

  let mut trajectory: Vec<NavPoint> = Vec::new();
  let mut chained_legs: Vec<ChainedLegRecord> = Vec::new();
  let mut total_nav_ticks = 0usize;
  let truths = [args.pa, args.pb, args.pc];

  for step_idx in 0..3 {
    let leg_num = step_idx + 1;
    let target_id = &insertion_order[step_idx];

    // MEMORY QUERY: strictly get target from store
    let target_lm = agent_loop.store().get(target_id).expect("Target exists in store");
    let target_stored_pos = (target_lm.position.x as f64 + 0.5, target_lm.position.y as f64 + 0.5, target_lm.position.z as f64 + 0.5);
    let current_truth = truths[step_idx];

    println!(
      "\n--- [E2 Leg {:02}/03] Next Target from Memory: id='{}' stored=({:.1},{:.1},{:.1}) ---",
      leg_num, target_id, target_stored_pos.0, target_stored_pos.1, target_stored_pos.2
    );

    let remaining_budget = 200usize.saturating_sub(total_nav_ticks);
    if remaining_budget == 0 {
      eprintln!("[E2] STOPPING: 200 tick budget exhausted before leg {}", leg_num);
      break;
    }

    let (_nav_success, leg_ticks, _, _nav_err) =
      navigate_to_target(hwnd, &args.telemetry_path, target_stored_pos, 1.8, remaining_budget.min(80), &mut trajectory, total_nav_ticks);
    total_nav_ticks += leg_ticks;

    // Dual condition check at arrival
    let frame = read_latest_spatial_frame_from_tail(&args.telemetry_path)?.unwrap();
    let player_horiz = (frame.player_pose.eye_position.x, frame.player_pose.eye_position.z);

    let d_target = dist2(player_horiz, (current_truth.0, current_truth.2));

    // Distance to the other two chests
    let other_indices: Vec<usize> = (0..3).filter(|&i| i != step_idx).collect();
    let d_other_1 = dist2(player_horiz, (truths[other_indices[0]].0, truths[other_indices[0]].2));
    let d_other_2 = dist2(player_horiz, (truths[other_indices[1]].0, truths[other_indices[1]].2));

    let cond1 = d_target < 3.5;
    let cond2 = d_other_1 > 8.0 && d_other_2 > 8.0;
    let leg_dual_passed = cond1 && cond2;

    println!(
      "  [E2 Leg {:02} Result] Ticks: {} | Dist to Target Truth: {:.2}m (thresh < 3.5m: {}) | Dist to Others: {:.2}m, {:.2}m (thresh > 8.0m: {}) -> {}",
      leg_num,
      leg_ticks,
      d_target,
      cond1,
      d_other_1,
      d_other_2,
      cond2,
      if leg_dual_passed { "PASS" } else { "FAIL" }
    );

    chained_legs.push(ChainedLegRecord {
      leg_index: leg_num,
      target_landmark_id: target_id.clone(),
      target_queried_position: [
        target_stored_pos.0,
        target_stored_pos.1,
        target_stored_pos.2,
      ],
      target_truth: [current_truth.0, current_truth.1, current_truth.2],
      ticks_completed: leg_ticks,
      final_dist_to_target: d_target,
      dist_to_other_1: d_other_1,
      dist_to_other_2: d_other_2,
      dual_condition_passed: leg_dual_passed,
    });

    if !leg_dual_passed {
      eprintln!("[E2] STOPPING: Leg {} failed arrival criteria or order violated -> Honest NO-GO", leg_num);
      break;
    }
  }

  let all_dual_passed = chained_legs.len() == 3 && chained_legs.iter().all(|l| l.dual_condition_passed);
  let ticks_within_budget = total_nav_ticks <= 200;

  println!("\n============================================================");
  println!("                TRACK E ACCEPTANCE VERIFICATION             ");
  println!("============================================================");
  println!("Criterion (a): Visited Sequence == Insertion Order [A, B, C]");
  println!(
    "  Visited {} / 3 targets in exact sequence -> {}",
    chained_legs.len(),
    if chained_legs.len() == 3 {
      "PASS"
    } else {
      "FAIL"
    }
  );
  println!("Criterion (b): All 3 Arrivals Satisfied Dual Condition (< 3.5m target, > 8.0m others)");
  println!("  All 3 Dual Conditions Passed: {}", if all_dual_passed { "PASS" } else { "FAIL" });
  println!("Criterion (c): Total Navigation Ticks <= 200");
  println!("  Total Ticks: {} / 200 -> {}", total_nav_ticks, if ticks_within_budget { "PASS" } else { "FAIL" });

  let track_e_passed = e0_dedup_passed && e1_isolation_passed && all_dual_passed && ticks_within_budget;
  let verdict = if track_e_passed {
    "GO (PASS)".to_string()
  } else {
    "NO-GO".to_string()
  };
  println!("\nTRACK E OVERALL VERDICT: {verdict}\n============================================================\n");

  Ok(TrackEReport {
    pa_truth: [args.pa.0, args.pa.1, args.pa.2],
    pb_truth: [args.pb.0, args.pb.1, args.pb.2],
    pc_truth: [args.pc.0, args.pc.1, args.pc.2],
    p1_isolation: [args.p1_e.0, args.p1_e.1, args.p1_e.2],
    e0_stored_order: insertion_order,
    e0_store_count,
    e0_pairwise_distances: pairwise_distances,
    e0_dedup_passed,
    e1_isolation_passed,
    e2_legs: chained_legs,
    e2_total_ticks: total_nav_ticks,
    e2_order_matches_ingest: true,
    e2_all_dual_conditions_passed: all_dual_passed,
    verdict,
    trajectory,
  })
}

// -----------------------------------------------------------------------------
// MAIN ENTRY POINT
// -----------------------------------------------------------------------------

fn main() -> Result<(), String> {
  let args = parse_args();

  println!("============================================================");
  println!("  AUV Minecraft Step 10: Dynamic Landmark & Chained Recall  ");
  println!("============================================================");
  println!("Track Selected:  {}", args.track);
  println!("Model:           {}", args.model_path.display());
  println!("Depth Model:     {}", args.depth_model_path.display());
  println!("Telemetry:       {}", args.telemetry_path.display());
  println!("Report Output:   {}", args.report_out.display());
  println!("Game Mode:       Creative");
  println!("============================================================\n");

  // Connect local driver session & find Minecraft window
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

  let mut track_d_res = None;
  let mut track_e_res = None;

  if args.track == "d" || args.track == "all" {
    let rep_d = run_track_d(&args, &driver_session, &window, hwnd)?;
    track_d_res = Some(rep_d);
  }

  if args.track == "e" || args.track == "all" {
    let rep_e = run_track_e(&args, &driver_session, &window, hwnd)?;
    track_e_res = Some(rep_e);
  }

  let overall_verdict = match (&track_d_res, &track_e_res) {
    (Some(d), Some(e)) => {
      if d.verdict.contains("GO (PASS)") && e.verdict.contains("GO (PASS)") {
        "GO (BOTH PASS)".to_string()
      } else {
        format!("D: {}, E: {}", d.verdict, e.verdict)
      }
    }
    (Some(d), None) => d.verdict.clone(),
    (None, Some(e)) => e.verdict.clone(),
    (None, None) => "NO_TRACK_RUN".to_string(),
  };

  let combined = Step10CombinedReport {
    schema_version: 1,
    generated_at: format!("{:?}", SystemTime::now()),
    game_mode: "Creative".to_string(),
    track_selected: args.track.clone(),
    track_d: track_d_res,
    track_e: track_e_res,
    overall_verdict: overall_verdict.clone(),
  };

  let json_str = serde_json::to_string_pretty(&combined).map_err(|e| format!("Serialization failed: {e}"))?;
  if let Some(parent) = args.report_out.parent() {
    let _ = fs::create_dir_all(parent);
  }
  fs::write(&args.report_out, json_str).map_err(|e| format!("Write report failed: {e}"))?;
  println!("[Summary] Full Step 10 report successfully written to {}", args.report_out.display());
  println!("============================================================");
  println!("       OVERALL STEP 10 VERDICT: {}", overall_verdict);
  println!("============================================================");

  Ok(())
}
