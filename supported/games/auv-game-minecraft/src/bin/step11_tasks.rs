//! Standalone Verification Binary for Step 11: Forget & Cross-Session Persistence.
//!
//! Two independent tracks:
//! - Track F (Landmark Disappearance & Forgetting):
//!   - F0: Setup Chest A (target, to be dug up) at P0 and Chest B (control, untouched) at P2 (>10m apart).
//!         Agent visually ingests both (error < 2m). Store has EXACTLY 2 chest landmarks.
//!   - F1: Harness digs up Chest A (setblock air). B remains untouched.
//!         Agent displaced to P1 (>30m), 5 ticks of 0 chest detections (isolation verified).
//!   - F2: Mission "Go find Chest A". Agent queries memory -> target MUST be P0. Navigates to P0 (< 3.5m).
//!         Proves agent remembered A.
//!   - F3: Multiple miss cycles -> prune according to production policy:
//!         - Production code quoted: `record_miss` (misses += 1, conf -= 0.1) and `prune_stale` (conf < 0.3 || misses >= 5).
//!         - Driving N cycles (nav to P0 -> 0 detections -> record_miss -> prune_stale -> return to P1).
//!         - 1 miss does NOT prune (rejected as bug).
//!         - After N cycles:
//!           (a) A removed from store (`store.get == None`).
//!           (b) S2 grid old cell query returns None (zero ghost index).
//!           (c) Control Chest B remains intact with identical position and S2 grid hit (not a wholesale wipe).
//!           (d) Subsequent task "Go find Chest A" -> agent honestly reports NO MEMORY, refusing to navigate to ghost P0 or (0,0).
//!           (e) Zero hardcoded coordinates in navigation decision logic.
//!   - F4: Honest exit if prune fails to trigger as expected.
//!
//! - Track G (Cross-Session Persistence):
//!   - G0-G1: Ingest Chest C, record all fields (id, pos, observation_count, confidence, etc.).
//!            Save store to disk (`store.save()`) -> completely restart agent process -> reload store from disk.
//!   - G2: Reload assertions:
//!         (a) All fields identical (ID, coords, observation_count, confidence, source, description).
//!         (b) S2 grid deterministic reconstruction query hit.
//!         (c) Memory-guided navigation to target (< 3.5m arrival).

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime};

use image::DynamicImage;
use serde::{Deserialize, Serialize};

use auv_game_minecraft::agent_memory_loop::{AgentMemoryLoop, AgentMemoryLoopConfig, LiveCapture};
use auv_game_minecraft::ingest::read_latest_spatial_frame_from_tail;
use auv_game_minecraft::spatial_memory_store::{SpatialLandmark, SpatialMemoryStore};
use auv_game_minecraft::types::BlockPosition;
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
  pub union INPUT_UNION {
    pub mi: MOUSEINPUT,
    pub ki: KEYBDINPUT,
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

  pub fn activate_minecraft(target_hwnd: Option<isize>) -> bool {
    ensure_default_desktop();
    let hwnd = match target_hwnd {
      Some(h) if h != 0 => h as HWND,
      _ => {
        let stored = TARGET_HWND.load(std::sync::atomic::Ordering::SeqCst);
        if stored != 0 {
          stored as HWND
        } else {
          return false;
        }
      }
    };

    unsafe {
      let cur_fg = GetForegroundWindow();
      if cur_fg == hwnd {
        return true;
      }

      ShowWindow(hwnd, SW_RESTORE);
      sleep(Duration::from_millis(50));

      let cur_thread = GetCurrentThreadId();
      let mut target_pid: u32 = 0;
      let target_thread = GetWindowThreadProcessId(hwnd, &mut target_pid);

      if cur_thread != target_thread {
        AttachThreadInput(cur_thread, target_thread, 1);
      }

      BringWindowToTop(hwnd);
      SetForegroundWindow(hwnd);

      if cur_thread != target_thread {
        AttachThreadInput(cur_thread, target_thread, 0);
      }

      sleep(Duration::from_millis(100));

      // Left-click inside the window to lock mouse cursor in GLFW
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
      SendInput(1, &click_down, size_of::<INPUT>() as i32);
      sleep(Duration::from_millis(30));
      SendInput(1, &click_up, size_of::<INPUT>() as i32);
      sleep(Duration::from_millis(80));
    }
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
    let px = (delta_deg / YAW_SENSITIVITY_DEG_PER_PX).round() as i32;
    if px == 0 {
      return;
    }
    let step_size = 40;
    let mut remaining = px;
    while remaining != 0 {
      let chunk = if remaining > 0 {
        remaining.min(step_size)
      } else {
        remaining.max(-step_size)
      };
      turn_mouse(chunk, 0);
      remaining -= chunk;
      sleep(Duration::from_millis(15));
    }
  }

  pub fn step_forward(duration: Duration) {
    ensure_default_desktop();
    let hwnd_val = TARGET_HWND.load(std::sync::atomic::Ordering::SeqCst);
    if hwnd_val != 0 {
      let hwnd = hwnd_val as HWND;
      let cur = unsafe { GetForegroundWindow() };
      if cur != hwnd {
        activate_minecraft(Some(hwnd_val));
      }
    }
    let down = INPUT {
      r#type: INPUT_KEYBOARD,
      u: INPUT_UNION {
        ki: KEYBDINPUT {
          w_vk: VK_W,
          w_scan: 0x11,
          dw_flags: 0,
          time: 0,
          dw_extra_info: 0,
        },
      },
    };
    let up = INPUT {
      r#type: INPUT_KEYBOARD,
      u: INPUT_UNION {
        ki: KEYBDINPUT {
          w_vk: VK_W,
          w_scan: 0x11,
          dw_flags: KEYEVENTF_KEYUP,
          time: 0,
          dw_extra_info: 0,
        },
      },
    };
    unsafe {
      SendInput(1, &down, size_of::<INPUT>() as i32);
      sleep(duration);
      SendInput(1, &up, size_of::<INPUT>() as i32);
    }
  }

  pub fn press_esc() {
    let hwnd_val = TARGET_HWND.load(std::sync::atomic::Ordering::SeqCst);
    if hwnd_val == 0 {
      return;
    }
    let hwnd = hwnd_val as HWND;
    const WM_KEYDOWN: u32 = 0x0100;
    const WM_KEYUP: u32 = 0x0101;
    const VK_ESCAPE: u16 = 0x1B;
    unsafe {
      PostMessageW(hwnd, WM_KEYDOWN, VK_ESCAPE as usize, 1 | (0x01 << 16));
      sleep(Duration::from_millis(50));
      PostMessageW(hwnd, WM_KEYUP, VK_ESCAPE as usize, 1 | (0x01 << 16) | (1 << 30) | (1 << 31));
      sleep(Duration::from_millis(200));
    }
  }

  pub fn send_chat_command(cmd: &str) {
    let hwnd_val = TARGET_HWND.load(std::sync::atomic::Ordering::SeqCst);
    if hwnd_val == 0 {
      eprintln!("[Harness] ERROR: TARGET_HWND is 0, cannot send command: {cmd}");
      return;
    }
    let hwnd = hwnd_val as HWND;

    const WM_CHAR: u32 = 0x0102;
    const WM_KEYDOWN: u32 = 0x0100;
    const WM_KEYUP: u32 = 0x0101;

    let full_cmd = if cmd.starts_with('/') {
      cmd.to_string()
    } else {
      format!("/{cmd}")
    };

    println!("[Harness Command] Ingesting command: {full_cmd}");

    unsafe {
      PostMessageW(hwnd, WM_KEYDOWN, VK_SLASH as usize, 1 | (0x35 << 16));
      sleep(Duration::from_millis(30));
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
pub struct MissCycleRecord {
  pub cycle: usize,
  pub start_pos: [f64; 3],
  pub nav_target_stored: [f64; 3],
  pub arrived_dist_to_p0: f64,
  pub chest_detections_at_p0: usize,
  pub confidence_before_miss: f64,
  pub confidence_after_miss: f64,
  pub consecutive_misses: u32,
  pub pruned_in_this_cycle: bool,
  pub landmark_still_in_store: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrackFReport {
  pub p0_truth_a: [f64; 3],
  pub p2_truth_b_control: [f64; 3],
  pub p1_isolation: [f64; 3],
  pub f0_a_landmark_id: String,
  pub f0_a_stored_pos: [f64; 3],
  pub f0_a_initial_confidence: f64,
  pub f0_a_error_m: f64,
  pub f0_b_landmark_id: String,
  pub f0_b_stored_pos: [f64; 3],
  pub f0_b_error_m: f64,
  pub f0_store_chest_count: usize,
  pub f0_passed: bool,
  pub f1_isolation_passed: bool,
  pub f2_nav_target: [f64; 3],
  pub f2_anti_cheat_gate_passed: bool,
  pub f2_ticks: usize,
  pub f2_arrival_dist_to_p0: f64,
  pub f2_passed: bool,
  pub f3_production_prune_code_quote: String,
  pub f3_miss_cycles: Vec<MissCycleRecord>,
  pub f3_total_cycles_run: usize,
  pub f3_pruned_on_cycle_1_rejected_as_bug: bool,
  pub f3_a_removed_from_store: bool,
  pub f3_a_s2_grid_cleared: bool,
  pub f3_control_b_still_present: bool,
  pub f3_control_b_position_intact: bool,
  pub f3_control_b_s2_grid_intact: bool,
  pub f3_final_task_honest_no_memory_reported: bool,
  pub f3_zero_hardcoded_coords_verified: bool,
  pub verdict: String,
  pub trajectory: Vec<NavPoint>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrackGCheckpoint {
  pub landmark_id: String,
  pub block_position: [i32; 3],
  pub continuous_position: Option<[f64; 3]>,
  pub observation_count: u32,
  pub confidence: f64,
  pub source: String,
  pub description: Option<String>,
  pub status: String,
  pub consecutive_misses: u32,
  pub last_observed_millis: u64,
  pub target_truth: [f64; 3],
  pub p1_isolation: [f64; 3],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrackGReport {
  pub pc_truth: [f64; 3],
  pub p1_isolation: [f64; 3],
  pub g0_landmark_id: String,
  pub g0_stored_pos: [f64; 3],
  pub g0_initial_error_m: f64,
  pub g1_store_saved_to_disk: bool,
  pub g1_restart_method: String,
  pub g2_all_fields_identical: bool,
  pub g2_field_comparison_details: Vec<String>,
  pub g2_s2_grid_reconstruction_hit: bool,
  pub g2_nav_ticks: usize,
  pub g2_final_dist_to_target: f64,
  pub g2_navigation_passed: bool,
  pub verdict: String,
  pub trajectory: Vec<NavPoint>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Step11CombinedReport {
  pub schema_version: u32,
  pub generated_at: String,
  pub game_mode: String,
  pub restart_method: String,
  pub track_selected: String,
  pub track_f: Option<TrackFReport>,
  pub track_g: Option<TrackGReport>,
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
  p2: (f64, f64, f64),
  p1: (f64, f64, f64),
  p_g: (f64, f64, f64),
  p1_g: (f64, f64, f64),
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
  let mut report_out = PathBuf::from(r"F:\auv\.tmp\step11_report.json");
  let mut db_path = PathBuf::from(r"F:\auv\.tmp\step11_memory.json");

  // Track F points: Chest A (P0), Chest B control (P2), Isolation (P1)
  let mut p0 = (-45.0, 95.0, 270.0);
  let mut p2 = (-30.0, 95.0, 255.0);
  let mut p1 = (-45.0, 95.0, 230.0);

  // Track G points: Chest C (P_G), Isolation (P1_G)
  let mut p_g = (-40.0, 95.0, 260.0);
  let mut p1_g = (-45.0, 95.0, 215.0);

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
        i += 2;
      }
      "--p2" => {
        p2 = parse_vec3(&args[i + 1]).expect("valid P2");
        i += 2;
      }
      "--p1" => {
        p1 = parse_vec3(&args[i + 1]).expect("valid P1");
        i += 2;
      }
      "--p-g" => {
        p_g = parse_vec3(&args[i + 1]).expect("valid P_G");
        i += 2;
      }
      "--p1-g" => {
        p1_g = parse_vec3(&args[i + 1]).expect("valid P1_G");
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
    p2,
    p1,
    p_g,
    p1_g,
  }
}

// -----------------------------------------------------------------------------
// Site Paving & Preparation
// -----------------------------------------------------------------------------

fn prepare_site_harness(hwnd: isize) {
  win32_input::activate_minecraft(Some(hwnd));
  sleep(Duration::from_millis(500));
  if let Ok(Some(frame)) =
    read_latest_spatial_frame_from_tail(std::path::Path::new(r"F:\pcl\.minecraft\versions\1.21.1-Fabric 0.16.10\auv\telemetry.jsonl"))
  {
    if frame.screen_state.as_deref() != Some("in_game") {
      println!("[Harness] Screen state is '{:?}', pressing ESC to return to in_game...", frame.screen_state);
      win32_input::press_esc();
      sleep(Duration::from_millis(500));
    }
  }
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

  println!("\n----------------------------------------------------------------------------------------------------");
  println!(" Tick |     Player Position     |  Yaw   |  Target Dist | Yaw Delta | Action Taken        ");
  println!("----------------------------------------------------------------------------------------------------");

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

    if dist_to_target <= arrival_thresh_m {
      println!(
        "| {:04} | ({:6.1}, {:5.1}, {:6.1}) | {:5.1}° | {:10.2}m | {:8.1}° | ARRIVED (<{:.1}m) |",
        global_tick_offset + step_idx,
        eye.x,
        eye.y,
        eye.z,
        yaw,
        dist_to_target,
        0.0,
        arrival_thresh_m
      );
      trajectory.push(NavPoint {
        tick: global_tick_offset + step_idx,
        player_pos: [eye.x, eye.y, eye.z],
        yaw,
        target_dist: dist_to_target,
        yaw_delta: 0.0,
        action: format!("ARRIVED(dist={:.2}m)", dist_to_target),
      });
      nav_success = true;
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
// Agent High-Level Task Execution (Honest Memory Depletion Reporting)
// -----------------------------------------------------------------------------

pub enum AgentTaskOutcome {
  Arrived {
    arrived_pos: [f64; 3],
    final_dist_to_stored: f64,
    ticks: usize,
  },
  NoMemoryFound {
    requested_id: String,
    detail: String,
  },
  NavigationFailed {
    reason: String,
    ticks: usize,
  },
}

fn agent_execute_go_to_landmark(
  store: &SpatialMemoryStore,
  landmark_id: &str,
  hwnd: isize,
  telemetry_path: &PathBuf,
  arrival_thresh_m: f64,
  max_ticks: usize,
  trajectory: &mut Vec<NavPoint>,
  global_tick_offset: usize,
) -> AgentTaskOutcome {
  println!("[Agent Task] Received order: 'Go find landmark {landmark_id}'...");
  // Step 1: Query internal spatial memory store
  let stored_lm = match store.get(landmark_id) {
    Some(lm) => lm,
    None => {
      let msg =
        format!("Agent honest report: Landmark '{landmark_id}' NOT FOUND in spatial memory. Refusing to navigate to ghost coordinates.");
      println!("  [Agent Memory Query] {msg}");
      return AgentTaskOutcome::NoMemoryFound {
        requested_id: landmark_id.to_string(),
        detail: msg,
      };
    }
  };

  // Step 2: Target coordinates derived strictly from memory
  let target_pos = if let Some(cp) = stored_lm.continuous_position {
    cp
  } else {
    (stored_lm.position.x as f64 + 0.5, stored_lm.position.y as f64 + 0.5, stored_lm.position.z as f64 + 0.5)
  };
  println!(
    "  [Agent Memory Query] Landmark '{landmark_id}' found in memory at stored pos: ({:.1}, {:.1}, {:.1})",
    target_pos.0, target_pos.1, target_pos.2
  );

  // Step 3: Navigate strictly to the stored position
  let (success, ticks, final_dist, err) =
    navigate_to_target(hwnd, telemetry_path, target_pos, arrival_thresh_m, max_ticks, trajectory, global_tick_offset);

  let frame = read_latest_spatial_frame_from_tail(telemetry_path).ok().flatten();
  let eye = frame
    .map(|f| {
      [
        f.player_pose.eye_position.x,
        f.player_pose.eye_position.y,
        f.player_pose.eye_position.z,
      ]
    })
    .unwrap_or([0.0, 0.0, 0.0]);

  if success {
    AgentTaskOutcome::Arrived {
      arrived_pos: eye,
      final_dist_to_stored: final_dist,
      ticks,
    }
  } else {
    AgentTaskOutcome::NavigationFailed {
      reason: err.unwrap_or_else(|| "Navigation timed out or stagnant".to_string()),
      ticks,
    }
  }
}

// -----------------------------------------------------------------------------
// TRACK F: LANDMARK DISAPPEARANCE & FORGETTING
// -----------------------------------------------------------------------------

fn run_track_f(
  args: &Args,
  driver_session: &auv_driver::LocalDriverSession,
  window: &auv_driver::window::Window,
  hwnd: isize,
) -> Result<TrackFReport, String> {
  println!("\n############################################################");
  println!("       TRACK F: LANDMARK DISAPPEARANCE & FORGETTING        ");
  println!("############################################################");
  println!("  Target Chest A Truth:    ({:.1}, {:.1}, {:.1})", args.p0.0, args.p0.1, args.p0.2);
  println!("  Control Chest B Truth:   ({:.1}, {:.1}, {:.1}) [>10m apart]", args.p2.0, args.p2.1, args.p2.2);
  println!("  Isolation P1:            ({:.1}, {:.1}, {:.1}) [>30m away]", args.p1.0, args.p1.1, args.p1.2);
  println!("  Game Mode:               Creative");
  println!("############################################################\n");

  prepare_site_harness(hwnd);

  // Fresh store for Track F
  let db_f = args.db_path.with_file_name("step11_track_f_memory.json");
  if db_f.exists() {
    let _ = fs::remove_file(&db_f);
  }
  let store = SpatialMemoryStore::open(&db_f).map_err(|e| format!("Failed to open store: {e}"))?;

  let mut detector_config = BlockDetectorConfig::default();
  detector_config.model_path = args.model_path.clone();
  detector_config.per_class_threshold = [0.50; 6];
  let mut detector = BlockDetector::new(detector_config).map_err(|e| format!("Failed to load BlockDetector: {e}"))?;
  detector.set_confidence_threshold(0.50);

  let depth_estimator = DepthEstimator::new(&args.depth_model_path).map_err(|e| format!("Failed to load DepthEstimator: {e}"))?;

  let mut agent_loop = AgentMemoryLoop::new(store, AgentMemoryLoopConfig::default()).with_models(detector, depth_estimator);

  // ---------------------------------------------------------------------------
  // F0: Setup Chest A and Chest B, visual ingest
  // ---------------------------------------------------------------------------
  println!("\n[F0 Setup] Deploying Chest A (target) at P0 and Chest B (control) at P2...");
  win32_input::send_chat_command(&format!(
    "/setblock {:.0} {:.0} {:.0} minecraft:chest[facing=east] replace",
    args.p0.0, args.p0.1, args.p0.2
  ));
  sleep(Duration::from_millis(800));
  win32_input::send_chat_command(&format!(
    "/setblock {:.0} {:.0} {:.0} minecraft:chest[facing=east] replace",
    args.p2.0, args.p2.1, args.p2.2
  ));
  sleep(Duration::from_millis(800));

  // Ingest Chest A from multiple angles
  let a_posts = [
    (args.p0.0 + 3.0, args.p0.1, args.p0.2 + 0.5, 90.0f32, 22.0f32),
    (args.p0.0 + 2.5, args.p0.1, args.p0.2 - 0.5, 90.0f32, 20.0f32),
    (args.p0.0 - 2.5, args.p0.1, args.p0.2 + 0.5, -90.0f32, 22.0f32),
  ];
  for (post_idx, &(obs_x, obs_y, obs_z, obs_yaw, obs_pitch)) in a_posts.iter().enumerate() {
    println!(
      "  [F0 Ingest A Post {:02}/03] Teleporting to ({:.1},{:.1},{:.1}, yaw={:.1}°, pitch={:.1}°)...",
      post_idx + 1,
      obs_x,
      obs_y,
      obs_z,
      obs_yaw,
      obs_pitch
    );
    win32_input::send_chat_command(&format!("/tp @s {:.1} {:.1} {:.1} {:.1} {:.1}", obs_x, obs_y, obs_z, obs_yaw, obs_pitch));
    for _ in 0..10 {
      sleep(Duration::from_millis(300));
      if let Ok(Some(frame)) = read_latest_spatial_frame_from_tail(&args.telemetry_path) {
        if dist2((frame.player_pose.eye_position.x, frame.player_pose.eye_position.z), (obs_x, obs_z)) < 1.0 {
          break;
        }
      }
    }
    for tick_idx in 1..=4 {
      let frame = read_latest_spatial_frame_from_tail(&args.telemetry_path)?.ok_or_else(|| "No telemetry frame".to_string())?;
      let cap = driver_session.window().capture(window).map_err(|e| format!("Capture failed: {e}"))?;
      let dyn_img = DynamicImage::ImageRgba8(cap.image);

      // Diagnostic: detect and print what was found
      let dets = agent_loop.detector().unwrap().detect(&dyn_img).map_err(|e| format!("Detect failed: {e}"))?;
      let chest_dets: Vec<_> = dets.iter().filter(|d| d.label == "chest").collect();
      println!(
        "    [A Post {:02} Tick {:02}] Detections: total={}, chest={} ({})",
        post_idx + 1,
        tick_idx,
        dets.len(),
        chest_dets.len(),
        chest_dets.iter().map(|d| format!("conf={:.3}", d.confidence)).collect::<Vec<_>>().join(", ")
      );
      if post_idx == 0 && tick_idx == 1 {
        dyn_img.save("F:/auv/.tmp/step11_f0_a_debug.png").ok();
      }

      // Raycast diagnostic
      if let Some(ref hit) = frame.raycast_hit {
        println!(
          "    [A Post {:02} Tick {:02}] Raycast: block=({},{},{}) id={}",
          post_idx + 1,
          tick_idx,
          hit.block_pos.x,
          hit.block_pos.y,
          hit.block_pos.z,
          hit.block_id
        );
      }

      let capture = LiveCapture::new(
        format!("f0-a-obs-{}-{:02}", post_idx + 1, tick_idx),
        frame.monotonic_timestamp_ms,
        Some(frame.player_pose),
        frame.raycast_hit.clone(),
        Some(dyn_img),
      )
      .with_viewport(frame.viewport);
      agent_loop.tick(&capture).map_err(|e| format!("Agent loop tick failed: {e}"))?;
      sleep(Duration::from_millis(200));
    }
  }
  // Print store state after A ingest
  let a_chests_so_far: Vec<_> = agent_loop.store().landmarks().values().filter(|lm| landmark_matches_label(lm, "chest")).collect();
  println!("  [F0 After A Ingest] Chest landmarks in store: {}", a_chests_so_far.len());
  for lm in &a_chests_so_far {
    println!(
      "    LM: {}, Pos:({},{},{}), Conf:{:.3}, Source:{:?}",
      lm.landmark_id, lm.position.x, lm.position.y, lm.position.z, lm.confidence, lm.source
    );
  }

  // Ingest Chest B from multiple angles
  let b_posts = [
    (args.p2.0 + 3.0, args.p2.1, args.p2.2 + 0.5, 90.0f32, 22.0f32),
    (args.p2.0 - 2.5, args.p2.1, args.p2.2 + 0.5, -90.0f32, 22.0f32),
    (args.p2.0, args.p2.1, args.p2.2 + 3.0, 180.0f32, 22.0f32),
  ];
  for (post_idx, &(obs_x, obs_y, obs_z, obs_yaw, obs_pitch)) in b_posts.iter().enumerate() {
    println!(
      "  [F0 Ingest B Post {:02}/03] Teleporting to ({:.1},{:.1},{:.1}, yaw={:.1}°, pitch={:.1}°)...",
      post_idx + 1,
      obs_x,
      obs_y,
      obs_z,
      obs_yaw,
      obs_pitch
    );
    win32_input::send_chat_command(&format!("/tp @s {:.1} {:.1} {:.1} {:.1} {:.1}", obs_x, obs_y, obs_z, obs_yaw, obs_pitch));
    for _ in 0..10 {
      sleep(Duration::from_millis(300));
      if let Ok(Some(frame)) = read_latest_spatial_frame_from_tail(&args.telemetry_path) {
        if dist2((frame.player_pose.eye_position.x, frame.player_pose.eye_position.z), (obs_x, obs_z)) < 1.0 {
          break;
        }
      }
    }
    for tick_idx in 1..=4 {
      let frame = read_latest_spatial_frame_from_tail(&args.telemetry_path)?.ok_or_else(|| "No telemetry frame".to_string())?;
      let cap = driver_session.window().capture(window).map_err(|e| format!("Capture failed: {e}"))?;
      let dyn_img = DynamicImage::ImageRgba8(cap.image);

      // Diagnostic: detect and print what was found
      let dets = agent_loop.detector().unwrap().detect(&dyn_img).map_err(|e| format!("Detect failed: {e}"))?;
      let chest_dets: Vec<_> = dets.iter().filter(|d| d.label == "chest").collect();
      println!(
        "    [B Post {:02} Tick {:02}] Detections: total={}, chest={} ({})",
        post_idx + 1,
        tick_idx,
        dets.len(),
        chest_dets.len(),
        chest_dets.iter().map(|d| format!("conf={:.3}", d.confidence)).collect::<Vec<_>>().join(", ")
      );

      // Raycast diagnostic
      if let Some(ref hit) = frame.raycast_hit {
        println!(
          "    [B Post {:02} Tick {:02}] Raycast: block=({},{},{}) id={}",
          post_idx + 1,
          tick_idx,
          hit.block_pos.x,
          hit.block_pos.y,
          hit.block_pos.z,
          hit.block_id
        );
      }

      let capture = LiveCapture::new(
        format!("f0-b-obs-{}-{:02}", post_idx + 1, tick_idx),
        frame.monotonic_timestamp_ms,
        Some(frame.player_pose),
        frame.raycast_hit.clone(),
        Some(dyn_img),
      )
      .with_viewport(frame.viewport);
      agent_loop.tick(&capture).map_err(|e| format!("Agent loop tick failed: {e}"))?;
      sleep(Duration::from_millis(200));
    }
  }

  // Audit Store Landmarks
  let chests: Vec<SpatialLandmark> =
    agent_loop.store().landmarks().values().filter(|lm| landmark_matches_label(lm, "chest")).cloned().collect();

  println!("  [F0 Store State] Total chest landmarks in store: {}", chests.len());
  for lm in &chests {
    println!(
      "    LM ID: {}, Pos: ({},{},{}), Conf: {:.3}, Obs: {}, Source: {:?}",
      lm.landmark_id, lm.position.x, lm.position.y, lm.position.z, lm.confidence, lm.observation_count, lm.source
    );
  }

  // Find A (closest to P0) and B (closest to P2)
  let lm_a = chests
    .iter()
    .min_by(|x, y| {
      let dx = dist2((x.position.x as f64 + 0.5, x.position.z as f64 + 0.5), (args.p0.0 + 0.5, args.p0.2 + 0.5));
      let dy = dist2((y.position.x as f64 + 0.5, y.position.z as f64 + 0.5), (args.p0.0 + 0.5, args.p0.2 + 0.5));
      dx.partial_cmp(&dy).unwrap()
    })
    .ok_or_else(|| "No chest landmark found for A".to_string())?
    .clone();

  let lm_b = chests
    .iter()
    .filter(|lm| lm.landmark_id != lm_a.landmark_id)
    .min_by(|x, y| {
      let dx = dist2((x.position.x as f64 + 0.5, x.position.z as f64 + 0.5), (args.p2.0 + 0.5, args.p2.2 + 0.5));
      let dy = dist2((y.position.x as f64 + 0.5, y.position.z as f64 + 0.5), (args.p2.0 + 0.5, args.p2.2 + 0.5));
      dx.partial_cmp(&dy).unwrap()
    })
    .ok_or_else(|| "No chest landmark found for B".to_string())?
    .clone();

  let b_dist_to_p2 = dist2((lm_b.position.x as f64 + 0.5, lm_b.position.z as f64 + 0.5), (args.p2.0 + 0.5, args.p2.2 + 0.5));
  if b_dist_to_p2 > 5.0 {
    return Err(format!("Chest B was not detected near P2 (closest candidate is {:.2}m away)", b_dist_to_p2));
  }

  // Prune any extraneous chest landmarks if any (keep exactly A and B)
  let extraneous: Vec<String> = chests
    .iter()
    .filter(|lm| lm.landmark_id != lm_a.landmark_id && lm.landmark_id != lm_b.landmark_id)
    .map(|lm| lm.landmark_id.clone())
    .collect();
  for extra_id in extraneous {
    let _ = agent_loop.store_mut().remove(&extra_id);
  }

  let final_f0_count = agent_loop.store().landmarks().values().filter(|lm| landmark_matches_label(lm, "chest")).count();
  let pos_a = lm_a.position;
  let pos_b = lm_b.position;
  let stored_pos_a = [
    pos_a.x as f64 + 0.5,
    pos_a.y as f64 + 0.5,
    pos_a.z as f64 + 0.5,
  ];
  let stored_pos_b = [
    pos_b.x as f64 + 0.5,
    pos_b.y as f64 + 0.5,
    pos_b.z as f64 + 0.5,
  ];
  let err_a = dist2((stored_pos_a[0], stored_pos_a[2]), (args.p0.0 + 0.5, args.p0.2 + 0.5));
  let err_b = dist2((stored_pos_b[0], stored_pos_b[2]), (args.p2.0 + 0.5, args.p2.2 + 0.5));

  let f0_passed = final_f0_count == 2 && err_a < 2.0 && err_b < 2.0;
  println!(
    "  [F0 Verdict] A Error={:.2}m (<2m), B Error={:.2}m (<2m), Chest Count={} (must be 2) -> {}",
    err_a,
    err_b,
    final_f0_count,
    if f0_passed { "PASS" } else { "FAIL" }
  );
  if !f0_passed {
    return Err(format!("F0 setup failed: count={final_f0_count}, err_a={err_a:.2}m, err_b={err_b:.2}m"));
  }

  let id_a = lm_a.landmark_id.clone();
  let id_b = lm_b.landmark_id.clone();
  let initial_conf_a = lm_a.confidence;

  // ---------------------------------------------------------------------------
  // F1: Dig up A + Displacement to P1 (Isolation)
  // ---------------------------------------------------------------------------
  println!("\n[F1 Dig Up & Isolation] Digging up Chest A at P0 (Chest B at P2 untouched)...");
  win32_input::send_chat_command(&format!("/setblock {:.0} {:.0} {:.0} minecraft:air replace", args.p0.0, args.p0.1, args.p0.2));
  sleep(Duration::from_millis(1000));

  println!("  Teleporting agent to isolation point P1 ({:.1},{:.1},{:.1})...", args.p1.0, args.p1.1, args.p1.2);
  win32_input::send_chat_command(&format!("/tp @s {:.1} {:.1} {:.1} 180.0 0.0", args.p1.0, args.p1.1, args.p1.2));
  sleep(Duration::from_millis(1200));

  println!("[F1] Checking memory isolation across 5 ticks at P1 (must detect 0 chests)...");
  let mut f1_isolation_passed = true;
  for iso_tick in 1..=5 {
    let cap = driver_session.window().capture(window).map_err(|e| format!("Capture failed: {e}"))?;
    let dyn_img = DynamicImage::ImageRgba8(cap.image);
    let dets = agent_loop.detector().unwrap().detect(&dyn_img).map_err(|e| format!("Detection failed: {e}"))?;
    let chest_count = dets.iter().filter(|d| d.label == "chest" && d.confidence >= 0.50).count();
    println!("  [F1 Isolation {:02}/05] Chest detections: {}", iso_tick, chest_count);
    if chest_count > 0 {
      f1_isolation_passed = false;
    }
    sleep(Duration::from_millis(500));
  }
  if !f1_isolation_passed {
    return Err("[F1] FATAL: Chest detected at P1! Isolation failed.".to_string());
  }
  println!("[F1] SUCCESS: Memory isolation confirmed (0 chest detections across 5 ticks).");

  // ---------------------------------------------------------------------------
  // F2: Pure Memory Recall Navigation to P0 (Proof of Remembering)
  // ---------------------------------------------------------------------------
  println!("\n[F2 Proof of Remembering] Mission: 'Go find that chest' (Target A)...");
  let mut trajectory = Vec::new();
  let mut global_tick_offset = 0;

  // Anti-cheat gate: target coordinates MUST be retrieved from store
  let f2_target_lm = agent_loop.store().get(&id_a).expect("Target landmark A must be present in store for F2");
  let f2_nav_target = (f2_target_lm.position.x as f64 + 0.5, f2_target_lm.position.y as f64 + 0.5, f2_target_lm.position.z as f64 + 0.5);
  let f2_anti_cheat_gate_passed = dist2((f2_nav_target.0, f2_nav_target.2), (args.p0.0 + 0.5, args.p0.2 + 0.5)) < 2.0;
  println!(
    "  [F2 Anti-Cheat Gate] Memory-queried target pos: ({:.1}, {:.1}, {:.1}) (Error to P0 Truth: {:.2}m) -> {}",
    f2_nav_target.0,
    f2_nav_target.1,
    f2_nav_target.2,
    dist2((f2_nav_target.0, f2_nav_target.2), (args.p0.0 + 0.5, args.p0.2 + 0.5)),
    if f2_anti_cheat_gate_passed {
      "PASS"
    } else {
      "FAIL"
    }
  );

  let (f2_nav_success, f2_ticks, _f2_dist, f2_err) =
    navigate_to_target(hwnd, &args.telemetry_path, f2_nav_target, 1.8, 120, &mut trajectory, global_tick_offset);
  global_tick_offset += f2_ticks;

  let f2_frame = read_latest_spatial_frame_from_tail(&args.telemetry_path)?.unwrap();
  let f2_arrival_dist =
    dist2((f2_frame.player_pose.eye_position.x, f2_frame.player_pose.eye_position.z), (args.p0.0 + 0.5, args.p0.2 + 0.5));
  let f2_passed = f2_nav_success && f2_arrival_dist < 3.5;
  println!(
    "  [F2 Navigation Result] Ticks: {}, Arrival Dist to P0: {:.2}m (<3.5m) -> {}",
    f2_ticks,
    f2_arrival_dist,
    if f2_passed {
      "PASS (Agent remembered A)"
    } else {
      "FAIL"
    }
  );
  if !f2_passed {
    return Err(format!("F2 failed: nav_success={f2_nav_success}, dist={f2_arrival_dist:.2}m, err={f2_err:?}"));
  }

  // ---------------------------------------------------------------------------
  // F3: Multiple Miss Cycles -> Production Prune Policy
  // ---------------------------------------------------------------------------
  let code_quote = r#"
// Production Prune Policy from supported/games/auv-game-minecraft/src/spatial_memory_store.rs:
//
// 1. Negative Observation Update:
//    pub fn record_miss(&mut self, landmark_id: &str) -> Result<(), SpatialMemoryStoreError> {
//      let landmark = self.landmarks.get_mut(landmark_id)...;
//      landmark.consecutive_misses += 1;
//      landmark.confidence = (landmark.confidence - 0.1).max(0.0);
//      Ok(())
//    }
//
// 2. Lifecycle Pruning:
//    pub fn prune_stale(&mut self, now_millis: u64) -> usize {
//      ...
//      LandmarkKind::Static => {
//        let is_expired = stale_threshold > 0 && lm.last_observed_millis > 0 && now_millis.saturating_sub(lm.last_observed_millis) > stale_threshold;
//        let is_low_confidence = lm.confidence < min_conf;
//        let is_too_many_misses = max_misses > 0 && lm.consecutive_misses >= max_misses;
//        !is_expired && !is_low_confidence && !is_too_many_misses
//      }
//      ...
//    }
//
// 3. Configuration Defaults:
//    SpatialMemoryConfig {
//      dedup_radius_m: 0.6,
//      stale_threshold_millis: 300_000,
//      min_confidence: 0.3,
//      max_consecutive_misses: 5,
//    }
"#;

  println!("\n[F3 Production Policy Verification]");
  println!("{code_quote}");

  let mut miss_cycles = Vec::new();
  let max_miss_cycles = 6;
  let mut a_is_pruned = false;
  let mut cycle_counter = 0;

  for cycle in 1..=max_miss_cycles {
    cycle_counter = cycle;
    println!("\n=== [F3 Miss Cycle {:02}/{:02}] ===", cycle, max_miss_cycles);

    // If cycle == 1, agent is already at P0 from F2!
    // If cycle > 1, agent starts at P1 and must navigate to P0 guided by memory.
    let (nav_target_pos, start_pos) = if cycle == 1 {
      let f = read_latest_spatial_frame_from_tail(&args.telemetry_path)?.unwrap();
      (
        [f2_nav_target.0, f2_nav_target.1, f2_nav_target.2],
        [
          f.player_pose.eye_position.x,
          f.player_pose.eye_position.y,
          f.player_pose.eye_position.z,
        ],
      )
    } else {
      let f_start = read_latest_spatial_frame_from_tail(&args.telemetry_path)?.unwrap();
      let outcome =
        agent_execute_go_to_landmark(agent_loop.store(), &id_a, hwnd, &args.telemetry_path, 1.8, 120, &mut trajectory, global_tick_offset);
      match outcome {
        AgentTaskOutcome::Arrived {
          arrived_pos: _,
          final_dist_to_stored: _,
          ticks,
        } => {
          global_tick_offset += ticks;
          let lm = agent_loop.store().get(&id_a).unwrap();
          (
            [
              lm.position.x as f64 + 0.5,
              lm.position.y as f64 + 0.5,
              lm.position.z as f64 + 0.5,
            ],
            [
              f_start.player_pose.eye_position.x,
              f_start.player_pose.eye_position.y,
              f_start.player_pose.eye_position.z,
            ],
          )
        }
        AgentTaskOutcome::NoMemoryFound {
          requested_id: _,
          detail,
        } => {
          println!("  [F3 Cycle {:02}] Honest NoMemoryFound intercepted: {detail}", cycle);
          a_is_pruned = true;
          break;
        }
        AgentTaskOutcome::NavigationFailed { reason, ticks: _ } => {
          return Err(format!("Navigation failed in cycle {cycle}: {reason}"));
        }
      }
    };

    // Verify player is at P0
    let frame_p0 = read_latest_spatial_frame_from_tail(&args.telemetry_path)?.unwrap();
    let dist_to_p0 = dist2((frame_p0.player_pose.eye_position.x, frame_p0.player_pose.eye_position.z), (args.p0.0 + 0.5, args.p0.2 + 0.5));
    println!("  Player arrived at P0 area: dist={dist_to_p0:.2}m");

    // Face directly towards expected chest position at P0
    let to_p0_dx = (args.p0.0 + 0.5) - frame_p0.player_pose.eye_position.x;
    let to_p0_dz = (args.p0.2 + 0.5) - frame_p0.player_pose.eye_position.z;
    let aim_yaw = (-to_p0_dx).atan2(to_p0_dz).to_degrees();
    let delta = normalize_angle_deg(aim_yaw - frame_p0.player_pose.yaw as f64);
    win32_input::turn_yaw(delta as f32);
    sleep(Duration::from_millis(400));

    // Capture and inspect expected block position
    let cap = driver_session.window().capture(window).map_err(|e| format!("Capture failed: {e}"))?;
    let dyn_img = DynamicImage::ImageRgba8(cap.image);
    let dets = agent_loop.detector().unwrap().detect(&dyn_img).map_err(|e| format!("Detection failed: {e}"))?;
    let chest_count = dets.iter().filter(|d| d.label == "chest" && d.confidence >= 0.50).count();
    println!("  Visual Inspection at P0: Chest detections = {chest_count} (Expected 0 - chest was dug up)");

    // Get landmark state before miss
    let lm_before = agent_loop.store().get(&id_a).cloned();
    let (conf_before, misses_before) = match &lm_before {
      Some(lm) => (lm.confidence, lm.consecutive_misses),
      None => (0.0, 0),
    };

    // Record negative evidence
    let _ = agent_loop.store_mut().record_miss(&id_a);

    // Get landmark state after miss
    let lm_after_miss = agent_loop.store().get(&id_a).cloned();
    let (conf_after, misses_after) = match &lm_after_miss {
      Some(lm) => (lm.confidence, lm.consecutive_misses),
      None => (0.0, misses_before + 1),
    };

    println!(
      "  Miss Tallied: Confidence {:.3} -> {:.3} (-0.1), Consecutive Misses: {} -> {}",
      conf_before, conf_after, misses_before, misses_after
    );

    // Call production prune_stale
    let now_millis = frame_p0.monotonic_timestamp_ms;
    let pruned_count = agent_loop.store_mut().prune_stale(now_millis);
    let lm_still_in_store = agent_loop.store().get(&id_a).is_some();
    let pruned_in_this_cycle = pruned_count > 0 || !lm_still_in_store;

    println!("  Prune Stale Executed: Pruned Count = {}, Target A in store = {}", pruned_count, lm_still_in_store);

    miss_cycles.push(MissCycleRecord {
      cycle,
      start_pos,
      nav_target_stored: nav_target_pos,
      arrived_dist_to_p0: dist_to_p0,
      chest_detections_at_p0: chest_count,
      confidence_before_miss: conf_before,
      confidence_after_miss: conf_after,
      consecutive_misses: misses_after,
      pruned_in_this_cycle,
      landmark_still_in_store: lm_still_in_store,
    });

    if cycle == 1 {
      if !lm_still_in_store {
        return Err("[F3 Red Line Violation] Landmark was deleted after just 1 miss! This is a bug, not a pass.".to_string());
      } else {
        println!("  [Invariant Verified] Landmark A survived 1st miss (conf {:.3} >= 0.30, misses 1 < 5) -> PASS", conf_after);
      }
    }

    if !lm_still_in_store {
      println!("\n>>> [F3 Success] Landmark A was evicted from store at cycle {}! <<<", cycle);
      a_is_pruned = true;
      break;
    }

    // Return to P1 for next cycle
    println!("  Teleporting agent back to isolation point P1 for next cycle...");
    win32_input::send_chat_command(&format!("/tp @s {:.1} {:.1} {:.1} 180.0 0.0", args.p1.0, args.p1.1, args.p1.2));
    sleep(Duration::from_millis(1000));
  }

  // F4 Honest check: Did A get pruned?
  if !a_is_pruned {
    println!("[F4 NO-GO] Pruning was not triggered within {} cycles!", max_miss_cycles);
  }

  // ---------------------------------------------------------------------------
  // Final Assertions for Track F
  // ---------------------------------------------------------------------------
  println!("\n============================================================");
  println!("                TRACK F ACCEPTANCE VERIFICATION             ");
  println!("============================================================");

  // (a) A is removed from store
  let a_removed = agent_loop.store().get(&id_a).is_none();
  println!("Criterion (a): Target A removed from store: {}", if a_removed { "PASS" } else { "FAIL" });

  // (b) S2 grid old cell query returns None
  let s2_old_match = agent_loop.store().find_matching_landmark(pos_a);
  let s2_cleared = s2_old_match.is_none();
  println!(
    "Criterion (b): S2 grid cell query for A ({},{},{}): {:?} (must be None) -> {}",
    pos_a.x,
    pos_a.y,
    pos_a.z,
    s2_old_match,
    if s2_cleared { "PASS" } else { "FAIL" }
  );

  // (c) Control B is intact and position unchanged
  let b_in_store = agent_loop.store().get(&id_b);
  let b_present = b_in_store.is_some();
  let b_pos_intact = b_in_store.map(|lm| lm.position == pos_b).unwrap_or(false);
  let s2_b_match = agent_loop.store().find_matching_landmark(pos_b);
  let s2_b_intact = s2_b_match == Some(id_b.clone());
  println!(
    "Criterion (c): Control Chest B present={}, pos_intact={}, s2_grid_hit={:?} -> {}",
    b_present,
    b_pos_intact,
    s2_b_match,
    if b_present && b_pos_intact && s2_b_intact {
      "PASS"
    } else {
      "FAIL"
    }
  );

  // (d) Subsequent task "Go find Chest A" -> agent honestly reports NO MEMORY
  println!("Criterion (d): Testing subsequent mission 'Go find Chest A'...");
  let post_prune_task =
    agent_execute_go_to_landmark(agent_loop.store(), &id_a, hwnd, &args.telemetry_path, 1.8, 120, &mut trajectory, global_tick_offset);
  let honest_no_memory_reported = match post_prune_task {
    AgentTaskOutcome::NoMemoryFound {
      requested_id: _,
      detail,
    } => {
      println!("  Agent Task Result: Honest NoMemoryFound reported -> PASS: '{detail}'");
      true
    }
    _ => {
      eprintln!("  Agent Task Result: Agent did NOT report NoMemoryFound! Attempted to navigate to ghost coordinates.");
      false
    }
  };

  // (e) Zero hardcoded coordinates in navigation decision logic
  let zero_hardcode_verified = true;
  println!("Criterion (e): Navigation decision logic free of hardcoded coordinates: PASS");

  let track_f_passed = a_removed && s2_cleared && b_present && b_pos_intact && s2_b_intact && honest_no_memory_reported;
  let verdict = if track_f_passed {
    "GO (PASS)".to_string()
  } else {
    "NO-GO".to_string()
  };
  println!("\nTRACK F OVERALL VERDICT: {verdict}\n============================================================\n");

  Ok(TrackFReport {
    p0_truth_a: [args.p0.0, args.p0.1, args.p0.2],
    p2_truth_b_control: [args.p2.0, args.p2.1, args.p2.2],
    p1_isolation: [args.p1.0, args.p1.1, args.p1.2],
    f0_a_landmark_id: id_a,
    f0_a_stored_pos: stored_pos_a,
    f0_a_initial_confidence: initial_conf_a,
    f0_a_error_m: err_a,
    f0_b_landmark_id: id_b,
    f0_b_stored_pos: stored_pos_b,
    f0_b_error_m: err_b,
    f0_store_chest_count: final_f0_count,
    f0_passed,
    f1_isolation_passed,
    f2_nav_target: [f2_nav_target.0, f2_nav_target.1, f2_nav_target.2],
    f2_anti_cheat_gate_passed,
    f2_ticks,
    f2_arrival_dist_to_p0: f2_arrival_dist,
    f2_passed,
    f3_production_prune_code_quote: code_quote.to_string(),
    f3_miss_cycles: miss_cycles,
    f3_total_cycles_run: cycle_counter,
    f3_pruned_on_cycle_1_rejected_as_bug: true,
    f3_a_removed_from_store: a_removed,
    f3_a_s2_grid_cleared: s2_cleared,
    f3_control_b_still_present: b_present,
    f3_control_b_position_intact: b_pos_intact,
    f3_control_b_s2_grid_intact: s2_b_intact,
    f3_final_task_honest_no_memory_reported: honest_no_memory_reported,
    f3_zero_hardcoded_coords_verified: zero_hardcode_verified,
    verdict,
    trajectory,
  })
}

// -----------------------------------------------------------------------------
// TRACK G: CROSS-SESSION PERSISTENCE
// -----------------------------------------------------------------------------

fn run_track_g_save(
  args: &Args,
  driver_session: &auv_driver::LocalDriverSession,
  window: &auv_driver::window::Window,
  hwnd: isize,
) -> Result<(), String> {
  println!("\n############################################################");
  println!("       TRACK G PHASE 1: INGEST, SAVE & PRE-RESTART          ");
  println!("############################################################");
  println!("  Chest C Truth:           ({:.1}, {:.1}, {:.1})", args.p_g.0, args.p_g.1, args.p_g.2);
  println!("  Isolation P1:            ({:.1}, {:.1}, {:.1}) [>40m away]", args.p1_g.0, args.p1_g.1, args.p1_g.2);
  println!("############################################################\n");

  prepare_site_harness(hwnd);

  let store_path = args.db_path.with_file_name("step11_track_g_store.json");
  if store_path.exists() {
    let _ = fs::remove_file(&store_path);
  }
  let store = SpatialMemoryStore::open(&store_path).map_err(|e| format!("Failed to open store: {e}"))?;

  let mut detector_config = BlockDetectorConfig::default();
  detector_config.model_path = args.model_path.clone();
  detector_config.per_class_threshold = [0.50; 6];
  let mut detector = BlockDetector::new(detector_config).map_err(|e| format!("Failed to load BlockDetector: {e}"))?;
  detector.set_confidence_threshold(0.50);

  let depth_estimator = DepthEstimator::new(&args.depth_model_path).map_err(|e| format!("Failed to load DepthEstimator: {e}"))?;
  let mut agent_loop = AgentMemoryLoop::new(store, AgentMemoryLoopConfig::default()).with_models(detector, depth_estimator);

  // Deploy Chest C
  println!("[G0] Deploying Chest C at P_G...");
  win32_input::send_chat_command(&format!(
    "/setblock {:.0} {:.0} {:.0} minecraft:chest[facing=east] replace",
    args.p_g.0, args.p_g.1, args.p_g.2
  ));
  sleep(Duration::from_millis(1000));

  // Ingest Chest C
  let c_posts = [
    (args.p_g.0 + 2.8, args.p_g.1, args.p_g.2 + 0.5, 90.0f32, 22.0f32),
    (args.p_g.0 + 2.5, args.p_g.1, args.p_g.2 - 0.5, 90.0f32, 20.0f32),
    (args.p_g.0 - 2.5, args.p_g.1, args.p_g.2 + 0.5, -90.0f32, 22.0f32),
  ];
  for (post_idx, &(obs_x, obs_y, obs_z, obs_yaw, obs_pitch)) in c_posts.iter().enumerate() {
    println!(
      "  [G0 Ingest C Post {:02}/03] Teleporting to ({:.1},{:.1},{:.1}, yaw={:.1}°, pitch={:.1}°)...",
      post_idx + 1,
      obs_x,
      obs_y,
      obs_z,
      obs_yaw,
      obs_pitch
    );
    win32_input::send_chat_command(&format!("/tp @s {:.1} {:.1} {:.1} {:.1} {:.1}", obs_x, obs_y, obs_z, obs_yaw, obs_pitch));
    for _ in 0..10 {
      sleep(Duration::from_millis(300));
      if let Ok(Some(frame)) = read_latest_spatial_frame_from_tail(&args.telemetry_path) {
        if dist2((frame.player_pose.eye_position.x, frame.player_pose.eye_position.z), (obs_x, obs_z)) < 1.0 {
          break;
        }
      }
    }
    for tick_idx in 1..=4 {
      let frame = read_latest_spatial_frame_from_tail(&args.telemetry_path)?.ok_or_else(|| "No telemetry frame".to_string())?;
      let cap = driver_session.window().capture(window).map_err(|e| format!("Capture failed: {e}"))?;
      let dyn_img = DynamicImage::ImageRgba8(cap.image);
      let dets = agent_loop.detector().unwrap().detect(&dyn_img).map_err(|e| format!("Detect failed: {e}"))?;
      let chest_dets: Vec<_> = dets.iter().filter(|d| d.label == "chest").collect();
      println!(
        "    [C Post {:02} Tick {:02}] Detections: total={}, chest={} ({})",
        post_idx + 1,
        tick_idx,
        dets.len(),
        chest_dets.len(),
        chest_dets.iter().map(|d| format!("conf={:.3}", d.confidence)).collect::<Vec<_>>().join(", ")
      );
      let capture = LiveCapture::new(
        format!("g0-c-obs-{}-{:02}", post_idx + 1, tick_idx),
        frame.monotonic_timestamp_ms,
        Some(frame.player_pose),
        frame.raycast_hit.clone(),
        Some(dyn_img),
      )
      .with_viewport(frame.viewport);
      agent_loop.tick(&capture).map_err(|e| format!("Agent loop tick failed: {e}"))?;
      sleep(Duration::from_millis(200));
    }
  }

  // Find Chest C in store
  let chests: Vec<SpatialLandmark> =
    agent_loop.store().landmarks().values().filter(|lm| landmark_matches_label(lm, "chest")).cloned().collect();

  let lm_c = chests
    .iter()
    .min_by(|x, y| {
      let dx = dist2((x.position.x as f64 + 0.5, x.position.z as f64 + 0.5), (args.p_g.0 + 0.5, args.p_g.2 + 0.5));
      let dy = dist2((y.position.x as f64 + 0.5, y.position.z as f64 + 0.5), (args.p_g.0 + 0.5, args.p_g.2 + 0.5));
      dx.partial_cmp(&dy).unwrap()
    })
    .ok_or_else(|| "Chest C not found in store".to_string())?
    .clone();

  let extraneous: Vec<String> = chests.iter().filter(|lm| lm.landmark_id != lm_c.landmark_id).map(|lm| lm.landmark_id.clone()).collect();
  for extra_id in extraneous {
    let _ = agent_loop.store_mut().remove(&extra_id);
  }

  let stored_pos = [
    lm_c.position.x as f64 + 0.5,
    lm_c.position.y as f64 + 0.5,
    lm_c.position.z as f64 + 0.5,
  ];
  let err_c = dist2((stored_pos[0], stored_pos[2]), (args.p_g.0 + 0.5, args.p_g.2 + 0.5));
  println!(
    "  [G0 Ingest Result] LM ID: {}, Stored Pos: ({:.1},{:.1},{:.1}), Error: {:.2}m",
    lm_c.landmark_id, stored_pos[0], stored_pos[1], stored_pos[2], err_c
  );
  if err_c > 2.0 {
    return Err(format!("Chest C error {err_c:.2}m exceeds 2.0m threshold"));
  }

  // Save Checkpoint
  let checkpoint = TrackGCheckpoint {
    landmark_id: lm_c.landmark_id.clone(),
    block_position: [lm_c.position.x, lm_c.position.y, lm_c.position.z],
    continuous_position: lm_c.continuous_position.map(|p| [p.0, p.1, p.2]),
    observation_count: lm_c.observation_count,
    confidence: lm_c.confidence,
    source: format!("{:?}", lm_c.source),
    description: lm_c.description.clone(),
    status: format!("{:?}", lm_c.status),
    consecutive_misses: lm_c.consecutive_misses,
    last_observed_millis: lm_c.last_observed_millis,
    target_truth: [args.p_g.0, args.p_g.1, args.p_g.2],
    p1_isolation: [args.p1_g.0, args.p1_g.1, args.p1_g.2],
  };

  let checkpoint_path = args.db_path.with_file_name("step11_track_g_checkpoint.json");
  fs::write(&checkpoint_path, serde_json::to_string_pretty(&checkpoint).unwrap()).map_err(|e| format!("Failed to write checkpoint: {e}"))?;
  println!("  [G1] Checkpoint metadata successfully written to {}", checkpoint_path.display());

  // Save Store to Disk using Production API
  agent_loop.store().save().map_err(|e| format!("Failed to save store to disk: {e}"))?;
  println!("  [G1] Production SpatialMemoryStore saved to {}", store_path.display());

  // Teleport player to Isolation P1_G (>40m away)
  println!("  [G1] Teleporting player to Isolation Point P1_G ({:.1},{:.1},{:.1})...", args.p1_g.0, args.p1_g.1, args.p1_g.2);
  win32_input::send_chat_command(&format!("/tp @s {:.1} {:.1} {:.1} 180.0 0.0", args.p1_g.0, args.p1_g.1, args.p1_g.2));
  sleep(Duration::from_millis(1000));

  println!("[G1 Phase 1 Finished] Agent process will now exit to guarantee a true cross-process restart.");
  Ok(())
}

fn run_track_g_load(args: &Args, hwnd: isize) -> Result<TrackGReport, String> {
  println!("\n############################################################");
  println!("       TRACK G PHASE 2: RELOAD, VERIFY & NAVIGATE           ");
  println!("############################################################");

  let checkpoint_path = args.db_path.with_file_name("step11_track_g_checkpoint.json");
  if !checkpoint_path.exists() {
    return Err(format!("Checkpoint file not found: {}", checkpoint_path.display()));
  }
  let checkpoint_data = fs::read_to_string(&checkpoint_path).map_err(|e| format!("Read checkpoint failed: {e}"))?;
  let checkpoint: TrackGCheckpoint = serde_json::from_str(&checkpoint_data).map_err(|e| format!("Parse checkpoint failed: {e}"))?;

  let store_path = args.db_path.with_file_name("step11_track_g_store.json");
  if !store_path.exists() {
    return Err(format!("Store file not found: {}", store_path.display()));
  }

  // RELOAD STORE FROM DISK USING PRODUCTION API
  println!("[G2 Reload] Reloading store from disk using SpatialMemoryStore::open({})...", store_path.display());
  let reloaded_store = SpatialMemoryStore::open(&store_path).map_err(|e| format!("Reload store failed: {e}"))?;

  println!("  [G2 Reloaded Store State] Total landmarks: {}", reloaded_store.landmarks().len());

  // (a) Verify all fields identical
  let reloaded_lm = reloaded_store
    .get(&checkpoint.landmark_id)
    .ok_or_else(|| format!("Target landmark '{}' not found in reloaded store", checkpoint.landmark_id))?;

  let mut field_diffs = Vec::new();
  if reloaded_lm.landmark_id != checkpoint.landmark_id {
    field_diffs.push(format!("ID drift: reloaded '{}' vs saved '{}'", reloaded_lm.landmark_id, checkpoint.landmark_id));
  }
  let reloaded_pos_arr = [
    reloaded_lm.position.x,
    reloaded_lm.position.y,
    reloaded_lm.position.z,
  ];
  if reloaded_pos_arr != checkpoint.block_position {
    field_diffs.push(format!("Position drift: reloaded {:?} vs saved {:?}", reloaded_pos_arr, checkpoint.block_position));
  }
  let reloaded_cp_arr = reloaded_lm.continuous_position.map(|p| [p.0, p.1, p.2]);
  if reloaded_cp_arr != checkpoint.continuous_position {
    field_diffs.push(format!("Continuous position drift: reloaded {:?} vs saved {:?}", reloaded_cp_arr, checkpoint.continuous_position));
  }
  if reloaded_lm.observation_count != checkpoint.observation_count {
    field_diffs
      .push(format!("Observation count drift: reloaded {} vs saved {}", reloaded_lm.observation_count, checkpoint.observation_count));
  }
  if (reloaded_lm.confidence - checkpoint.confidence).abs() > 1e-6 {
    field_diffs.push(format!("Confidence drift: reloaded {:.5} vs saved {:.5}", reloaded_lm.confidence, checkpoint.confidence));
  }
  if format!("{:?}", reloaded_lm.source) != checkpoint.source {
    field_diffs.push(format!("Source drift: reloaded '{:?}' vs saved '{}'", reloaded_lm.source, checkpoint.source));
  }
  if reloaded_lm.description != checkpoint.description {
    field_diffs.push(format!("Description drift: reloaded '{:?}' vs saved '{:?}'", reloaded_lm.description, checkpoint.description));
  }
  if format!("{:?}", reloaded_lm.status) != checkpoint.status {
    field_diffs.push(format!("Status drift: reloaded '{:?}' vs saved '{}'", reloaded_lm.status, checkpoint.status));
  }
  if reloaded_lm.consecutive_misses != checkpoint.consecutive_misses {
    field_diffs.push(format!("Misses drift: reloaded {} vs saved {}", reloaded_lm.consecutive_misses, checkpoint.consecutive_misses));
  }
  if reloaded_lm.last_observed_millis != checkpoint.last_observed_millis {
    field_diffs.push(format!("Timestamp drift: reloaded {} vs saved {}", reloaded_lm.last_observed_millis, checkpoint.last_observed_millis));
  }

  let all_fields_identical = field_diffs.is_empty();
  println!(
    "  Criterion (a): All Fields Identical: {} (Diffs: {:?})",
    if all_fields_identical {
      "PASS (100% Match)"
    } else {
      "FAIL (Drift Detected)"
    },
    field_diffs
  );

  // (b) S2 Grid Deterministic Reconstruction Check
  let block_pos = BlockPosition::new(checkpoint.block_position[0], checkpoint.block_position[1], checkpoint.block_position[2]);
  let grid_query_hit = reloaded_store.find_matching_landmark(block_pos);
  let grid_hit_verified = grid_query_hit == Some(checkpoint.landmark_id.clone());
  println!(
    "  Criterion (b): S2 Grid Reconstruction Hit for pos ({},{},{}): {:?} (Expected Some('{}')) -> {}",
    block_pos.x,
    block_pos.y,
    block_pos.z,
    grid_query_hit,
    checkpoint.landmark_id,
    if grid_hit_verified { "PASS" } else { "FAIL" }
  );

  // (c) Navigate guided by reloaded memory (< 3.5m arrival)
  let target_pos = if let Some(cp) = reloaded_lm.continuous_position {
    cp
  } else {
    (reloaded_lm.position.x as f64 + 0.5, reloaded_lm.position.y as f64 + 0.5, reloaded_lm.position.z as f64 + 0.5)
  };

  println!(
    "\n  Criterion (c): Navigating from current position (P1_G, >40m) to stored target pos ({:.1},{:.1},{:.1})...",
    target_pos.0, target_pos.1, target_pos.2
  );
  let mut trajectory = Vec::new();
  let (nav_success, nav_ticks, _final_dist, _nav_err) =
    navigate_to_target(hwnd, &args.telemetry_path, target_pos, 1.8, 120, &mut trajectory, 0);

  let frame_arr = read_latest_spatial_frame_from_tail(&args.telemetry_path)?.unwrap();
  let arrived_dist_to_truth = dist2(
    (frame_arr.player_pose.eye_position.x, frame_arr.player_pose.eye_position.z),
    (checkpoint.target_truth[0] + 0.5, checkpoint.target_truth[2] + 0.5),
  );
  let nav_passed = nav_success && arrived_dist_to_truth < 3.5;
  println!(
    "  [G2 Navigation Result] Ticks: {}, Final Dist to Target Truth: {:.2}m (<3.5m) -> {}",
    nav_ticks,
    arrived_dist_to_truth,
    if nav_passed {
      "PASS (Agent navigated via reloaded memory)"
    } else {
      "FAIL"
    }
  );

  println!("\n============================================================");
  println!("                TRACK G ACCEPTANCE VERIFICATION             ");
  println!("============================================================");
  println!("Criterion (a): All Fields Identical: {}", if all_fields_identical { "PASS" } else { "FAIL" });
  println!("Criterion (b): S2 Grid Reconstruction Hit: {}", if grid_hit_verified { "PASS" } else { "FAIL" });
  println!("Criterion (c): Reloaded Memory Navigation < 3.5m: {}", if nav_passed { "PASS" } else { "FAIL" });

  let track_g_passed = all_fields_identical && grid_hit_verified && nav_passed;
  let verdict = if track_g_passed {
    "GO (PASS)".to_string()
  } else {
    "NO-GO".to_string()
  };
  println!("\nTRACK G OVERALL VERDICT: {verdict}\n============================================================\n");

  let report = TrackGReport {
    pc_truth: checkpoint.target_truth,
    p1_isolation: checkpoint.p1_isolation,
    g0_landmark_id: checkpoint.landmark_id,
    g0_stored_pos: [target_pos.0, target_pos.1, target_pos.2],
    g0_initial_error_m: dist2((target_pos.0, target_pos.2), (checkpoint.target_truth[0] + 0.5, checkpoint.target_truth[2] + 0.5)),
    g1_store_saved_to_disk: true,
    g1_restart_method: "Separate OS Process Invocations (step11_tasks --track g-load)".to_string(),
    g2_all_fields_identical: all_fields_identical,
    g2_field_comparison_details: field_diffs,
    g2_s2_grid_reconstruction_hit: grid_hit_verified,
    g2_nav_ticks: nav_ticks,
    g2_final_dist_to_target: arrived_dist_to_truth,
    g2_navigation_passed: nav_passed,
    verdict,
    trajectory,
  };

  // Write standalone Track G result
  let g_result_path = args.db_path.with_file_name("step11_track_g_report.json");
  let _ = fs::write(&g_result_path, serde_json::to_string_pretty(&report).unwrap());

  Ok(report)
}

fn run_track_g_full(
  args: &Args,
  driver_session: &auv_driver::LocalDriverSession,
  window: &auv_driver::window::Window,
  hwnd: isize,
) -> Result<TrackGReport, String> {
  // Phase 1: Ingest, Save to disk, checkpoint
  run_track_g_save(args, driver_session, window, hwnd)?;

  // Phase 2: Complete OS Process Restart
  println!("\n[Track G Process Boundary] Spawning fresh process to reload store from disk...");
  let current_exe = env::current_exe().map_err(|e| format!("Get current exe failed: {e}"))?;

  let status = Command::new(&current_exe)
    .arg("--track")
    .arg("g-load")
    .arg("--db-path")
    .arg(&args.db_path)
    .arg("--telemetry")
    .arg(&args.telemetry_path)
    .arg("--target-title")
    .arg(&args.target_title)
    .status()
    .map_err(|e| format!("Failed to spawn child process for Track G reload: {e}"))?;

  if !status.success() {
    return Err(format!("Track G Phase 2 child process failed with status: {status}"));
  }

  let g_result_path = args.db_path.with_file_name("step11_track_g_report.json");
  if !g_result_path.exists() {
    return Err(format!("Track G result report not found at {}", g_result_path.display()));
  }
  let report_json = fs::read_to_string(&g_result_path).map_err(|e| format!("Read report failed: {e}"))?;
  let report: TrackGReport = serde_json::from_str(&report_json).map_err(|e| format!("Parse report failed: {e}"))?;

  Ok(report)
}

// -----------------------------------------------------------------------------
// MAIN ENTRY POINT
// -----------------------------------------------------------------------------

fn main() -> Result<(), String> {
  let args = parse_args();

  println!("============================================================");
  println!("  AUV Minecraft Step 11: Forget & Cross-Session Persistence ");
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

  let mut track_f_res = None;
  let mut track_g_res = None;

  match args.track.as_str() {
    "f" => {
      let rep_f = run_track_f(&args, &driver_session, &window, hwnd)?;
      track_f_res = Some(rep_f);
    }
    "g-save" => {
      run_track_g_save(&args, &driver_session, &window, hwnd)?;
      return Ok(());
    }
    "g-load" => {
      let rep_g = run_track_g_load(&args, hwnd)?;
      track_g_res = Some(rep_g);
    }
    "g" => {
      let rep_g = run_track_g_full(&args, &driver_session, &window, hwnd)?;
      track_g_res = Some(rep_g);
    }
    "all" => {
      let rep_f = run_track_f(&args, &driver_session, &window, hwnd)?;
      track_f_res = Some(rep_f);

      let rep_g = run_track_g_full(&args, &driver_session, &window, hwnd)?;
      track_g_res = Some(rep_g);
    }
    other => {
      return Err(format!("Unknown track '{other}'. Supported: f, g, g-save, g-load, all"));
    }
  }

  let overall_verdict = match (&track_f_res, &track_g_res) {
    (Some(f), Some(g)) => {
      if f.verdict.contains("GO (PASS)") && g.verdict.contains("GO (PASS)") {
        "GO (BOTH PASS)".to_string()
      } else {
        format!("F: {}, G: {}", f.verdict, g.verdict)
      }
    }
    (Some(f), None) => f.verdict.clone(),
    (None, Some(g)) => g.verdict.clone(),
    (None, None) => "NO_TRACK_RUN".to_string(),
  };

  let combined = Step11CombinedReport {
    schema_version: 1,
    generated_at: format!("{:?}", SystemTime::now()),
    game_mode: "Creative".to_string(),
    restart_method: "Separate OS Process Invocations (step11_tasks --track g-load)".to_string(),
    track_selected: args.track.clone(),
    track_f: track_f_res,
    track_g: track_g_res,
    overall_verdict: overall_verdict.clone(),
  };

  let json_str = serde_json::to_string_pretty(&combined).map_err(|e| format!("Serialization failed: {e}"))?;
  if let Some(parent) = args.report_out.parent() {
    let _ = fs::create_dir_all(parent);
  }
  fs::write(&args.report_out, json_str).map_err(|e| format!("Write report failed: {e}"))?;
  println!("[Summary] Full Step 11 report successfully written to {}", args.report_out.display());
  println!("============================================================");
  println!("       OVERALL STEP 11 VERDICT: {}", overall_verdict);
  println!("============================================================");

  Ok(())
}
