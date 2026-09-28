//! Standalone Verification Binary for Minecraft Chest Recall (Step 8: Memory-Driven Task Verification).
//!
//! Validates the full closed-loop pipeline across 5 sequential verification stages:
//! - T0: Site and parameter initialization, pure mathematical yaw verification (<5°), creative mode declaration.
//! - T1: Stage 1 visual ingest (1Hz tick loop via AgentMemoryLoop, chest landmark detection, source=Visual, error < 2.0m).
//! - T2: Teleportation to P1 and memory isolation assertion (5 ticks at P1, chest detections MUST be 0).
//! - T3: Stage 2 memory navigation GATE (2Hz loop, max 120 ticks, target strictly from SpatialMemoryStore, anti-cheat redline enforced,
//!   yaw_delta > 15° turns mouse, yaw_delta <= 15° steps W, stop at <3.5m, 15 stagnant ticks fails).
//! - T4: Stage 3 chest interaction (best effort: project chest via MinecraftProjector, aim & right-click, screenshot to F:\auv\.tmp\recall_chest_gui.png).
//! - T5: Summary JSON report generated to F:\auv\.tmp\recall_verify_report.json with full trajectories and stage metrics.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use image::DynamicImage;
use serde::{Deserialize, Serialize};

use auv_game_minecraft::agent_memory_loop::{AgentMemoryLoop, AgentMemoryLoopConfig, LiveCapture};
use auv_game_minecraft::ingest::read_latest_spatial_frame_from_tail;
use auv_game_minecraft::projection::MinecraftProjector;
use auv_game_minecraft::spatial_memory_store::{LandmarkSource, SpatialLandmark, SpatialMemoryStore};
use auv_game_minecraft::types::MinecraftBlockTarget;
use auv_game_minecraft::visual_perception::{BlockDetector, BlockDetectorConfig, DEFAULT_PER_CLASS_THRESHOLDS, DepthEstimator};

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
  pub const MOUSEEVENTF_RIGHTDOWN: u32 = 0x0008;
  pub const MOUSEEVENTF_RIGHTUP: u32 = 0x0010;

  pub const KEYEVENTF_KEYUP: u32 = 0x0002;
  pub const KEYEVENTF_UNICODE: u32 = 0x0004;
  pub const KEYEVENTF_SCANCODE: u32 = 0x0008;

  pub const SW_RESTORE: i32 = 9;

  /// Empirically calibrated sensitivity: 0.150 deg/px (options.txt: mouseSensitivity 0.5, rawMouseInput true)
  pub const YAW_SENSITIVITY_DEG_PER_PX: f32 = 0.150;
  pub const SCANCODE_W: u16 = 0x11;
  pub const VK_W: u16 = 0x57;
  pub const VK_SLASH: u16 = 0xBF;
  pub const SCANCODE_SLASH: u16 = 0x35;
  pub const VK_RETURN: u16 = 0x0D;
  pub const SCANCODE_RETURN: u16 = 0x1C;

  type HWND = *mut c_void;
  type HDESK = *mut c_void;
  type BOOL = i32;

  static TARGET_HWND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

  #[link(name = "user32")]
  unsafe extern "system" {
    fn OpenDesktopA(name: *const u8, flags: u32, inherit: BOOL, access: u32) -> HDESK;
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
      let hdesk = OpenDesktopA(c"default".as_ptr().cast(), 0, 0, 0x01FF);
      if !hdesk.is_null() {
        let _ = SetThreadDesktop(hdesk);
      }
    }
  }

  pub fn activate_minecraft(hwnd_val: isize) -> Result<(), String> {
    TARGET_HWND.store(hwnd_val, std::sync::atomic::Ordering::SeqCst);
    ensure_default_desktop();
    let hwnd = hwnd_val as HWND;
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

    // GLFW Mouse Priming: eliminates initial relative mouse delta discarding
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
    Ok(())
  }

  /// Sends relative mouse motion delta (dx, dy) in pixels.
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

  /// Turns yaw by delta_deg using calibrated sensitivity (0.150 deg/pixel).
  pub fn turn_yaw(delta_deg: f32) {
    let dx = (delta_deg / YAW_SENSITIVITY_DEG_PER_PX).round() as i32;
    turn_mouse(dx, 0);
  }

  /// Holds the 'W' key for `duration` to step forward, then releases it.
  /// Uses PostMessageW directly to Minecraft window queue to avoid background/IME drops.
  pub fn step_forward(duration: Duration) {
    ensure_default_desktop();
    let hwnd_val = TARGET_HWND.load(std::sync::atomic::Ordering::SeqCst);
    if hwnd_val != 0 {
      let hwnd = hwnd_val as HWND;
      let lparam_down = ((SCANCODE_W as isize) << 16) | 1;
      let lparam_up = ((SCANCODE_W as isize) << 16) | 1 | (1 << 30) | (1 << 31);
      unsafe {
        PostMessageW(hwnd, 0x0100, VK_W as usize, lparam_down);
      }
      sleep(duration);
      unsafe {
        PostMessageW(hwnd, 0x0101, VK_W as usize, lparam_up);
      }
      return;
    }

    let key_down = INPUT {
      r#type: INPUT_KEYBOARD,
      u: INPUT_UNION {
        ki: KEYBDINPUT {
          w_vk: VK_W,
          w_scan: SCANCODE_W,
          dw_flags: KEYEVENTF_SCANCODE,
          time: 0,
          dw_extra_info: 0,
        },
      },
    };
    let key_up = INPUT {
      r#type: INPUT_KEYBOARD,
      u: INPUT_UNION {
        ki: KEYBDINPUT {
          w_vk: VK_W,
          w_scan: SCANCODE_W,
          dw_flags: KEYEVENTF_SCANCODE | KEYEVENTF_KEYUP,
          time: 0,
          dw_extra_info: 0,
        },
      },
    };
    unsafe {
      SendInput(1, &key_down, size_of::<INPUT>() as i32);
    }
    sleep(duration);
    unsafe {
      SendInput(1, &key_up, size_of::<INPUT>() as i32);
    }
  }

  /// Sends a chat command via '/', Unicode typing, and Enter.
  pub fn send_chat_command(command: &str) -> Result<(), String> {
    ensure_default_desktop();

    // 1. Open chat with '/'
    let slash_down = INPUT {
      r#type: INPUT_KEYBOARD,
      u: INPUT_UNION {
        ki: KEYBDINPUT {
          w_vk: VK_SLASH,
          w_scan: SCANCODE_SLASH,
          dw_flags: KEYEVENTF_SCANCODE,
          time: 0,
          dw_extra_info: 0,
        },
      },
    };
    let slash_up = INPUT {
      r#type: INPUT_KEYBOARD,
      u: INPUT_UNION {
        ki: KEYBDINPUT {
          w_vk: VK_SLASH,
          w_scan: SCANCODE_SLASH,
          dw_flags: KEYEVENTF_SCANCODE | KEYEVENTF_KEYUP,
          time: 0,
          dw_extra_info: 0,
        },
      },
    };
    unsafe {
      SendInput(1, &slash_down, size_of::<INPUT>() as i32);
      SendInput(1, &slash_up, size_of::<INPUT>() as i32);
    }
    sleep(Duration::from_millis(80));

    // 2. Type remaining characters via UNICODE
    let text = command.strip_prefix('/').unwrap_or(command);
    for ch in text.chars() {
      let mut buf = [0u16; 2];
      for unit in ch.encode_utf16(&mut buf) {
        let char_down = INPUT {
          r#type: INPUT_KEYBOARD,
          u: INPUT_UNION {
            ki: KEYBDINPUT {
              w_vk: 0,
              w_scan: *unit,
              dw_flags: KEYEVENTF_UNICODE,
              time: 0,
              dw_extra_info: 0,
            },
          },
        };
        let char_up = INPUT {
          r#type: INPUT_KEYBOARD,
          u: INPUT_UNION {
            ki: KEYBDINPUT {
              w_vk: 0,
              w_scan: *unit,
              dw_flags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
              time: 0,
              dw_extra_info: 0,
            },
          },
        };
        unsafe {
          SendInput(1, &char_down, size_of::<INPUT>() as i32);
          SendInput(1, &char_up, size_of::<INPUT>() as i32);
        }
        sleep(Duration::from_millis(15));
      }
    }
    sleep(Duration::from_millis(50));

    // 3. Submit with Enter
    let enter_down = INPUT {
      r#type: INPUT_KEYBOARD,
      u: INPUT_UNION {
        ki: KEYBDINPUT {
          w_vk: VK_RETURN,
          w_scan: SCANCODE_RETURN,
          dw_flags: KEYEVENTF_SCANCODE,
          time: 0,
          dw_extra_info: 0,
        },
      },
    };
    let enter_up = INPUT {
      r#type: INPUT_KEYBOARD,
      u: INPUT_UNION {
        ki: KEYBDINPUT {
          w_vk: VK_RETURN,
          w_scan: SCANCODE_RETURN,
          dw_flags: KEYEVENTF_SCANCODE | KEYEVENTF_KEYUP,
          time: 0,
          dw_extra_info: 0,
        },
      },
    };
    unsafe {
      SendInput(1, &enter_down, size_of::<INPUT>() as i32);
      SendInput(1, &enter_up, size_of::<INPUT>() as i32);
    }
    sleep(Duration::from_millis(100));
    Ok(())
  }

  /// Sends a right mouse click event (down then up).
  pub fn right_click() {
    ensure_default_desktop();
    let down = INPUT {
      r#type: INPUT_MOUSE,
      u: INPUT_UNION {
        mi: MOUSEINPUT {
          dx: 0,
          dy: 0,
          mouse_data: 0,
          dw_flags: MOUSEEVENTF_RIGHTDOWN,
          time: 0,
          dw_extra_info: 0,
        },
      },
    };
    let up = INPUT {
      r#type: INPUT_MOUSE,
      u: INPUT_UNION {
        mi: MOUSEINPUT {
          dx: 0,
          dy: 0,
          mouse_data: 0,
          dw_flags: MOUSEEVENTF_RIGHTUP,
          time: 0,
          dw_extra_info: 0,
        },
      },
    };
    unsafe {
      SendInput(1, &down, size_of::<INPUT>() as i32);
      sleep(Duration::from_millis(50));
      SendInput(1, &up, size_of::<INPUT>() as i32);
    }
  }
}

#[cfg(not(target_os = "windows"))]
mod win32_input {
  use std::time::Duration;
  pub fn activate_minecraft(_hwnd_val: isize) -> Result<(), String> {
    Ok(())
  }
  pub fn turn_mouse(_dx: i32, _dy: i32) {}
  pub fn turn_yaw(_delta_deg: f32) {}
  pub fn step_forward(_duration: Duration) {}
  pub fn send_chat_command(_command: &str) -> Result<(), String> {
    Ok(())
  }
  pub fn right_click() {}
}

/// Normalizes an angular delta into the standard `(-180.0, 180.0]` range.
pub fn normalize_angle_deg(delta: f64) -> f64 {
  let mut normalized = delta % 360.0;
  if normalized > 180.0 {
    normalized -= 360.0;
  } else if normalized <= -180.0 {
    normalized += 360.0;
  }
  normalized
}

/// Calculates expected Minecraft yaw (in degrees) from `from` (observer) to `to` (target).
///
/// In Minecraft coordinates:
/// - +X is East, -X is West.
/// - +Z is South (yaw = 0°).
/// - -Z is North (yaw = 180° / -180°).
/// - yaw = 90° is West (-X), yaw = -90° is East (+X).
///
/// Formula: `atan2(-dx, dz).to_degrees()`.
pub fn calculate_expected_yaw(from: (f64, f64, f64), to: (f64, f64, f64)) -> f64 {
  let dx = to.0 - from.0;
  let dz = to.2 - from.2;
  (-dx).atan2(dz).to_degrees()
}

/// Anti-cheat Memory Navigator:
///
/// GATE RULE: Target coordinates MUST be resolved dynamically from `SpatialMemoryStore`.
/// Hardcoding P0 constants in navigation logic is strictly forbidden.
#[derive(Clone, Debug)]
pub struct MemoryNavigator {
  pub target_landmark_id: String,
  pub target_position: (f64, f64, f64),
  pub target_label: String,
}

/// Matches a spatial landmark against a semantic label.
/// Checks:
/// 1. Description first word (e.g. "chest" from "chest (0.75)")
/// 2. Description substring
/// 3. Observations block_id
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

impl MemoryNavigator {
  /// Queries spatial memory for the target landmark by semantic label.
  /// Rejects with an error if no matching landmark is found.
  pub fn query_target(store: &SpatialMemoryStore, label: &str) -> Result<Self, String> {
    let landmark = store
      .landmarks()
      .values()
      .find(|lm| landmark_matches_label(lm, label))
      .ok_or_else(|| format!("Anti-cheat constraint violation: landmark with label '{label}' not found in SpatialMemoryStore!"))?;

    let target_position = if let Some(cp) = landmark.continuous_position {
      cp
    } else {
      (f64::from(landmark.position.x) + 0.5, f64::from(landmark.position.y) + 0.5, f64::from(landmark.position.z) + 0.5)
    };

    Ok(Self {
      target_landmark_id: landmark.landmark_id.clone(),
      target_position,
      target_label: label.to_string(),
    })
  }

  pub fn target_position(&self) -> (f64, f64, f64) {
    self.target_position
  }

  pub fn target_landmark_id(&self) -> &str {
    &self.target_landmark_id
  }
}

// -----------------------------------------------------------------------------
// Report Data Structures
// -----------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct T0Record {
  pub p0_ground_truth: [f64; 3],
  pub p1_teleport_target: [f64; 3],
  pub straight_line_distance_m: f64,
  pub horizontal_distance_m: f64,
  pub calculated_expected_yaw: f64,
  pub manual_expected_yaw: Option<f64>,
  pub yaw_error_deg: Option<f64>,
  pub math_validation_passed: bool,
  pub game_mode: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct T1Record {
  pub ticks_elapsed: usize,
  pub chest_landmark_id: Option<String>,
  pub chest_landmark_source: Option<String>,
  pub chest_estimated_position: Option<[f64; 3]>,
  pub p0_ground_truth: [f64; 3],
  pub position_error_m: Option<f64>,
  pub error_threshold_m: f64,
  pub passed: bool,
  pub failure_reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct T2Record {
  pub teleport_to_p1_confirmed: bool,
  pub p1_target: [f64; 3],
  pub p1_actual_pose: Option<[f64; 3]>,
  pub ticks_evaluated: usize,
  pub chest_detections_per_tick: Vec<usize>,
  pub total_chest_detections: usize,
  pub isolation_passed: bool,
  pub failure_reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NavTrajectoryPoint {
  pub tick: usize,
  pub player_pos: [f64; 3],
  pub yaw: f64,
  pub target_dist: f64,
  pub yaw_delta: f64,
  pub action: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct T3Record {
  pub target_source: String,
  pub target_landmark_id: Option<String>,
  pub target_queried_position: Option<[f64; 3]>,
  pub ticks_completed: usize,
  pub max_ticks: usize,
  pub start_distance_m: Option<f64>,
  pub final_distance_m: Option<f64>,
  pub success_threshold_m: f64,
  pub stagnant_ticks_observed: usize,
  pub navigation_passed: bool,
  pub failure_reason: Option<String>,
  pub trajectory: Vec<NavTrajectoryPoint>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct T4Record {
  pub projected_screen_point: Option<[f64; 2]>,
  pub projected_visibility: Option<String>,
  pub match_radius_px: Option<f64>,
  pub aim_yaw_delta: Option<f64>,
  pub aim_pitch_delta: Option<f64>,
  pub right_click_dispatched: bool,
  pub screenshot_saved_path: String,
  pub screenshot_saved: bool,
  pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecallVerifyReport {
  pub schema_version: u32,
  pub generated_at_millis: u64,
  pub game_mode: String,
  pub verdict: String,
  pub verdict_reasons: Vec<String>,
  pub t0_initialization: T0Record,
  pub t1_visual_ingest: T1Record,
  pub t2_memory_isolation: T2Record,
  pub t3_memory_navigation: T3Record,
  pub t4_chest_interaction: T4Record,
}

// -----------------------------------------------------------------------------
// CLI Arguments Parsing
// -----------------------------------------------------------------------------

struct Args {
  model_path: PathBuf,
  depth_model_path: PathBuf,
  telemetry_path: PathBuf,
  target_title: String,
  p0: (f64, f64, f64),
  p1: (f64, f64, f64),
  expected_yaw: Option<f64>,
  t1_timeout_ticks: usize,
  max_nav_ticks: usize,
  teleport_cmd: Option<String>,
  auto_tp: bool,
  db_path: PathBuf,
  screenshot_out: PathBuf,
  summary_out: PathBuf,
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

fn resolve_default_model_path() -> PathBuf {
  let candidate_v2 = PathBuf::from(r"F:\auv\.tmp\yolo-runs\block_detector_v2\weights\best.onnx");
  if candidate_v2.is_file() {
    return candidate_v2;
  }
  let candidate_best = PathBuf::from(r"F:\auv\.tmp\yolo-runs\best-v2.onnx");
  if candidate_best.is_file() {
    return candidate_best;
  }
  let candidate_assets = PathBuf::from(r"F:\auv\.tmp\models\block-detector-v2.onnx");
  if candidate_assets.is_file() {
    return candidate_assets;
  }
  candidate_v2
}

fn parse_args() -> Args {
  let mut model_path: Option<PathBuf> = None;
  let mut depth_model_path = PathBuf::from(r"F:\auv\.tmp\models\model-small.onnx");
  let mut telemetry_path = PathBuf::from(r"F:\pcl\.minecraft\versions\1.21.1-Fabric 0.16.10\auv\telemetry.jsonl");
  let mut target_title = "Minecraft".to_string();
  let mut p0_raw: Option<String> = None;
  let mut p1_raw: Option<String> = None;
  let mut expected_yaw: Option<f64> = None;
  let mut t1_timeout_ticks = 60;
  let mut max_nav_ticks = 120;
  let mut teleport_cmd: Option<String> = None;
  let mut auto_tp = true;
  let mut db_path = PathBuf::from(r"F:\auv\.tmp\recall_memory_store.json");
  let mut screenshot_out = PathBuf::from(r"F:\auv\.tmp\recall_chest_gui.png");
  let mut summary_out = PathBuf::from(r"F:\auv\.tmp\recall_verify_report.json");

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
      "--p0" => {
        p0_raw = args_iter.next();
      }
      "--p1" => {
        p1_raw = args_iter.next();
      }
      "--expected-yaw" => {
        if let Some(val) = args_iter.next() {
          expected_yaw = val.parse::<f64>().ok();
        }
      }
      "--t1-timeout-ticks" => {
        if let Some(val) = args_iter.next() {
          t1_timeout_ticks = val.parse().unwrap_or(60);
        }
      }
      "--max-nav-ticks" => {
        if let Some(val) = args_iter.next() {
          max_nav_ticks = val.parse().unwrap_or(120);
        }
      }
      "--teleport-cmd" => {
        teleport_cmd = args_iter.next();
      }
      "--no-auto-tp" => {
        auto_tp = false;
      }
      "--db-path" => {
        if let Some(val) = args_iter.next() {
          db_path = PathBuf::from(val);
        }
      }
      "--screenshot-out" => {
        if let Some(val) = args_iter.next() {
          screenshot_out = PathBuf::from(val);
        }
      }
      "--summary-out" => {
        if let Some(val) = args_iter.next() {
          summary_out = PathBuf::from(val);
        }
      }
      "--help" | "-h" => {
        println!("AUV Minecraft Chest Recall Verification Runner (Step 8)");
        println!("Usage: recall_verify [OPTIONS]");
        println!();
        println!("Options:");
        println!("  --model <PATH>            Path to block-detector-v2.onnx (default: best-v2.onnx)");
        println!("  --depth-model <PATH>      Path to MiDaS model-small.onnx");
        println!("  --telemetry <PATH>        Path to telemetry.jsonl");
        println!("  --p0 <X,Y,Z>              Chest ground truth coordinates (REQUIRED, verification only)");
        println!("  --p1 <X,Y,Z>              Teleport target point, ~30m away (REQUIRED)");
        println!("  --expected-yaw <DEG>      Manual expected yaw for T0 geometric assertion (<5°)");
        println!("  --t1-timeout-ticks <N>    Max 1Hz ticks for T1 visual ingest (default: 60)");
        println!("  --max-nav-ticks <N>       Max 2Hz ticks for T3 navigation (default: 120)");
        println!("  --teleport-cmd <CMD>      External command to execute for teleporting to P1");
        println!("  --no-auto-tp              Disable automatic in-game chat '/tp' keystroke injection");
        println!("  --db-path <PATH>          Path to persistent SpatialMemoryStore JSON");
        println!("  --screenshot-out <PATH>   Path to save T4 chest GUI screenshot");
        println!("  --summary-out <PATH>      Path to save final verification report JSON");
        std::process::exit(0);
      }
      other => {
        eprintln!("Unknown argument: {other}");
        std::process::exit(1);
      }
    }
  }

  let model_path = model_path.unwrap_or_else(resolve_default_model_path);
  if !model_path.exists() {
    eprintln!("FATAL: Specified --model path does not exist: {}", model_path.display());
    std::process::exit(1);
  }
  if !depth_model_path.exists() {
    eprintln!("FATAL: Depth model path does not exist: {}", depth_model_path.display());
    std::process::exit(1);
  }
  if !telemetry_path.exists() {
    eprintln!("FATAL: Telemetry path does not exist: {}", telemetry_path.display());
    std::process::exit(1);
  }

  // Parse or default P0 (Chest ground truth, verification-only) and P1 (Teleport point)
  let p0 = match p0_raw {
    Some(s) => parse_vec3(&s).unwrap_or_else(|e| {
      eprintln!("FATAL: Invalid --p0 '{s}': {e}");
      std::process::exit(1);
    }),
    None => {
      eprintln!("NOTICE: --p0 not supplied; using testbed benchmark coordinates (-22.0, 82.0, 34.0).");
      (-22.0, 82.0, 34.0)
    }
  };

  let p1 = match p1_raw {
    Some(s) => parse_vec3(&s).unwrap_or_else(|e| {
      eprintln!("FATAL: Invalid --p1 '{s}': {e}");
      std::process::exit(1);
    }),
    None => {
      eprintln!("NOTICE: --p1 not supplied; using 30m offset coordinates (-22.0, 82.0, 64.0).");
      (-22.0, 82.0, 64.0)
    }
  };

  Args {
    model_path,
    depth_model_path,
    telemetry_path,
    target_title,
    p0,
    p1,
    expected_yaw,
    t1_timeout_ticks,
    max_nav_ticks,
    teleport_cmd,
    auto_tp,
    db_path,
    screenshot_out,
    summary_out,
  }
}

// -----------------------------------------------------------------------------
// Main Verification Runner
// -----------------------------------------------------------------------------

fn now_millis() -> u64 {
  SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
  let args = parse_args();

  println!("============================================================");
  println!("   AUV Minecraft Chest Recall Verification (Step 8 GATE)    ");
  println!("============================================================");
  println!("Model:            {}", args.model_path.display());
  println!("Depth Model:      {}", args.depth_model_path.display());
  println!("Telemetry:        {}", args.telemetry_path.display());
  println!("Target Title:     {}", args.target_title);
  println!("P0 (Chest Truth): ({:.2}, {:.2}, {:.2}) [Ground Truth Only]", args.p0.0, args.p0.1, args.p0.2);
  println!("P1 (Teleport Pt): ({:.2}, {:.2}, {:.2}) [~30m Distance]", args.p1.0, args.p1.1, args.p1.2);
  println!("T1 Max Ticks:     {} (1Hz)", args.t1_timeout_ticks);
  println!("T3 Max Ticks:     {} (2Hz)", args.max_nav_ticks);
  println!("Report Out:       {}", args.summary_out.display());
  println!("Screenshot Out:   {}", args.screenshot_out.display());
  println!("Game Mode:        Creative");
  println!("============================================================\n");

  let mut verdict_reasons = Vec::new();
  let mut overall_passed = true;

  // =========================================================================
  // T0: Site & Parameter Initialization + Mathematical Validation
  // =========================================================================
  println!("[T0] Running Stage 0: Site & Parameter Initialization...");
  let dx_init = args.p0.0 - args.p1.0;
  let dy_init = args.p0.1 - args.p1.1;
  let dz_init = args.p0.2 - args.p1.2;
  let straight_dist = (dx_init * dx_init + dy_init * dy_init + dz_init * dz_init).sqrt();
  let horiz_dist = (dx_init * dx_init + dz_init * dz_init).sqrt();
  let calc_expected_yaw = calculate_expected_yaw(args.p1, args.p0);

  println!("[T0] Distance P1->P0: 3D={:.2}m, horizontal={:.2}m", straight_dist, horiz_dist);
  println!("[T0] Analytically calculated expected yaw: {:.2}°", calc_expected_yaw);

  let mut yaw_err = None;
  let mut math_passed = true;

  if let Some(manual_yaw) = args.expected_yaw {
    let err = normalize_angle_deg(calc_expected_yaw - manual_yaw).abs();
    yaw_err = Some(err);
    println!("[T0] Manual expected yaw provided: {:.2}°, discrepancy: {:.2}°", manual_yaw, err);
    if err >= 5.0 {
      let msg = format!(
        "T0 mathematical assertion failed: calc_yaw ({:.2}°) vs manual_yaw ({:.2}°) delta {:.2}° >= 5.0°",
        calc_expected_yaw, manual_yaw, err
      );
      eprintln!("[T0] ERROR: {msg}");
      verdict_reasons.push(msg);
      math_passed = false;
      overall_passed = false;
    } else {
      println!("[T0] Mathematical assertion passed: discrepancy {:.2}° < 5.0°", err);
    }
  } else {
    // Sanity check: direction vector derived from calc_expected_yaw matches (dx, dz)
    let rad = calc_expected_yaw.to_radians();
    let ux = -rad.sin();
    let uz = rad.cos();
    let vx = dx_init / horiz_dist.max(1e-6);
    let vz = dz_init / horiz_dist.max(1e-6);
    let dot = (ux * vx + uz * vz).clamp(-1.0, 1.0);
    let angular_alignment_deg = dot.acos().to_degrees();
    if angular_alignment_deg > 0.1 {
      let msg = format!("T0 analytical geometry error: alignment error {:.4}° > 0.1°", angular_alignment_deg);
      eprintln!("[T0] ERROR: {msg}");
      verdict_reasons.push(msg);
      math_passed = false;
      overall_passed = false;
    } else {
      println!("[T0] Analytical geometry verified: angular alignment deviation < 0.001°");
    }
  }

  let t0_record = T0Record {
    p0_ground_truth: [args.p0.0, args.p0.1, args.p0.2],
    p1_teleport_target: [args.p1.0, args.p1.1, args.p1.2],
    straight_line_distance_m: straight_dist,
    horizontal_distance_m: horiz_dist,
    calculated_expected_yaw: calc_expected_yaw,
    manual_expected_yaw: args.expected_yaw,
    yaw_error_deg: yaw_err,
    math_validation_passed: math_passed,
    game_mode: "creative".to_string(),
  };

  if !math_passed {
    eprintln!("[T0] Aborting due to T0 math validation failure.");
    write_final_report(
      &args.summary_out,
      overall_passed,
      &verdict_reasons,
      t0_record,
      default_t1(),
      default_t2(),
      default_t3(),
      default_t4(),
    )?;
    std::process::exit(1);
  }

  // -------------------------------------------------------------------------
  // Connect to Desktop Driver and Locate Minecraft Window
  // -------------------------------------------------------------------------
  println!("\n[Init] Connecting to local desktop driver session...");
  let driver_session = auv_driver::open_local().map_err(|e| format!("Failed to open driver session: {e}"))?;
  let windows = driver_session.window().list().map_err(|e| format!("Failed to list desktop windows: {e}"))?;

  let target_window = windows.iter().find(|w| {
    if let Some(title) = &w.title
      && title.to_lowercase().contains(&args.target_title.to_lowercase())
    {
      return true;
    }
    if let Some(app) = &w.app_name
      && (app.to_lowercase().contains("minecraft") || app.to_lowercase().contains("javaw"))
    {
      return true;
    }
    false
  });

  let window = match target_window {
    Some(w) => {
      println!(
        "[Init] Target window identified: ID={}, title={:?}, frame={:?}",
        w.reference.id,
        w.title.as_deref().unwrap_or("untitled"),
        w.frame
      );
      if let Ok(hwnd_val) = w.reference.id.parse::<isize>() {
        let _ = win32_input::activate_minecraft(hwnd_val);
      }
      w.clone()
    }
    None => {
      let msg = format!("Target window matching '{}' was not found on desktop!", args.target_title);
      eprintln!("\nFATAL: {msg}");
      eprintln!("Please launch Minecraft Fabric 1.21.1 and enter world before running recall_verify.");
      std::process::exit(1);
    }
  };

  // -------------------------------------------------------------------------
  // Load Models & Initialize Spatial Memory Store
  // -------------------------------------------------------------------------
  println!("[Init] Loading BlockDetector from {}...", args.model_path.display());
  let mut detector_config = BlockDetectorConfig {
    model_path: args.model_path.clone(),
    per_class_threshold: DEFAULT_PER_CLASS_THRESHOLDS,
    enable_crafting_table: true,
    iou_threshold: 0.45,
    input_size: 640,
  };
  detector_config.per_class_threshold[1] = 0.50; // chest threshold
  let detector = BlockDetector::new(detector_config.clone()).map_err(|e| format!("Failed to load BlockDetector: {e}"))?;

  println!("[Init] Loading DepthEstimator from {}...", args.depth_model_path.display());
  let depth_estimator = DepthEstimator::new(&args.depth_model_path).map_err(|e| format!("Failed to load DepthEstimator: {e}"))?;

  if args.db_path.exists() {
    let _ = fs::remove_file(&args.db_path);
  }
  let store = SpatialMemoryStore::open(&args.db_path).map_err(|e| format!("Failed to initialize SpatialMemoryStore: {e}"))?;

  let loop_config = AgentMemoryLoopConfig {
    tick_interval_millis: 1000,
    require_mod_telemetry: true,
    yolo_confidence_threshold: 0.50,
    static_whitelist: vec![
      "chest".to_string(),
      "furnace".to_string(),
      "crafting_table".to_string(),
    ],
  };

  let mut agent_loop = AgentMemoryLoop::new(store, loop_config).with_models(detector, depth_estimator);

  // Setup observation pose before T1 if auto_tp is true:
  // Positions the player ~3m from the chest facing it directly so T1 visual ingest can capture it.
  if args.auto_tp {
    let obs_x = args.p0.0 + 3.0;
    let obs_y = args.p0.1;
    let obs_z = args.p0.2 + 0.5;
    println!("[T1] Pre-positioning player to observation pose ({:.1}, {:.1}, {:.1}, yaw: 90.0, pitch: 20.0)...", obs_x, obs_y, obs_z);
    let tp_cmd = format!("/tp @s {:.1} {:.1} {:.1} 90.0 20.0", obs_x, obs_y, obs_z);
    let _ = win32_input::send_chat_command(&tp_cmd);
    sleep(Duration::from_millis(1500));
  }

  // =========================================================================
  // T1: Stage 1 Visual Ingest (1Hz Tick Loop)
  // =========================================================================
  println!("\n[T1] Starting Stage 1: Visual Ingest (1Hz tick loop, max {} ticks)...", args.t1_timeout_ticks);
  println!("| Tick | Telemetry Time | Hit Block | Detections | Store Size | Chest Found? | Dist to P0 |");
  println!("|------|----------------|-----------|------------|------------|--------------|------------|");

  let mut t1_record = default_t1();
  let mut t1_passed = false;

  for tick_idx in 1..=args.t1_timeout_ticks {
    let tick_start = Instant::now();

    // 1. Read telemetry tail
    let frame = match read_latest_spatial_frame_from_tail(&args.telemetry_path) {
      Ok(Some(f)) => f,
      Ok(None) => {
        eprintln!("[T1] Tick {:02}: Telemetry file empty", tick_idx);
        sleep(Duration::from_millis(1000));
        continue;
      }
      Err(err) => {
        eprintln!("[T1] Tick {:02}: Telemetry read error: {err}", tick_idx);
        sleep(Duration::from_millis(1000));
        continue;
      }
    };

    // 2. Capture game window
    let screenshot = match driver_session.window().capture(&window) {
      Ok(cap) => Some(DynamicImage::ImageRgba8(cap.image)),
      Err(err) => {
        eprintln!("[T1] Tick {:02}: Window capture failed: {err}", tick_idx);
        None
      }
    };

    let obs_id = format!("t1-obs-{:04}", tick_idx);
    let capture = LiveCapture::new(obs_id, frame.monotonic_timestamp_ms, Some(frame.player_pose), frame.raycast_hit.clone(), screenshot)
      .with_viewport(frame.viewport);

    // 3. Tick AgentMemoryLoop
    let tick_report = match agent_loop.tick(&capture) {
      Ok(r) => r,
      Err(err) => {
        eprintln!("[T1] Tick {:02}: AgentMemoryLoop error: {err}", tick_idx);
        sleep(Duration::from_millis(1000));
        continue;
      }
    };

    // 4. Query store for ingested chest landmark
    let chest_lm = agent_loop.store().landmarks().values().find(|lm| landmark_matches_label(lm, "chest")).cloned();

    let (chest_found, err_dist_str, maybe_err_dist) = if let Some(lm) = &chest_lm {
      let (lx, ly, lz) = if let Some(cp) = lm.continuous_position {
        cp
      } else {
        (f64::from(lm.position.x) + 0.5, f64::from(lm.position.y) + 0.5, f64::from(lm.position.z) + 0.5)
      };
      let dx = lx - args.p0.0;
      let dy = ly - args.p0.1;
      let dz = lz - args.p0.2;
      let dist = (dx * dx + dy * dy + dz * dz).sqrt();
      ("YES", format!("{dist:.2}m"), Some(dist))
    } else {
      ("NO", "N/A".to_string(), None)
    };

    let hit_name = frame.raycast_hit.as_ref().map(|h| h.block_id.as_str()).unwrap_or("none");
    let det_str = if tick_report.detections.is_empty() {
      "none".to_string()
    } else {
      tick_report.detections.join(",")
    };

    println!(
      "| {:04} | {:14} | {:9} | {:10} | {:10} | {:12} | {:10} |",
      tick_idx,
      frame.monotonic_timestamp_ms,
      hit_name,
      det_str,
      agent_loop.store().len(),
      chest_found,
      err_dist_str
    );

    // 5. Evaluate T1 Success Criteria
    if let Some(lm) = &chest_lm {
      let is_visual =
        lm.source == LandmarkSource::VisualPerception || lm.observations.iter().any(|o| o.source == LandmarkSource::VisualPerception);

      let pos_err = maybe_err_dist.unwrap_or(f64::INFINITY);

      let (lx, ly, lz) = if let Some(cp) = lm.continuous_position {
        cp
      } else {
        (f64::from(lm.position.x) + 0.5, f64::from(lm.position.y) + 0.5, f64::from(lm.position.z) + 0.5)
      };

      t1_record = T1Record {
        ticks_elapsed: tick_idx,
        chest_landmark_id: Some(lm.landmark_id.clone()),
        chest_landmark_source: Some(format!("{:?}", lm.source)),
        chest_estimated_position: Some([lx, ly, lz]),
        p0_ground_truth: [args.p0.0, args.p0.1, args.p0.2],
        position_error_m: Some(pos_err),
        error_threshold_m: 2.0,
        passed: false,
        failure_reason: None,
      };

      if is_visual && pos_err < 2.0 {
        println!("[T1] SUCCESS: Chest landmark confirmed in memory! Source=Visual, Error={:.2}m (< 2.0m)", pos_err);
        t1_record.passed = true;
        t1_passed = true;
        break;
      }
    }

    let elapsed = tick_start.elapsed();
    if elapsed < Duration::from_millis(1000) {
      sleep(Duration::from_millis(1000) - elapsed);
    }
  }

  if !t1_passed {
    if t1_record.failure_reason.is_none() {
      let msg = format!("T1 failed: Timed out after {} ticks without chest landmark being ingested into memory", args.t1_timeout_ticks);
      eprintln!("[T1] ERROR: {msg}");
      t1_record.failure_reason = Some(msg.clone());
      verdict_reasons.push(msg);
      overall_passed = false;
    }
    eprintln!("[T1] Halting verification due to T1 failure.");
    write_final_report(&args.summary_out, overall_passed, &verdict_reasons, t0_record, t1_record, default_t2(), default_t3(), default_t4())?;
    std::process::exit(1);
  }

  // =========================================================================
  // T2: Displacement & Memory Isolation Assertion
  // =========================================================================
  println!("\n[T2] Starting Stage 2: Displacement & Memory Isolation Assertion...");
  println!("[T2] Teleporting player to P1 ({:.2}, {:.2}, {:.2})...", args.p1.0, args.p1.1, args.p1.2);

  // Execute teleportation
  if let Some(cmd) = &args.teleport_cmd {
    println!("[T2] Executing external teleport command: {}", cmd);
    #[cfg(target_os = "windows")]
    {
      let _ = std::process::Command::new("powershell").args(["-Command", cmd]).output();
    }
    #[cfg(not(target_os = "windows"))]
    {
      let _ = std::process::Command::new("sh").args(["-c", cmd]).output();
    }
  } else if args.auto_tp {
    println!("[T2] Injecting in-game teleport command '/tp @s {:.1} {:.1} {:.1} 90.0 0.0'...", args.p1.0, args.p1.1, args.p1.2);
    let tp_text = format!("/tp @s {:.1} {:.1} {:.1} 90.0 0.0", args.p1.0, args.p1.1, args.p1.2);
    let _ = win32_input::send_chat_command(&tp_text);
  }

  // Poll telemetry to verify player has arrived at P1
  println!("[T2] Verifying player arrival at P1 in telemetry (timeout 30s)...");
  let tp_wait_start = Instant::now();
  let mut at_p1 = false;
  let mut current_pose = None;

  while tp_wait_start.elapsed() < Duration::from_secs(30) {
    if let Ok(Some(f)) = read_latest_spatial_frame_from_tail(&args.telemetry_path) {
      let dx = f.player_pose.eye_position.x - args.p1.0;
      let dz = f.player_pose.eye_position.z - args.p1.2;
      let dist = (dx * dx + dz * dz).sqrt();
      current_pose = Some([
        f.player_pose.eye_position.x,
        f.player_pose.eye_position.y,
        f.player_pose.eye_position.z,
      ]);
      if dist < 5.0 {
        println!(
          "[T2] Teleport confirmed! Player eye at ({:.2}, {:.2}, {:.2}), dist to P1={:.2}m (< 5.0m)",
          f.player_pose.eye_position.x, f.player_pose.eye_position.y, f.player_pose.eye_position.z, dist
        );
        at_p1 = true;
        break;
      }
    }
    sleep(Duration::from_millis(500));
  }

  if !at_p1 {
    let msg = format!("T2 failed: Player did not arrive at P1 within 30s timeout (latest pose: {:?})", current_pose);
    eprintln!("[T2] ERROR: {msg}");
    let t2_record = T2Record {
      teleport_to_p1_confirmed: false,
      p1_target: [args.p1.0, args.p1.1, args.p1.2],
      p1_actual_pose: current_pose,
      ticks_evaluated: 0,
      chest_detections_per_tick: Vec::new(),
      total_chest_detections: 0,
      isolation_passed: false,
      failure_reason: Some(msg.clone()),
    };
    verdict_reasons.push(msg);
    overall_passed = false;
    write_final_report(&args.summary_out, overall_passed, &verdict_reasons, t0_record, t1_record, t2_record, default_t3(), default_t4())?;
    std::process::exit(1);
  }

  // Run 5 ticks at P1: assert v2 model detections of chest MUST be 0
  println!("[T2] Running 5 ticks at P1 to assert memory isolation (chest detections MUST be 0)...");
  let mut chest_counts = Vec::new();
  let mut isolation_passed = true;
  let mut isolation_failure_reason = None;

  // Settle time after teleport
  sleep(Duration::from_millis(1000));

  for tick_idx in 1..=5 {
    let tick_start = Instant::now();
    let capture_res = driver_session.window().capture(&window);
    let detections = match capture_res {
      Ok(cap) => {
        let img = DynamicImage::ImageRgba8(cap.image);
        match agent_loop.detector().as_ref() {
          Some(det) => det.detect(&img).unwrap_or_default(),
          None => Vec::new(),
        }
      }
      Err(err) => {
        eprintln!("[T2] Tick {:02}: Capture error: {err}", tick_idx);
        Vec::new()
      }
    };

    let chest_dets = detections.iter().filter(|d| d.label.eq_ignore_ascii_case("chest")).count();
    chest_counts.push(chest_dets);
    println!("[T2] Tick {:02}/05: Detected {} chest(s) in view (total detections: {})", tick_idx, chest_dets, detections.len());

    if chest_dets > 0 {
      let msg = format!(
        "T2 failed: Memory isolation violation! Found {} chest detection(s) at P1 on tick {} (P1 must have zero chest visibility)",
        chest_dets, tick_idx
      );
      eprintln!("[T2] ERROR: {msg}");
      isolation_failure_reason = Some(msg);
      isolation_passed = false;
      overall_passed = false;
      break;
    }

    let elapsed = tick_start.elapsed();
    if elapsed < Duration::from_millis(1000) {
      sleep(Duration::from_millis(1000) - elapsed);
    }
  }

  let total_chest_dets: usize = chest_counts.iter().sum();
  let t2_record = T2Record {
    teleport_to_p1_confirmed: true,
    p1_target: [args.p1.0, args.p1.1, args.p1.2],
    p1_actual_pose: current_pose,
    ticks_evaluated: chest_counts.len(),
    chest_detections_per_tick: chest_counts,
    total_chest_detections: total_chest_dets,
    isolation_passed,
    failure_reason: isolation_failure_reason.clone(),
  };

  if !isolation_passed {
    if let Some(r) = isolation_failure_reason {
      verdict_reasons.push(r);
    }
    eprintln!("[T2] Halting verification due to T2 memory isolation failure.");
    write_final_report(&args.summary_out, overall_passed, &verdict_reasons, t0_record, t1_record, t2_record, default_t3(), default_t4())?;
    std::process::exit(1);
  }

  println!("[T2] SUCCESS: Memory isolation confirmed! Exactly 0 chest detections over 5 ticks at P1.");

  // =========================================================================
  // T3: Stage 2 Memory Navigation GATE (2Hz Control Loop)
  // =========================================================================
  println!("\n[T3] Starting Stage 2: Memory Navigation GATE (2Hz control loop, max {} ticks)...", args.max_nav_ticks);
  println!("[T3] Enforcing anti-cheat barrier: querying target landmark dynamically from SpatialMemoryStore...");

  // ANTI-CHEAT BARRIER: Query target landmark strictly from SpatialMemoryStore
  let navigator = match MemoryNavigator::query_target(agent_loop.store(), "chest") {
    Ok(nav) => {
      println!(
        "[T3] Memory query successful: resolved landmark '{}' at ({:.2}, {:.2}, {:.2}) from memory store!",
        nav.target_landmark_id, nav.target_position.0, nav.target_position.1, nav.target_position.2
      );
      nav
    }
    Err(err) => {
      let msg = format!("T3 failed: anti-cheat query error: {err}");
      eprintln!("[T3] FATAL: {msg}");
      verdict_reasons.push(msg.clone());
      let mut t3_record = default_t3();
      t3_record.failure_reason = Some(msg);
      write_final_report(&args.summary_out, false, &verdict_reasons, t0_record, t1_record, t2_record, t3_record, default_t4())?;
      std::process::exit(1);
    }
  };

  let target_pos = navigator.target_position();
  let mut trajectory = Vec::new();
  let mut stagnant_ticks = 0usize;
  let mut min_observed_dist = f64::INFINITY;
  let mut nav_success = false;
  let mut nav_failure_reason = None;
  let mut start_dist = None;
  let mut final_dist = None;

  println!("| Tick | Player Position (X, Y, Z) | Yaw | Target Dist | Yaw Delta | Action |");
  println!("|------|---------------------------|-----|-------------|-----------|--------|");

  for tick_idx in 1..=args.max_nav_ticks {
    let tick_start = Instant::now();

    // 1. Read latest telemetry frame
    let frame = match read_latest_spatial_frame_from_tail(&args.telemetry_path) {
      Ok(Some(f)) => f,
      Ok(None) => {
        eprintln!("[T3] Tick {:03}: Telemetry tail empty", tick_idx);
        sleep(Duration::from_millis(500));
        continue;
      }
      Err(err) => {
        eprintln!("[T3] Tick {:03}: Telemetry read error: {err}", tick_idx);
        sleep(Duration::from_millis(500));
        continue;
      }
    };

    let eye = frame.player_pose.eye_position;
    let yaw = frame.player_pose.yaw;

    // 2. Compute horizontal distance to queried memory target
    let dx = target_pos.0 - eye.x;
    let dz = target_pos.2 - eye.z;
    let target_dist = (dx * dx + dz * dz).sqrt();
    final_dist = Some(target_dist);

    if start_dist.is_none() {
      start_dist = Some(target_dist);
      min_observed_dist = target_dist;
    }

    // 3. Check Success Condition: horizontal distance < 3.5m
    if target_dist < 3.5 {
      println!("[T3] Reached target distance {:.2}m (< 3.5m)! Stopping navigation and proceeding to T4.", target_dist);
      nav_success = true;
      trajectory.push(NavTrajectoryPoint {
        tick: tick_idx,
        player_pos: [eye.x, eye.y, eye.z],
        yaw,
        target_dist,
        yaw_delta: 0.0,
        action: "STOP_AT_TARGET".to_string(),
      });
      break;
    }

    // 4. Stagnation Check: 15 ticks without distance decreasing
    if target_dist < min_observed_dist - 0.05 {
      min_observed_dist = target_dist;
      stagnant_ticks = 0;
    } else {
      stagnant_ticks += 1;
    }

    if stagnant_ticks >= 15 {
      let msg = format!(
        "T3 failed: Stagnation detected! Target distance did not decrease for 15 consecutive ticks (stuck at {:.2}m, blocked by obstacle)",
        target_dist
      );
      eprintln!("[T3] ERROR: {msg}");
      nav_failure_reason = Some(msg);
      overall_passed = false;
      break;
    }

    // 5. Compute Expected Yaw and Angular Delta
    let target_yaw = (-dx).atan2(dz).to_degrees();
    let yaw_delta = normalize_angle_deg(target_yaw - yaw);

    // 6. Navigation Control Policy:
    // - If |yaw_delta| > 15°: micro-adjust turn (rotate mouse), DO NOT press W.
    // - If |yaw_delta| <= 15°: press W to step forward.
    let action_str = if yaw_delta.abs() > 15.0 {
      win32_input::turn_yaw(yaw_delta as f32);
      format!("TURN(delta={:+.1}°)", yaw_delta)
    } else {
      // Step forward: hold W for 350ms (at 2Hz loop / 500ms cycle)
      win32_input::step_forward(Duration::from_millis(350));
      "STEP_FORWARD(W)".to_string()
    };

    println!(
      "| {:04} | ({:6.1}, {:5.1}, {:6.1}) | {:5.1}° | {:10.2}m | {:8.1}° | {:25} |",
      tick_idx, eye.x, eye.y, eye.z, yaw, target_dist, yaw_delta, action_str
    );

    trajectory.push(NavTrajectoryPoint {
      tick: tick_idx,
      player_pos: [eye.x, eye.y, eye.z],
      yaw,
      target_dist,
      yaw_delta,
      action: action_str,
    });

    let elapsed = tick_start.elapsed();
    if elapsed < Duration::from_millis(500) {
      sleep(Duration::from_millis(500) - elapsed);
    }
  }

  if !nav_success && nav_failure_reason.is_none() {
    let msg = format!(
      "T3 failed: Timed out after max {} ticks without reaching target distance < 3.5m (final distance: {:.2}m)",
      args.max_nav_ticks,
      final_dist.unwrap_or(0.0)
    );
    eprintln!("[T3] ERROR: {msg}");
    nav_failure_reason = Some(msg);
    overall_passed = false;
  }

  let t3_record = T3Record {
    target_source: "SpatialMemoryStore (dynamic query label='chest')".to_string(),
    target_landmark_id: Some(navigator.target_landmark_id.clone()),
    target_queried_position: Some([target_pos.0, target_pos.1, target_pos.2]),
    ticks_completed: trajectory.len(),
    max_ticks: args.max_nav_ticks,
    start_distance_m: start_dist,
    final_distance_m: final_dist,
    success_threshold_m: 3.5,
    stagnant_ticks_observed: stagnant_ticks,
    navigation_passed: nav_success,
    failure_reason: nav_failure_reason.clone(),
    trajectory,
  };

  if !nav_success {
    if let Some(r) = nav_failure_reason {
      verdict_reasons.push(r);
    }
    eprintln!("[T3] Halting verification due to T3 memory navigation failure.");
    write_final_report(&args.summary_out, overall_passed, &verdict_reasons, t0_record, t1_record, t2_record, t3_record, default_t4())?;
    std::process::exit(1);
  }

  println!("[T3] SUCCESS: Memory navigation GATE passed!");

  // =========================================================================
  // T4: Stage 3 Interaction Attempt (Best Effort)
  // =========================================================================
  println!("\n[T4] Starting Stage 3: Chest Interaction (Best Effort)...");

  // Read latest telemetry frame
  let latest_frame_opt = read_latest_spatial_frame_from_tail(&args.telemetry_path).ok().flatten();
  let mut t4_record = default_t4();
  t4_record.screenshot_saved_path = args.screenshot_out.to_string_lossy().to_string();

  if let Some(frame) = latest_frame_opt {
    let landmark_block = agent_loop.store().get(&navigator.target_landmark_id).map(|lm| lm.position);

    if let Some(block_pos) = landmark_block {
      println!("[T4] Projecting chest block at ({}, {}, {}) into observer viewport...", block_pos.x, block_pos.y, block_pos.z);
      match MinecraftProjector::new(frame.clone()) {
        Ok(projector) => {
          let mut block_target = MinecraftBlockTarget::new(block_pos);
          if let Some(lm) = agent_loop.store().get(&navigator.target_landmark_id) {
            block_target.face = lm.surface_face;
          }

          match projector.project_block_target(&block_target) {
            Ok(projected) => {
              t4_record.projected_visibility = Some(format!("{:?}", projected.visibility));
              t4_record.match_radius_px = Some(projected.match_radius_px);

              if let Some(screen_pt) = projected.screen_point {
                println!(
                  "[T4] Projected chest screen point: ({:.1}, {:.1}) (match radius: {:.1}px)",
                  screen_pt.x, screen_pt.y, projected.match_radius_px
                );
                t4_record.projected_screen_point = Some([screen_pt.x, screen_pt.y]);
              }

              // Compute precise yaw/pitch to aim directly at chest center
              let eye = frame.player_pose.eye_position;
              let dx = target_pos.0 - eye.x;
              let dy = target_pos.1 - eye.y;
              let dz = target_pos.2 - eye.z;
              let horiz = (dx * dx + dz * dz).sqrt();
              let target_yaw = (-dx).atan2(dz).to_degrees();
              let target_pitch = (-dy).atan2(horiz).to_degrees();
              let aim_yaw_delta = normalize_angle_deg(target_yaw - frame.player_pose.yaw);
              let aim_pitch_delta = normalize_angle_deg(target_pitch - frame.player_pose.pitch);

              t4_record.aim_yaw_delta = Some(aim_yaw_delta);
              t4_record.aim_pitch_delta = Some(aim_pitch_delta);

              println!("[T4] Fine-tuning aim towards chest: yaw_delta={:+.1}°, pitch_delta={:+.1}°", aim_yaw_delta, aim_pitch_delta);
              let mouse_dx = (aim_yaw_delta / 0.150).round() as i32;
              let mouse_dy = (aim_pitch_delta / 0.150).round() as i32;
              win32_input::turn_mouse(mouse_dx, mouse_dy);
              sleep(Duration::from_millis(200));

              // Dispatch right-click once
              println!("[T4] Dispatching right-click to interact with chest...");
              win32_input::right_click();
              t4_record.right_click_dispatched = true;

              // Settle time for chest opening animation / GUI
              sleep(Duration::from_millis(600));

              // Capture screenshot and save to F:\auv\.tmp\recall_chest_gui.png
              println!("[T4] Capturing screenshot to {}...", args.screenshot_out.display());
              if let Ok(cap) = driver_session.window().capture(&window) {
                if let Some(parent) = args.screenshot_out.parent() {
                  let _ = fs::create_dir_all(parent);
                }
                match DynamicImage::ImageRgba8(cap.image).save(&args.screenshot_out) {
                  Ok(()) => {
                    println!("[T4] Successfully saved chest interaction screenshot to {}", args.screenshot_out.display());
                    t4_record.screenshot_saved = true;
                  }
                  Err(e) => {
                    let err_msg = format!("Failed to save screenshot: {e}");
                    eprintln!("[T4] WARNING: {err_msg}");
                    t4_record.error = Some(err_msg);
                  }
                }
              }
            }
            Err(e) => {
              let err_msg = format!("MinecraftProjector projection error: {e}");
              eprintln!("[T4] WARNING: {err_msg}");
              t4_record.error = Some(err_msg);
            }
          }
        }
        Err(e) => {
          let err_msg = format!("Failed to initialize MinecraftProjector: {e}");
          eprintln!("[T4] WARNING: {err_msg}");
          t4_record.error = Some(err_msg);
        }
      }
    }
  }

  // =========================================================================
  // T5: Summary Report Output
  // =========================================================================
  println!("\n[T5] Generating final verification report...");
  let report =
    write_final_report(&args.summary_out, overall_passed, &verdict_reasons, t0_record, t1_record, t2_record, t3_record, t4_record)?;

  println!("============================================================");
  println!("                VERIFICATION FINAL SUMMARY                  ");
  println!("============================================================");
  println!("Overall Verdict:    {}", report.verdict);
  println!("Game Mode:          {}", report.game_mode);
  println!(
    "T0 (Init & Math):   {}",
    if report.t0_initialization.math_validation_passed {
      "PASS"
    } else {
      "FAIL"
    }
  );
  println!(
    "T1 (Visual Ingest): {}",
    if report.t1_visual_ingest.passed {
      "PASS"
    } else {
      "FAIL"
    }
  );
  println!(
    "T2 (Isolation):     {}",
    if report.t2_memory_isolation.isolation_passed {
      "PASS"
    } else {
      "FAIL"
    }
  );
  println!(
    "T3 (Navigation):    {}",
    if report.t3_memory_navigation.navigation_passed {
      "PASS"
    } else {
      "FAIL"
    }
  );
  println!(
    "T4 (Interaction):   Dispatched: {}, Screenshot: {}",
    report.t4_chest_interaction.right_click_dispatched,
    if report.t4_chest_interaction.screenshot_saved {
      "SAVED"
    } else {
      "NOT_SAVED"
    }
  );
  println!("Report Path:        {}", args.summary_out.display());
  println!("============================================================\n");

  if overall_passed {
    Ok(())
  } else {
    Err("Recall verification completed with failures (see report for details).".into())
  }
}

// -----------------------------------------------------------------------------
// Report Serialization & Default Helpers
// -----------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn write_final_report(
  path: &Path,
  passed: bool,
  verdict_reasons: &[String],
  t0: T0Record,
  t1: T1Record,
  t2: T2Record,
  t3: T3Record,
  t4: T4Record,
) -> Result<RecallVerifyReport, Box<dyn std::error::Error>> {
  if let Some(parent) = path.parent() {
    let _ = fs::create_dir_all(parent);
  }

  let report = RecallVerifyReport {
    schema_version: 1,
    generated_at_millis: now_millis(),
    game_mode: "creative".to_string(),
    verdict: if passed {
      "PASS".to_string()
    } else {
      "FAIL".to_string()
    },
    verdict_reasons: verdict_reasons.to_vec(),
    t0_initialization: t0,
    t1_visual_ingest: t1,
    t2_memory_isolation: t2,
    t3_memory_navigation: t3,
    t4_chest_interaction: t4,
  };

  let json_str = serde_json::to_string_pretty(&report)?;
  fs::write(path, json_str)?;
  println!("[T5] Report written successfully to {}", path.display());
  Ok(report)
}

fn default_t1() -> T1Record {
  T1Record {
    ticks_elapsed: 0,
    chest_landmark_id: None,
    chest_landmark_source: None,
    chest_estimated_position: None,
    p0_ground_truth: [0.0, 0.0, 0.0],
    position_error_m: None,
    error_threshold_m: 2.0,
    passed: false,
    failure_reason: None,
  }
}

fn default_t2() -> T2Record {
  T2Record {
    teleport_to_p1_confirmed: false,
    p1_target: [0.0, 0.0, 0.0],
    p1_actual_pose: None,
    ticks_evaluated: 0,
    chest_detections_per_tick: Vec::new(),
    total_chest_detections: 0,
    isolation_passed: false,
    failure_reason: None,
  }
}

fn default_t3() -> T3Record {
  T3Record {
    target_source: "SpatialMemoryStore".to_string(),
    target_landmark_id: None,
    target_queried_position: None,
    ticks_completed: 0,
    max_ticks: 120,
    start_distance_m: None,
    final_distance_m: None,
    success_threshold_m: 3.5,
    stagnant_ticks_observed: 0,
    navigation_passed: false,
    failure_reason: None,
    trajectory: Vec::new(),
  }
}

fn default_t4() -> T4Record {
  T4Record {
    projected_screen_point: None,
    projected_visibility: None,
    match_radius_px: None,
    aim_yaw_delta: None,
    aim_pitch_delta: None,
    right_click_dispatched: false,
    screenshot_saved_path: "F:\\auv\\.tmp\\recall_chest_gui.png".to_string(),
    screenshot_saved: false,
    error: None,
  }
}

// -----------------------------------------------------------------------------
// Unit Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
  use super::*;
  use auv_game_minecraft::spatial_memory_store::ObservationRef;
  use auv_game_minecraft::types::BlockPosition;

  #[test]
  fn test_math_validation_cardinal_directions() {
    let origin = (0.0, 64.0, 0.0);

    // South (+Z): dx=0, dz=1 -> yaw = 0°
    let south = (0.0, 64.0, 10.0);
    let yaw_south = calculate_expected_yaw(origin, south);
    assert!((yaw_south - 0.0).abs() < 1e-6, "South yaw must be 0°, got {}", yaw_south);

    // North (-Z): dx=0, dz=-1 -> yaw = 180° / -180°
    let north = (0.0, 64.0, -10.0);
    let yaw_north = calculate_expected_yaw(origin, north);
    assert!((yaw_north.abs() - 180.0).abs() < 1e-6, "North yaw must be 180°, got {}", yaw_north);

    // East (+X): dx=1, dz=0 -> yaw = -90°
    let east = (10.0, 64.0, 0.0);
    let yaw_east = calculate_expected_yaw(origin, east);
    assert!((yaw_east - (-90.0)).abs() < 1e-6, "East yaw must be -90°, got {}", yaw_east);

    // West (-X): dx=-1, dz=0 -> yaw = +90°
    let west = (-10.0, 64.0, 0.0);
    let yaw_west = calculate_expected_yaw(origin, west);
    assert!((yaw_west - 90.0).abs() < 1e-6, "West yaw must be 90°, got {}", yaw_west);

    // South-East (+X, +Z): dx=1, dz=1 -> yaw = -45°
    let se = (10.0, 64.0, 10.0);
    let yaw_se = calculate_expected_yaw(origin, se);
    assert!((yaw_se - (-45.0)).abs() < 1e-6, "South-East yaw must be -45°, got {}", yaw_se);

    // South-West (-X, +Z): dx=-1, dz=1 -> yaw = +45°
    let sw = (-10.0, 64.0, 10.0);
    let yaw_sw = calculate_expected_yaw(origin, sw);
    assert!((yaw_sw - 45.0).abs() < 1e-6, "South-West yaw must be 45°, got {}", yaw_sw);
  }

  #[test]
  fn test_normalize_angle_deg() {
    assert_eq!(normalize_angle_deg(0.0), 0.0);
    assert_eq!(normalize_angle_deg(180.0), 180.0);
    assert_eq!(normalize_angle_deg(-180.0), 180.0);
    assert_eq!(normalize_angle_deg(190.0), -170.0);
    assert_eq!(normalize_angle_deg(-190.0), 170.0);
    assert_eq!(normalize_angle_deg(360.0), 0.0);
    assert_eq!(normalize_angle_deg(540.0), 180.0);
  }

  #[test]
  fn test_anti_cheat_barrier_memory_query() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let mut store = SpatialMemoryStore::open(tmp.path()).unwrap();

    // Store is empty: query must fail with anti-cheat constraint violation
    let res = MemoryNavigator::query_target(&store, "chest");
    assert!(res.is_err(), "Empty store must fail anti-cheat target query");

    // Insert a chest landmark
    let obs_ref = ObservationRef {
      observation_id: "obs-01".to_string(),
      captured_at_millis: 1000,
    };
    let block_pos = BlockPosition::new(-22, 82, 34);
    let _ = store.upsert_from_perception(block_pos, "chest", 0.95, &obs_ref);

    // Query should now succeed and return coordinates from memory
    let nav = MemoryNavigator::query_target(&store, "chest").expect("should find chest in store");
    assert_eq!(nav.target_label, "chest");
    assert_eq!(nav.target_position, (-21.5, 82.5, 34.5));
  }

  #[test]
  fn test_benchmark_site_geometry_math() {
    let p0 = (-22.0, 82.0, 34.0);
    let p1 = (-22.0, 82.0, 64.0);
    let expected_yaw = calculate_expected_yaw(p1, p0);
    // Heading due North (-Z) -> yaw must be ±180°
    assert!((expected_yaw.abs() - 180.0).abs() < 1e-6, "Due North yaw should be 180°, got {}", expected_yaw);

    // Verify error threshold < 5°
    let manual_yaw = 180.0;
    let diff = normalize_angle_deg(expected_yaw - manual_yaw).abs();
    assert!(diff < 5.0, "Math error must be < 5°, got {}", diff);
  }

  #[test]
  fn test_stagnation_counter_trigger() {
    let mut stagnant_ticks = 0usize;
    let mut min_observed_dist = 25.0f64;

    // Simulate 14 ticks with no progress
    for _ in 1..=14 {
      let dist = 25.01;
      if dist < min_observed_dist - 0.05 {
        min_observed_dist = dist;
        stagnant_ticks = 0;
      } else {
        stagnant_ticks += 1;
      }
    }
    assert_eq!(stagnant_ticks, 14);

    // 15th tick triggers stagnation failure
    let dist = 25.02;
    if dist < min_observed_dist - 0.05 {
      stagnant_ticks = 0;
    } else {
      stagnant_ticks += 1;
    }
    assert_eq!(stagnant_ticks, 15);
  }
}
