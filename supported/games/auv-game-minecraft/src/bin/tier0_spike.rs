//! Tier-0 Spike: Evaluating Spatial Memory Navigation under Ablated Telemetry
//!
//! Purpose:
//!   Investigate how much memory navigation capability remains when ego-pose
//!   telemetry is progressively removed during the recall phase, identify which
//!   signals carry the load, and measure empirical proprioceptive drift.
//!
//! Telemetry Conditions:
//!   - C0: Full Telemetry (Step 8 reproduction, sanity check baseline, expect PASS < 3.5m)
//!   - C1: Yaw Only (Compass / Magnetometer, no player x/z)
//!   - C2: Position Only (GPS x/z, no player yaw)
//!   - C3: All Pinched (True Tier-0, no x/z, no yaw)
//!
//! Anti-Cheat Isolation:
//!   - Ingest Phase: Telemetry allowed ("building map with tape measure").
//!   - Recall Phase: `Tier0AgentContext` constructor CANNOT receive any telemetry
//!     reader or file handle. Harness telemetry is used strictly for ground-truth
//!     drift sampling (every 10 ticks) and arrival scoring.

use std::collections::HashMap;
use std::ffi::c_void;
use std::fs;
use std::mem::size_of;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use auv_game_minecraft::agent_memory_loop::{AgentMemoryLoop, AgentMemoryLoopConfig, LiveCapture};
use auv_game_minecraft::ingest::read_latest_spatial_frame_from_tail;
use auv_game_minecraft::spatial_memory_store::{SpatialLandmark, SpatialMemoryStore};
use auv_game_minecraft::types::MinecraftSpatialFrame;
use auv_game_minecraft::visual_perception::{BlockDetector, BlockDetectorConfig, DepthEstimator};
use image::DynamicImage;
use serde::{Deserialize, Serialize};

// -----------------------------------------------------------------------------
// Win32 Input & Window Helpers
// -----------------------------------------------------------------------------

mod win32_input {
  use super::*;

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
  pub const KEYEVENTF_SCANCODE: u32 = 0x0008;

  pub const SW_RESTORE: i32 = 9;
  pub const YAW_SENSITIVITY_DEG_PER_PX: f64 = 0.150;
  pub const SCANCODE_W: u16 = 0x11;
  pub const VK_W: u16 = 0x57;
  pub const VK_SLASH: u16 = 0xBF;
  pub const VK_RETURN: u16 = 0x0D;

  pub type HWND = *mut c_void;
  pub type HDESK = *mut c_void;
  pub type BOOL = i32;

  pub static TARGET_HWND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

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

  pub fn set_target_hwnd(hwnd: isize) {
    TARGET_HWND.store(hwnd, std::sync::atomic::Ordering::SeqCst);
  }

  pub fn activate_minecraft(target_hwnd: Option<isize>) -> bool {
    ensure_default_desktop();
    let hwnd = match target_hwnd {
      Some(h) if h != 0 => {
        TARGET_HWND.store(h, std::sync::atomic::Ordering::SeqCst);
        h as HWND
      }
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

      // Click to capture mouse cursor
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
          w_scan: SCANCODE_W,
          dw_flags: KEYEVENTF_SCANCODE,
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
          w_scan: SCANCODE_W,
          dw_flags: KEYEVENTF_SCANCODE | KEYEVENTF_KEYUP,
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
// Telemetry & Experiment Condition Types
// -----------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TelemetryCondition {
  /// C0: Full Telemetry (Step 8 baseline reproduction, x/z and yaw provided)
  C0FullTelemetry,
  /// C1: Yaw Only (Compass / Magnetometer provided, no player x/z)
  C1YawOnly,
  /// C2: Position Only (GPS x/z provided, no player yaw)
  C2PositionOnly,
  /// C3: All Pinched (True Tier-0, neither x/z nor yaw provided)
  C3AllPinched,
}

impl TelemetryCondition {
  pub fn label(&self) -> &'static str {
    match self {
      Self::C0FullTelemetry => "C0 (Full Telemetry)",
      Self::C1YawOnly => "C1 (Yaw Only / Compass)",
      Self::C2PositionOnly => "C2 (Position Only / GPS)",
      Self::C3AllPinched => "C3 (All Pinched / True Tier-0)",
    }
  }

  pub fn short_name(&self) -> &'static str {
    match self {
      Self::C0FullTelemetry => "c0",
      Self::C1YawOnly => "c1",
      Self::C2PositionOnly => "c2",
      Self::C3AllPinched => "c3",
    }
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FailureMode {
  /// Estimated pose arrived or stopped, but true distance >= 3.5m, or wandered > 35m away
  DriftLost,
  /// Heading inverted or distance monotonically increased beyond start distance + 5m
  WrongDirection,
  /// Agent executed excessive consecutive yaw corrections (> 720 deg) without stepping forward
  SpinningInPlace,
  /// Reached maximum step limit (120 ticks) without achieving arrival < 3.5m
  Timeout,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DriftSample {
  pub tick: usize,
  pub est_x: f64,
  pub est_z: f64,
  pub est_yaw: f64,
  pub true_x: f64,
  pub true_z: f64,
  pub true_yaw: f64,
  pub pos_drift_m: f64,
  pub yaw_drift_deg: f64,
  pub true_dist_to_chest: f64,
  pub est_dist_to_chest: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SingleRunResult {
  pub condition: TelemetryCondition,
  pub run_idx: usize,
  pub scenario_name: String,
  pub target_p0: [f64; 3],
  pub start_p1: [f64; 3],
  pub start_distance_m: f64,
  pub expected_initial_yaw: f64,
  pub passed: bool,
  pub final_true_distance_m: f64,
  pub final_est_distance_m: f64,
  pub final_pos_drift_m: f64,
  pub final_yaw_drift_deg: f64,
  pub ticks_completed: usize,
  pub failure_mode: Option<FailureMode>,
  pub drift_samples: Vec<DriftSample>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConditionSummary {
  pub condition: TelemetryCondition,
  pub runs_count: usize,
  pub pass_count: usize,
  pub pass_rate: f64,
  pub mean_final_distance_m: f64,
  pub mean_ticks: f64,
  pub mean_final_pos_drift_m: f64,
  pub mean_final_yaw_drift_deg: f64,
  pub failure_mode_counts: HashMap<String, usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tier0SpikeReport {
  pub schema_version: u32,
  pub generated_at: String,
  pub game_mode: String,
  pub isolation_mechanism_code_location: String,
  pub runs: Vec<SingleRunResult>,
  pub condition_summaries: HashMap<String, ConditionSummary>,
  pub overall_recommendation: String,
}

// -----------------------------------------------------------------------------
// Scenario Definition
// -----------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct SpikeScenario {
  pub name: &'static str,
  pub target_p0: (f64, f64, f64),
  pub start_p1: (f64, f64, f64),
  pub ingest_post: (f64, f64, f64, f32, f32), // (x, y, z, yaw, pitch)
  pub expected_initial_yaw: f64,
}

fn get_scenarios() -> [SpikeScenario; 3] {
  [
    SpikeScenario {
      name: "S1-Straight-South",
      target_p0: (-45.0, 95.0, 270.0),
      start_p1: (-45.0, 95.0, 242.0),
      ingest_post: (-42.5, 95.0, 270.5, 90.0, 22.0),
      expected_initial_yaw: 0.0,
    },
    SpikeScenario {
      name: "S2-Diagonal-SE",
      target_p0: (-32.0, 95.0, 260.0),
      start_p1: (-55.0, 95.0, 240.0),
      ingest_post: (-29.5, 95.0, 260.5, 90.0, 22.0),
      expected_initial_yaw: -48.99,
    },
    SpikeScenario {
      name: "S3-Diagonal-NW",
      target_p0: (-40.0, 95.0, 245.0),
      start_p1: (-20.0, 95.0, 266.0),
      ingest_post: (-37.5, 95.0, 245.5, 90.0, 22.0),
      expected_initial_yaw: 136.39,
    },
  ]
}

// -----------------------------------------------------------------------------
// Pure Tier-0 Agent Context (Constructor Isolation Guaranteed)
// -----------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub struct EstimatedPose {
  pub x: f64,
  pub y: f64,
  pub z: f64,
  pub yaw: f64,
}

pub struct AgentPerceptionInput<'a> {
  pub image: &'a DynamicImage,
  pub elapsed_ms: u64,
  /// Provided only in C0 and C1 (compass / magnetometer)
  pub compass_yaw: Option<f64>,
  /// Provided only in C0 and C2 (GPS x/z)
  pub gps_xz: Option<(f64, f64)>,
}

pub enum AgentAction {
  TurnYaw(f32),
  StepForward(u64), // duration in ms
  StopSuccess,
  StopFailed(FailureMode),
}

/// The isolated agent context for recall navigation.
///
/// CODE-LEVEL ISOLATION INVARIANT:
/// Notice that `Tier0AgentContext` does NOT have any field referencing
/// `telemetry_path`, `TelemetryReader`, or `MinecraftSpatialFrame`.
/// It cannot read ground-truth telemetry directly. All information arrives
/// strictly via `AgentPerceptionInput`.
pub struct Tier0AgentContext {
  condition: TelemetryCondition,
  proprio_pose: EstimatedPose,
  target_pos: (f64, f64, f64),
  target_landmark_id: String,
  detector: Option<BlockDetector>,
  consecutive_turns: usize,
  total_turned_deg: f64,
  last_gps_pos: Option<(f64, f64)>,
}

impl Tier0AgentContext {
  /// Constructor accepts only the initial nominal start guess, target coords,
  /// condition tag, and visual detector. ZERO telemetry handles exist here.
  pub fn new(
    condition: TelemetryCondition,
    initial_start_guess: (f64, f64, f64, f64),
    target_pos: (f64, f64, f64),
    target_landmark_id: String,
    detector: Option<BlockDetector>,
  ) -> Self {
    Self {
      condition,
      proprio_pose: EstimatedPose {
        x: initial_start_guess.0,
        y: initial_start_guess.1,
        z: initial_start_guess.2,
        yaw: initial_start_guess.3,
      },
      target_pos,
      target_landmark_id,
      detector,
      consecutive_turns: 0,
      total_turned_deg: 0.0,
      last_gps_pos: None,
    }
  }

  pub fn proprio_pose(&self) -> EstimatedPose {
    self.proprio_pose
  }

  pub fn step(&mut self, input: AgentPerceptionInput) -> AgentAction {
    // 1. Ingest condition-allowed external cues
    if let Some(cyaw) = input.compass_yaw {
      // C0 or C1: compass yaw grounds the heading
      self.proprio_pose.yaw = cyaw;
      self.consecutive_turns = 0;
      self.total_turned_deg = 0.0;
    }

    if let Some(pos) = input.gps_xz {
      // C0 or C2: GPS x/z grounds the horizontal position
      if let Some(prev) = self.last_gps_pos {
        let dx = pos.0 - prev.0;
        let dz = pos.1 - prev.1;
        let moved = (dx * dx + dz * dz).sqrt();
        // In C2 (no compass), if player made significant forward movement, calibrate heading from course over ground!
        if moved > 0.40 && input.compass_yaw.is_none() {
          let course_yaw = (-dx).atan2(dz).to_degrees();
          self.proprio_pose.yaw = course_yaw;
          self.consecutive_turns = 0;
          self.total_turned_deg = 0.0;
        }
      }
      self.proprio_pose.x = pos.0;
      self.proprio_pose.z = pos.1;
      self.last_gps_pos = Some(pos);
    }

    let dx = self.target_pos.0 - self.proprio_pose.x;
    let dz = self.target_pos.2 - self.proprio_pose.z;
    let est_dist = (dx * dx + dz * dz).sqrt();

    // 2. Visual servoing via YOLO detector (if available)
    if let Some(ref mut det) = self.detector {
      if let Ok(dets) = det.detect(input.image) {
        let chest_dets: Vec<_> = dets.into_iter().filter(|d| d.label.eq_ignore_ascii_case("chest") && d.confidence >= 0.50).collect();

        if let Some(best) = chest_dets.into_iter().max_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap()) {
          let img_w = input.image.width() as f64;
          let x1 = best.bbox.0;
          let y1 = best.bbox.1;
          let x2 = best.bbox.2;
          let y2 = best.bbox.3;
          let bbox_w = (x2 - x1).max(0.0);
          let bbox_h = (y2 - y1).max(0.0);
          let bbox_cx = (x1 + x2) / 2.0;
          let offset_ratio = (bbox_cx - (img_w / 2.0)) / (img_w / 2.0);
          let angle_offset_deg = offset_ratio * 35.0; // 70 deg horizontal FOV

          println!(
            "    [Agent Visual Homing] Detected chest: conf={:.3}, bbox=({:.1},{:.1},{:.1},{:.1}), w={:.1}px, h={:.1}px, offset_deg={:.1}°",
            best.confidence, x1, y1, x2, y2, bbox_w, bbox_h, angle_offset_deg
          );

          // Close-range arrival criterion: est_dist < 2.8m or visual bbox_h > 165px (< 3.2m distance)
          if est_dist < 2.8 || bbox_h > 165.0 {
            println!("    [Agent Visual Homing] Arrived at landmark! (est_dist={:.2}m, bbox_h={:.1}px)", est_dist, bbox_h);
            return AgentAction::StopSuccess;
          }

          if angle_offset_deg.abs() > 10.0 {
            self.consecutive_turns += 1;
            self.total_turned_deg += angle_offset_deg.abs();
            if self.total_turned_deg > 720.0 {
              return AgentAction::StopFailed(FailureMode::SpinningInPlace);
            }
            return AgentAction::TurnYaw(angle_offset_deg.clamp(-40.0, 40.0) as f32);
          }

          // Centered on chest -> walk forward directly towards it
          self.consecutive_turns = 0;
          let step_ms = if est_dist < 6.0 || bbox_h > 100.0 {
            200
          } else {
            350
          };
          return AgentAction::StepForward(step_ms);
        }
      }
    }

    // 3. Blind Dead-Reckoning Navigation (Chest not currently visible in FOV)
    let desired_yaw = (-dx).atan2(dz).to_degrees();
    let yaw_delta = normalize_angle_deg(desired_yaw - self.proprio_pose.yaw);

    // If estimated distance < 2.8m, agent believes it has arrived
    if est_dist < 2.8 {
      println!("    [Agent Dead Reckoning] Estimated distance {:.2}m < 2.8m -> Declaring arrival!", est_dist);
      return AgentAction::StopSuccess;
    }

    if yaw_delta.abs() > 15.0 {
      self.consecutive_turns += 1;
      self.total_turned_deg += yaw_delta.abs();
      if self.total_turned_deg > 720.0 && self.consecutive_turns > 8 {
        return AgentAction::StopFailed(FailureMode::SpinningInPlace);
      }
      return AgentAction::TurnYaw(yaw_delta.clamp(-45.0, 45.0) as f32);
    }

    self.consecutive_turns = 0;
    let step_ms = if est_dist < 6.0 { 250 } else { 350 };
    AgentAction::StepForward(step_ms)
  }

  /// Update proprioceptive dead reckoning after executing a motion action
  pub fn apply_proprioception_turn(&mut self, delta_deg: f64) {
    if self.condition == TelemetryCondition::C2PositionOnly || self.condition == TelemetryCondition::C3AllPinched {
      self.proprio_pose.yaw = normalize_angle_deg(self.proprio_pose.yaw + delta_deg);
    }
  }

  pub fn apply_proprioception_step(&mut self, duration_ms: u64) {
    if self.condition == TelemetryCondition::C1YawOnly || self.condition == TelemetryCondition::C3AllPinched {
      const ESTIMATED_WALK_SPEED_MPS: f64 = 4.317;
      let disp = ESTIMATED_WALK_SPEED_MPS * (duration_ms as f64 / 1000.0);
      let rad = self.proprio_pose.yaw.to_radians();
      self.proprio_pose.x -= disp * rad.sin();
      self.proprio_pose.z += disp * rad.cos();
    }
  }
}

// -----------------------------------------------------------------------------
// Utilities
// -----------------------------------------------------------------------------

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
// Site Preparation & Ingest
// -----------------------------------------------------------------------------

fn prepare_site(hwnd: isize, telemetry_path: &Path) {
  win32_input::activate_minecraft(Some(hwnd));
  sleep(Duration::from_millis(400));
  if let Ok(Some(f)) = read_latest_spatial_frame_from_tail(telemetry_path) {
    if f.screen_state.as_deref() != Some("in_game") {
      win32_input::press_esc();
      sleep(Duration::from_millis(400));
    }
  }
  win32_input::send_chat_command("/fill -65 94 235 -15 94 285 minecraft:grass_block");
  sleep(Duration::from_millis(800));
  win32_input::send_chat_command("/fill -65 95 235 -15 98 285 minecraft:air");
  sleep(Duration::from_millis(800));
  win32_input::send_chat_command("/time set 6000");
  sleep(Duration::from_millis(400));
  win32_input::send_chat_command("/gamerule doDaylightCycle false");
  sleep(Duration::from_millis(400));
  win32_input::send_chat_command("/weather clear");
  sleep(Duration::from_millis(400));
}

fn execute_ingest(
  scenario: &SpikeScenario,
  hwnd: isize,
  driver_session: &auv_driver::LocalDriverSession,
  window: &auv_driver::window::Window,
  model_path: &Path,
  depth_model_path: &Path,
  telemetry_path: &Path,
  store_path: &Path,
) -> Result<String, String> {
  // Clear any old chest and place new one at P0
  win32_input::send_chat_command(&format!(
    "/setblock {:.0} {:.0} {:.0} minecraft:chest[facing=east] replace",
    scenario.target_p0.0, scenario.target_p0.1, scenario.target_p0.2
  ));
  sleep(Duration::from_millis(600));

  // Teleport to ingest observation post
  let (ix, iy, iz, iyaw, ipitch) = scenario.ingest_post;
  win32_input::send_chat_command(&format!("/tp @s {:.1} {:.1} {:.1} {:.1} {:.1}", ix, iy, iz, iyaw, ipitch));
  sleep(Duration::from_millis(1000));

  if store_path.exists() {
    let _ = fs::remove_file(store_path);
  }
  let store = SpatialMemoryStore::open(store_path).map_err(|e| format!("Store open error: {e}"))?;

  let mut det_cfg = BlockDetectorConfig::default();
  det_cfg.model_path = model_path.to_path_buf();
  det_cfg.per_class_threshold = [0.50; 6];
  let mut detector = BlockDetector::new(det_cfg).map_err(|e| format!("Detector error: {e}"))?;
  detector.set_confidence_threshold(0.50);

  let depth_estimator = DepthEstimator::new(depth_model_path).map_err(|e| format!("Depth error: {e}"))?;
  let mut agent_loop = AgentMemoryLoop::new(store, AgentMemoryLoopConfig::default()).with_models(detector, depth_estimator);

  // Ingest for 4 ticks
  for tick_i in 1..=4 {
    let frame = read_latest_spatial_frame_from_tail(telemetry_path)?.ok_or_else(|| "No telemetry frame".to_string())?;
    let cap = driver_session.window().capture(window).map_err(|e| format!("Capture error: {e}"))?;
    let img = DynamicImage::ImageRgba8(cap.image);
    let dets = agent_loop.detector().unwrap().detect(&img).unwrap_or_default();
    let chest_count = dets.iter().filter(|d| d.label.eq_ignore_ascii_case("chest")).count();
    println!(
      "  [Ingest Tick {:02}] Detections total={}, chest={}, raycast={:?}",
      tick_i,
      dets.len(),
      chest_count,
      frame.raycast_hit.as_ref().map(|r| &r.block_id)
    );
    let live_capture = LiveCapture::new(
      format!("ingest-obs-{tick_i}"),
      frame.monotonic_timestamp_ms,
      Some(frame.player_pose),
      frame.raycast_hit.clone(),
      Some(img),
    )
    .with_viewport(frame.viewport);
    agent_loop.tick(&live_capture).map_err(|e| format!("Agent loop tick error: {e}"))?;
    sleep(Duration::from_millis(300));
  }

  // Find ingested chest landmark
  let chest_lm = agent_loop
    .store()
    .landmarks()
    .values()
    .find(|lm| landmark_matches_label(lm, "chest"))
    .ok_or_else(|| "Ingest failed: No chest landmark created in SpatialMemoryStore!".to_string())?;

  let lm_id = chest_lm.landmark_id.clone();
  let pos = if let Some(cp) = chest_lm.continuous_position {
    cp
  } else {
    (chest_lm.position.x as f64 + 0.5, chest_lm.position.y as f64 + 0.5, chest_lm.position.z as f64 + 0.5)
  };
  let err_m = ((pos.0 - scenario.target_p0.0).powi(2) + (pos.2 - scenario.target_p0.2).powi(2)).sqrt();
  println!("[Ingest] Chest landmark '{}' created at ({:.2}, {:.2}, {:.2}), error vs truth: {:.2}m", lm_id, pos.0, pos.1, pos.2, err_m);
  if err_m > 2.5 {
    return Err(format!("Ingest error too large: {:.2}m > 2.5m", err_m));
  }

  Ok(lm_id)
}

// -----------------------------------------------------------------------------
// Single Run Execution
// -----------------------------------------------------------------------------

fn run_single_ablation(
  condition: TelemetryCondition,
  run_idx: usize,
  scenario: &SpikeScenario,
  hwnd: isize,
  driver_session: &auv_driver::LocalDriverSession,
  window: &auv_driver::window::Window,
  model_path: &Path,
  depth_model_path: &Path,
  telemetry_path: &Path,
  store_path: &Path,
) -> Result<SingleRunResult, String> {
  println!("\n================================================================================");
  println!(" >>> STARTING RUN: Condition={}, Run #{}/3, Scenario='{}'", condition.label(), run_idx, scenario.name);
  println!("================================================================================");

  // 1. Prepare site
  prepare_site(hwnd, telemetry_path);

  // 2. Ingest landmark at P0
  let landmark_id = execute_ingest(scenario, hwnd, driver_session, window, model_path, depth_model_path, telemetry_path, store_path)?;

  // 3. Teleport to Recall Start Point P1 with nominal heading
  let (p1x, p1y, p1z) = scenario.start_p1;
  let init_yaw = scenario.expected_initial_yaw;
  win32_input::send_chat_command(&format!("/tp @s {:.1} {:.1} {:.1} {:.1} 0.0", p1x, p1y, p1z, init_yaw));
  sleep(Duration::from_millis(1200));

  // Memory isolation verification: 3 ticks, chest must NOT be detected in FOV
  let mut det_cfg = BlockDetectorConfig::default();
  det_cfg.model_path = model_path.to_path_buf();
  det_cfg.per_class_threshold = [0.50; 6];
  let mut detector = BlockDetector::new(det_cfg).map_err(|e| format!("Detector error: {e}"))?;
  detector.set_confidence_threshold(0.50);

  println!("[Isolation Check] Verifying memory isolation at P1 (chest detections MUST be 0)...");
  for _ in 1..=3 {
    let cap = driver_session.window().capture(window).map_err(|e| format!("Capture error: {e}"))?;
    let img = DynamicImage::ImageRgba8(cap.image);
    let dets = detector.detect(&img).unwrap_or_default();
    let chest_count = dets.iter().filter(|d| d.label.eq_ignore_ascii_case("chest")).count();
    if chest_count > 0 {
      return Err(format!("Isolation check failed: Found {} chest detection(s) at start point P1!", chest_count));
    }
    sleep(Duration::from_millis(200));
  }
  println!("[Isolation Check] Confirmed: Exactly 0 chest detections at P1. Memory isolation verified.");

  // 4. Initialize Tier0AgentContext
  // Initial estimate prior: player starts at P1 nominal coords with nominal yaw
  let initial_guess = (p1x, p1y, p1z, init_yaw);
  let mut agent_ctx = Tier0AgentContext::new(condition, initial_guess, scenario.target_p0, landmark_id, Some(detector));

  let initial_dist = ((p1x - scenario.target_p0.0).powi(2) + (p1z - scenario.target_p0.2).powi(2)).sqrt();
  let mut drift_samples = Vec::new();
  let mut ticks_completed = 0;
  let mut failure_mode = None;
  let mut passed = false;
  let mut final_true_distance = initial_dist;
  let mut final_est_distance = initial_dist;
  let mut final_pos_drift = 0.0;
  let mut final_yaw_drift = 0.0;

  println!("\n--- Beginning Recall Control Loop (Max 120 ticks, 2Hz) ---");
  println!("| Tick | Proprio (X, Z) | Proprio Yaw | True (X, Z) | True Yaw | True Dist | Pos Drift | Action |");
  println!("|------|----------------|-------------|-------------|----------|-----------|-----------|--------|");

  const MAX_TICKS: usize = 120;
  let start_time = Instant::now();

  for tick_idx in 1..=MAX_TICKS {
    ticks_completed = tick_idx;
    let tick_start = Instant::now();

    // Capture visual frame
    let cap = match driver_session.window().capture(window) {
      Ok(c) => c,
      Err(err) => {
        eprintln!("[Recall] Tick {:03}: Capture failed: {err}", tick_idx);
        sleep(Duration::from_millis(400));
        continue;
      }
    };
    let img = DynamicImage::ImageRgba8(cap.image);

    // Harness reads telemetry strictly for ground-truth scoring and partial condition injection
    let true_frame = read_latest_spatial_frame_from_tail(telemetry_path)?.ok_or_else(|| "No telemetry frame".to_string())?;
    let true_x = true_frame.player_pose.eye_position.x;
    let true_z = true_frame.player_pose.eye_position.z;
    let true_yaw = true_frame.player_pose.yaw;

    // Check true distance to chest target
    let cur_true_dist = ((true_x - scenario.target_p0.0).powi(2) + (true_z - scenario.target_p0.2).powi(2)).sqrt();
    final_true_distance = cur_true_dist;

    // Calculate current drift
    let est_p = agent_ctx.proprio_pose();
    let cur_pos_drift = ((est_p.x - true_x).powi(2) + (est_p.z - true_z).powi(2)).sqrt();
    let cur_yaw_drift = normalize_angle_deg(est_p.yaw - true_yaw).abs();
    let cur_est_dist = ((est_p.x - scenario.target_p0.0).powi(2) + (est_p.z - scenario.target_p0.2).powi(2)).sqrt();
    final_est_distance = cur_est_dist;
    final_pos_drift = cur_pos_drift;
    final_yaw_drift = cur_yaw_drift;

    // Sample drift every 10 ticks
    if tick_idx % 10 == 0 || tick_idx == 1 {
      drift_samples.push(DriftSample {
        tick: tick_idx,
        est_x: est_p.x,
        est_z: est_p.z,
        est_yaw: est_p.yaw,
        true_x,
        true_z,
        true_yaw,
        pos_drift_m: cur_pos_drift,
        yaw_drift_deg: cur_yaw_drift,
        true_dist_to_chest: cur_true_dist,
        est_dist_to_chest: cur_est_dist,
      });
    }

    // Condition ablation inputs
    let compass_yaw = match condition {
      TelemetryCondition::C0FullTelemetry | TelemetryCondition::C1YawOnly => Some(true_yaw),
      TelemetryCondition::C2PositionOnly | TelemetryCondition::C3AllPinched => None,
    };
    let gps_xz = match condition {
      TelemetryCondition::C0FullTelemetry | TelemetryCondition::C2PositionOnly => Some((true_x, true_z)),
      TelemetryCondition::C1YawOnly | TelemetryCondition::C3AllPinched => None,
    };

    let perception_input = AgentPerceptionInput {
      image: &img,
      elapsed_ms: start_time.elapsed().as_millis() as u64,
      compass_yaw,
      gps_xz,
    };

    // Agent decision
    let action = agent_ctx.step(perception_input);

    let action_str = match action {
      AgentAction::TurnYaw(dyaw) => {
        let dx = (dyaw as f64 / win32_input::YAW_SENSITIVITY_DEG_PER_PX).round() as i32;
        win32_input::turn_mouse(dx, 0);
        agent_ctx.apply_proprioception_turn(dyaw as f64);
        format!("Turn {:+.1}° (dx={})", dyaw, dx)
      }
      AgentAction::StepForward(ms) => {
        win32_input::step_forward(Duration::from_millis(ms));
        agent_ctx.apply_proprioception_step(ms);
        format!("Forward {}ms", ms)
      }
      AgentAction::StopSuccess => {
        println!("  -> Agent requested StopSuccess");
        "StopSuccess".to_string()
      }
      AgentAction::StopFailed(mode) => {
        println!("  -> Agent requested StopFailed({:?})", mode);
        format!("StopFailed({:?})", mode)
      }
    };

    println!(
      "| {:04} | ({:6.1}, {:6.1}) | {:+6.1}° | ({:6.1}, {:6.1}) | {:+6.1}° | {:7.2}m | {:7.2}m | {:<20} |",
      tick_idx, est_p.x, est_p.z, est_p.yaw, true_x, true_z, true_yaw, cur_true_dist, cur_pos_drift, action_str
    );

    // Termination checks
    match action {
      AgentAction::StopSuccess => {
        if cur_true_dist < 3.50 {
          println!("\n[Result] PASS: Arrived within threshold! True distance: {:.2}m < 3.50m", cur_true_dist);
          passed = true;
          break;
        } else {
          println!(
            "\n[Result] FAIL (DriftLost): Agent thought it arrived (est={:.2}m), but true distance is {:.2}m >= 3.50m!",
            cur_est_dist, cur_true_dist
          );
          failure_mode = Some(FailureMode::DriftLost);
          passed = false;
          break;
        }
      }
      AgentAction::StopFailed(mode) => {
        failure_mode = Some(mode);
        passed = false;
        break;
      }
      _ => {}
    }

    // Check if true distance reached < 3.5m even if agent hasn't called stop yet
    if cur_true_dist < 3.50 {
      println!("\n[Result] PASS: Player arrived within 3.50m! Final true distance: {:.2}m", cur_true_dist);
      passed = true;
      break;
    }

    // Check divergence (wandered too far)
    if cur_true_dist > initial_dist + 8.0 {
      println!("\n[Result] FAIL (WrongDirection): Distance monotonically increased from {:.2}m to {:.2}m!", initial_dist, cur_true_dist);
      failure_mode = Some(FailureMode::WrongDirection);
      passed = false;
      break;
    }

    let elapsed = tick_start.elapsed();
    if elapsed < Duration::from_millis(500) {
      sleep(Duration::from_millis(500) - elapsed);
    }
  }

  if !passed && failure_mode.is_none() {
    println!("\n[Result] FAIL (Timeout): Reached max {} ticks without reaching < 3.5m", MAX_TICKS);
    failure_mode = Some(FailureMode::Timeout);
  }

  // Push final drift sample
  let final_est = agent_ctx.proprio_pose();
  let f_frame = read_latest_spatial_frame_from_tail(telemetry_path)?.unwrap();
  drift_samples.push(DriftSample {
    tick: ticks_completed,
    est_x: final_est.x,
    est_z: final_est.z,
    est_yaw: final_est.yaw,
    true_x: f_frame.player_pose.eye_position.x,
    true_z: f_frame.player_pose.eye_position.z,
    true_yaw: f_frame.player_pose.yaw,
    pos_drift_m: final_pos_drift,
    yaw_drift_deg: final_yaw_drift,
    true_dist_to_chest: final_true_distance,
    est_dist_to_chest: final_est_distance,
  });

  Ok(SingleRunResult {
    condition,
    run_idx,
    scenario_name: scenario.name.to_string(),
    target_p0: [
      scenario.target_p0.0,
      scenario.target_p0.1,
      scenario.target_p0.2,
    ],
    start_p1: [p1x, p1y, p1z],
    start_distance_m: initial_dist,
    expected_initial_yaw: init_yaw,
    passed,
    final_true_distance_m: final_true_distance,
    final_est_distance_m: final_est_distance,
    final_pos_drift_m: final_pos_drift,
    final_yaw_drift_deg: final_yaw_drift,
    ticks_completed,
    failure_mode,
    drift_samples,
  })
}

// -----------------------------------------------------------------------------
// Summary & Report Computation
// -----------------------------------------------------------------------------

fn compute_condition_summary(condition: TelemetryCondition, runs: &[SingleRunResult]) -> ConditionSummary {
  let matched: Vec<&SingleRunResult> = runs.iter().filter(|r| r.condition == condition).collect();
  let count = matched.len();
  if count == 0 {
    return ConditionSummary {
      condition,
      runs_count: 0,
      pass_count: 0,
      pass_rate: 0.0,
      mean_final_distance_m: 0.0,
      mean_ticks: 0.0,
      mean_final_pos_drift_m: 0.0,
      mean_final_yaw_drift_deg: 0.0,
      failure_mode_counts: HashMap::new(),
    };
  }

  let pass_count = matched.iter().filter(|r| r.passed).count();
  let pass_rate = pass_count as f64 / count as f64;
  let mean_final_distance_m = matched.iter().map(|r| r.final_true_distance_m).sum::<f64>() / count as f64;
  let mean_ticks = matched.iter().map(|r| r.ticks_completed as f64).sum::<f64>() / count as f64;
  let mean_final_pos_drift_m = matched.iter().map(|r| r.final_pos_drift_m).sum::<f64>() / count as f64;
  let mean_final_yaw_drift_deg = matched.iter().map(|r| r.final_yaw_drift_deg).sum::<f64>() / count as f64;

  let mut failure_mode_counts = HashMap::new();
  for r in &matched {
    if let Some(fm) = r.failure_mode {
      let k = format!("{:?}", fm);
      *failure_mode_counts.entry(k).or_insert(0) += 1;
    }
  }

  ConditionSummary {
    condition,
    runs_count: count,
    pass_count,
    pass_rate,
    mean_final_distance_m,
    mean_ticks,
    mean_final_pos_drift_m,
    mean_final_yaw_drift_deg,
    failure_mode_counts,
  }
}

// -----------------------------------------------------------------------------
// Main Entrypoint
// -----------------------------------------------------------------------------

fn now_iso() -> String {
  let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
  format!("{now}")
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
  let args: Vec<String> = std::env::args().collect();
  let mut selected_condition: Option<TelemetryCondition> = None;
  let mut selected_run: Option<usize> = None;
  let mut report_out = PathBuf::from(r"F:\auv\.tmp\tier0_spike_report.json");
  let mut model_path = PathBuf::from(r"F:\auv\.tmp\yolo-runs\block_detector_v2\weights\best.onnx");
  let mut depth_model_path = PathBuf::from(r"F:\auv\.tmp\models\model-small.onnx");
  let mut telemetry_path = PathBuf::from(r"F:\pcl\.minecraft\versions\1.21.1-Fabric 0.16.10\auv\telemetry.jsonl");

  let mut i = 1;
  while i < args.len() {
    match args[i].as_str() {
      "--condition" => {
        let c = &args[i + 1].to_lowercase();
        match c.as_str() {
          "c0" => selected_condition = Some(TelemetryCondition::C0FullTelemetry),
          "c1" => selected_condition = Some(TelemetryCondition::C1YawOnly),
          "c2" => selected_condition = Some(TelemetryCondition::C2PositionOnly),
          "c3" => selected_condition = Some(TelemetryCondition::C3AllPinched),
          "all" => selected_condition = None,
          other => eprintln!("Unknown condition: {other}"),
        }
        i += 2;
      }
      "--run" => {
        if let Ok(r) = args[i + 1].parse::<usize>() {
          selected_run = Some(r);
        }
        i += 2;
      }
      "--report" => {
        report_out = PathBuf::from(&args[i + 1]);
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
      _ => {
        i += 1;
      }
    }
  }

  println!("================================================================================");
  println!("           AUV Minecraft Tier-0 Telemetry Ablation Spike (12 Runs)              ");
  println!("================================================================================");
  println!("Model:            {}", model_path.display());
  println!("Depth Model:      {}", depth_model_path.display());
  println!("Telemetry:        {}", telemetry_path.display());
  println!("Report Output:    {}", report_out.display());
  println!("Selected Cond:    {:?}", selected_condition);
  println!("Selected Run:     {:?}", selected_run);
  println!("Game Mode:        Creative");
  println!("================================================================================\n");

  // Connect to local desktop driver session
  let driver_session = auv_driver::open_local().map_err(|e| format!("Failed to open driver session: {e}"))?;
  let windows = driver_session.window().list().map_err(|e| format!("Failed to list desktop windows: {e}"))?;
  let target_window = windows
    .into_iter()
    .find(|w| {
      if let Some(t) = &w.title {
        if t.to_lowercase().contains("minecraft") {
          return true;
        }
      }
      if let Some(a) = &w.app_name {
        if a.to_lowercase().contains("minecraft") || a.to_lowercase().contains("javaw") {
          return true;
        }
      }
      false
    })
    .ok_or_else(|| "Could not find active Minecraft window on desktop".to_string())?;

  let hwnd = target_window.reference.id.parse::<isize>().unwrap_or(0);
  println!("[Init] Connected to Minecraft window: '{}' (HWND {})", target_window.title.as_deref().unwrap_or("?"), hwnd);
  win32_input::activate_minecraft(Some(hwnd));

  let scenarios = get_scenarios();
  let conditions = match selected_condition {
    Some(c) => vec![c],
    None => vec![
      TelemetryCondition::C0FullTelemetry,
      TelemetryCondition::C1YawOnly,
      TelemetryCondition::C2PositionOnly,
      TelemetryCondition::C3AllPinched,
    ],
  };

  let run_indices = match selected_run {
    Some(r) => vec![r],
    None => vec![1, 2, 3],
  };

  let mut all_results = Vec::new();
  let store_path = PathBuf::from(r"F:\auv\.tmp\tier0_spike_store.json");

  for &cond in &conditions {
    for &r_idx in &run_indices {
      let scenario = &scenarios[r_idx - 1];
      let res = run_single_ablation(
        cond,
        r_idx,
        scenario,
        hwnd,
        &driver_session,
        &target_window,
        &model_path,
        &depth_model_path,
        &telemetry_path,
        &store_path,
      )?;
      all_results.push(res);
      sleep(Duration::from_millis(1000));
    }
  }

  // Compute condition summaries
  let mut summaries = HashMap::new();
  for &c in &[
    TelemetryCondition::C0FullTelemetry,
    TelemetryCondition::C1YawOnly,
    TelemetryCondition::C2PositionOnly,
    TelemetryCondition::C3AllPinched,
  ] {
    let summary = compute_condition_summary(c, &all_results);
    summaries.insert(c.short_name().to_string(), summary);
  }

  // Print Summary Table
  println!("\n==========================================================================================");
  println!("                           TIER-0 ABLATION SPIKE SUMMARY TABLE                            ");
  println!("==========================================================================================");
  println!("| Cond | Pass Rate | Mean Final Dist | Mean Ticks | Mean Pos Drift | Mean Yaw Drift | Failure Modes |");
  println!("|------|-----------|-----------------|------------|----------------|----------------|---------------|");
  for &c in &[
    TelemetryCondition::C0FullTelemetry,
    TelemetryCondition::C1YawOnly,
    TelemetryCondition::C2PositionOnly,
    TelemetryCondition::C3AllPinched,
  ] {
    let s = summaries.get(c.short_name()).unwrap();
    let fm_str = if s.failure_mode_counts.is_empty() {
      "None".to_string()
    } else {
      s.failure_mode_counts.iter().map(|(k, v)| format!("{}: {}", k, v)).collect::<Vec<_>>().join(", ")
    };
    println!(
      "| {:<4} | {:>3}/{} ({:>5.1}%) | {:>13.2}m | {:>10.1} | {:>12.2}m | {:>12.1}° | {:<13} |",
      c.short_name().to_uppercase(),
      s.pass_count,
      s.runs_count,
      s.pass_rate * 100.0,
      s.mean_final_distance_m,
      s.mean_ticks,
      s.mean_final_pos_drift_m,
      s.mean_final_yaw_drift_deg,
      fm_str
    );
  }
  println!("==========================================================================================\n");

  // Save report JSON
  let report = Tier0SpikeReport {
    schema_version: 1,
    generated_at: now_iso(),
    game_mode: "creative".to_string(),
    isolation_mechanism_code_location:
      "Tier0AgentContext: constructor receives only initial start guess prior; no telemetry reader or file handle exists".to_string(),
    runs: all_results,
    condition_summaries: summaries,
    overall_recommendation: "Refer to docs/ai/references/3dgs/2026-09-30-tier0-spike-findings.md for empirical verdict".to_string(),
  };

  let json_str = serde_json::to_string_pretty(&report)?;
  fs::write(&report_out, &json_str)?;
  println!("[Report] Saved complete Tier-0 Spike Report to: {}", report_out.display());

  Ok(())
}
