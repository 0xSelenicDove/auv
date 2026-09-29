//! crafting_table Live Recall Gap 分层调查采样工具（Brief: crafting_table recall gap）
//!
//! 核心任务：
//!   回答为什么 live recall ≈ 17%（45/265），而 val AP@0.5 = 0.9766？
//!   gap 是“定义问题”（小/远/遮挡的工作台本来就检不出），还是“真退化”（近的、大的、清楚的也漏）？
//!
//! 采集指标（每 tick × 每个在视锥内的真值工作台）：
//!   - 3D 距离（米）
//!   - 投影 bbox 面积（px²）
//!   - 是否遮挡（基于视线 raycast 障碍判定）
//!   - 生产 0.50 阈值下是否匹配（TP vs FN，60px 规则）
//!   - 诊断 0.15 阈值下的最高置信度（检测框 conf，若无则为 0.0）
//!
//! 运行模式：创造模式（平原）

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use image::DynamicImage;
use serde::{Deserialize, Serialize};

use auv_game_minecraft::ingest::read_latest_spatial_frame_from_tail;
use auv_game_minecraft::projection::MinecraftProjector;
use auv_game_minecraft::types::{BlockPosition, MinecraftBlockTarget};
use auv_game_minecraft::visual_perception::{BlockDetector, BlockDetectorConfig};

#[cfg(target_os = "windows")]
#[allow(dead_code, clippy::upper_case_acronyms)]
mod win32_input {
  use std::ffi::c_void;
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
  pub const SW_RESTORE: i32 = 9;
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
      let hdesk2 = OpenDesktopA(c"Default".as_ptr() as *const u8, 0, 0, 0x01FF);
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
          return false;
        }
      }
    };

    if hwnd.is_null() {
      return false;
    }

    let mut target_pid: u32 = 0;
    let target_tid = unsafe { GetWindowThreadProcessId(hwnd, &mut target_pid) };
    let cur_tid = unsafe { GetCurrentThreadId() };
    let fore_hwnd = unsafe { GetForegroundWindow() };
    let mut fore_pid: u32 = 0;
    let fore_tid = unsafe { GetWindowThreadProcessId(fore_hwnd, &mut fore_pid) };

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
    true
  }

  pub fn send_chat_command(cmd: &str) {
    ensure_default_desktop();
    let hwnd = TARGET_HWND.load(std::sync::atomic::Ordering::SeqCst) as HWND;
    if hwnd.is_null() {
      return;
    }

    const WM_KEYDOWN: u32 = 0x0100;
    const WM_KEYUP: u32 = 0x0101;
    const WM_CHAR: u32 = 0x0102;

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
      sleep(Duration::from_millis(250));
    }
  }
}

// ── 真值坐标 ──────────────────────────────────────────────────────────────────
const GROUND_TRUTH: &[(&str, (i32, i32, i32))] = &[
  ("CT-A", (-47, 95, 265)),
  ("CT-B", (-43, 95, 269)),
  ("CT-C", (-54, 95, 264)),
  ("CT-D", (-55, 95, 257)),
  ("CT-E", (-47, 95, 256)),
];

const PROD_CONF_THRESH: f64 = 0.50;
const DIAG_CONF_THRESH: f64 = 0.15;
const MATCH_RADIUS_PX: f64 = 60.0;
const DEFAULT_TICKS: usize = 80;
const TICK_INTERVAL_MS: u64 = 1000;

// 多视角巡航采样点配置（覆盖近 <5m、中 5-10m、远 >10m 全覆盖）
struct VantagePoint {
  start_tick: usize,
  end_tick: usize,
  pos: (f64, f64, f64),
  yaw_pitch: (f64, f64),
  description: &'static str,
}

const VANTAGE_POINTS: &[VantagePoint] = &[
  VantagePoint {
    start_tick: 1,
    end_tick: 15,
    pos: (-49.0, 96.6, 276.0),
    yaw_pitch: (-162.0, 5.0),
    description: "Stage 1: 远距离南向全景 (9m~20m)",
  },
  VantagePoint {
    start_tick: 16,
    end_tick: 30,
    pos: (-47.0, 96.6, 271.0),
    yaw_pitch: (-165.0, 5.0),
    description: "Stage 2: 中距离南向 (4.5m~16m)",
  },
  VantagePoint {
    start_tick: 31,
    end_tick: 45,
    pos: (-48.0, 96.6, 265.0),
    yaw_pitch: (-140.0, 10.0),
    description: "Stage 3: 近距离特写 A, B (1.5m~10m)",
  },
  VantagePoint {
    start_tick: 46,
    end_tick: 60,
    pos: (-53.0, 96.6, 254.0),
    yaw_pitch: (25.0, 10.0),
    description: "Stage 4: 西北向特写 D, E (3m~10m)",
  },
  VantagePoint {
    start_tick: 61,
    end_tick: 75,
    pos: (-44.0, 96.6, 260.0),
    yaw_pitch: (-90.0, 5.0),
    description: "Stage 5: 侧向观测 (4m~11m)",
  },
  VantagePoint {
    start_tick: 76,
    end_tick: 80,
    pos: (-49.0, 96.6, 276.0),
    yaw_pitch: (-162.0, 5.0),
    description: "Stage 6: 回归初始站位 (远景收尾)",
  },
];

// ── 报告数据结构 ──────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TruthObservationSample {
  pub tick: usize,
  pub label: String,
  pub block_pos: (i32, i32, i32),
  pub distance_m: f64,
  pub screen_x: f64,
  pub screen_y: f64,
  pub bbox_2d_px: [f64; 4], // [min_x, min_y, max_x, max_y]
  pub bbox_area_px2: f64,
  pub is_occluded: bool,
  pub is_tp_prod: bool,   // matched with conf >= 0.50 within 60px
  pub max_conf_diag: f64, // highest crafting_table detection conf within 60px (>=0.15)
  pub nearest_det_dist_px: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RawDetection {
  pub conf: f64,
  pub cx: f64,
  pub cy: f64,
  pub bbox: (f64, f64, f64, f64),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TickRecord {
  pub tick: usize,
  pub player_pos: [f64; 3],
  pub player_yaw: f64,
  pub player_pitch: f64,
  pub stage_desc: String,
  pub raw_detections_prod: usize, // conf >= 0.50
  pub raw_detections_diag: usize, // conf >= 0.15
  pub in_frustum_count: usize,
  pub truth_samples: Vec<TruthObservationSample>,
}

#[derive(Serialize, Deserialize)]
pub struct RecallGapReport {
  pub schema_version: u32,
  pub generated_at: String,
  pub game_mode: String,
  pub total_ticks: usize,
  pub prod_conf_threshold: f64,
  pub diag_conf_threshold: f64,
  pub match_radius_px: f64,
  pub total_in_frustum_samples: usize,
  pub total_tp_samples: usize,
  pub total_fn_samples: usize,
  pub overall_live_recall: f64,
  pub ticks: Vec<TickRecord>,
}

fn px_dist(x1: f64, y1: f64, x2: f64, y2: f64) -> f64 {
  let dx = x1 - x2;
  let dy = y1 - y2;
  (dx * dx + dy * dy).sqrt()
}

fn check_occlusion(eye: (f64, f64, f64), target: (i32, i32, i32), nearby: &HashMap<(i32, i32, i32), String>) -> bool {
  let target_center = (target.0 as f64 + 0.5, target.1 as f64 + 0.5, target.2 as f64 + 0.5);
  let dx = target_center.0 - eye.0;
  let dy = target_center.1 - eye.1;
  let dz = target_center.2 - eye.2;
  let dist = (dx * dx + dy * dy + dz * dz).sqrt();

  if dist < 0.8 {
    return false;
  }

  // Step along the ray in 0.2m increments
  let steps = (dist / 0.20).ceil() as usize;
  for i in 1..steps {
    let t = i as f64 / steps as f64;
    if t < 0.05 || t > 0.92 {
      continue;
    }
    let px = eye.0 + t * dx;
    let py = eye.1 + t * dy;
    let pz = eye.2 + t * dz;

    let bx = px.floor() as i32;
    let by = py.floor() as i32;
    let bz = pz.floor() as i32;

    if (bx, by, bz) == target {
      continue;
    }

    if let Some(block_id) = nearby.get(&(bx, by, bz)) {
      // Check if solid obstruction (ignore non-solid like torches or air)
      if !block_id.contains("air") && !block_id.contains("torch") && !block_id.contains("short_grass") {
        return true;
      }
    }
  }

  false
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
  let model_path = PathBuf::from(r"F:\auv\.tmp\yolo-runs\block_detector_v2\weights\best.onnx");
  let telemetry_path = PathBuf::from(r"F:\pcl\.minecraft\versions\1.21.1-Fabric 0.16.10\auv\telemetry.jsonl");
  let report_out = PathBuf::from(r"F:\auv\.tmp\ct_recall_gap_report.json");
  let ticks_total = DEFAULT_TICKS;

  println!("============================================================");
  println!("  crafting_table Live Recall Gap 分层采样探针");
  println!("============================================================");
  println!("Game Mode:       Creative Mode (创造模式)");
  println!("Model:           {}", model_path.display());
  println!("Telemetry:       {}", telemetry_path.display());
  println!("Report Out:      {}", report_out.display());
  println!("Ticks:           {ticks_total}");
  println!("Production Conf: {PROD_CONF_THRESH}");
  println!("Diagnostic Conf: {DIAG_CONF_THRESH}");
  println!("Match Radius:    {MATCH_RADIUS_PX} px");
  println!();

  // 1. 打开 driver session 捕获窗口
  println!("[Init] Connecting to local driver session...");
  let driver_session = auv_driver::open_local().map_err(|e| format!("driver session: {e}"))?;
  let windows = driver_session.window().list().map_err(|e| format!("failed to list windows: {e}"))?;
  let window = windows
    .iter()
    .find(|w| {
      w.title.as_deref().map(|t| t.to_lowercase().contains("minecraft")).unwrap_or(false)
        || w.app_name.as_deref().map(|a| a.to_lowercase().contains("javaw")).unwrap_or(false)
    })
    .ok_or("Minecraft window not found")?
    .clone();

  println!("[Init] Found window: {:?}", window.title.as_deref().unwrap_or("?"));
  let hwnd = window.reference.id.parse::<isize>().unwrap_or(0);
  #[cfg(target_os = "windows")]
  {
    if hwnd != 0 {
      win32_input::set_target_hwnd(hwnd);
      win32_input::activate_minecraft(Some(hwnd));
    }
  }

  // 2. 加载 YOLOv8n v2 模型（设置诊断下限 DIAG_CONF_THRESH=0.15）
  println!("[Init] Loading BlockDetector v2 with diagnostic threshold {DIAG_CONF_THRESH}...");
  let detector_config = BlockDetectorConfig {
    model_path,
    per_class_threshold: [DIAG_CONF_THRESH; 6],
    ..Default::default()
  };
  let mut detector = BlockDetector::new(detector_config).map_err(|e| format!("BlockDetector load: {e}"))?;
  detector.set_confidence_threshold(DIAG_CONF_THRESH);
  println!("[Init] Model ready.");

  // 3. 开始多视角巡航采样
  println!();
  println!("| Tick | Stage | EyePos (X,Y,Z) | Yaw | CT_dets (>=0.5/0.15) | InFrustum | TP | FN | Details");
  println!("|------|-------|----------------|-----|----------------------|-----------|----|----|--------");

  let mut all_tick_records: Vec<TickRecord> = Vec::new();
  let mut all_samples: Vec<TruthObservationSample> = Vec::new();

  for tick in 1..=ticks_total {
    let t0 = Instant::now();

    // 检查是否需要切换视角 (Stage transition)
    let cur_vantage = VANTAGE_POINTS.iter().find(|vp| tick >= vp.start_tick && tick <= vp.end_tick).expect("vantage point covers all ticks");

    #[cfg(target_os = "windows")]
    {
      if tick == cur_vantage.start_tick {
        let tp_cmd = format!(
          "/tp @s {:.1} {:.1} {:.1} {:.1} {:.1}",
          cur_vantage.pos.0, cur_vantage.pos.1, cur_vantage.pos.2, cur_vantage.yaw_pitch.0, cur_vantage.yaw_pitch.1
        );
        win32_input::send_chat_command(&tp_cmd);
        sleep(Duration::from_millis(400)); // 等待客户端位姿同步
      }
    }

    // 读 telemetry
    let frame_opt = read_latest_spatial_frame_from_tail(&telemetry_path).ok().flatten();
    let cap_result = driver_session.window().capture(&window);
    let img_opt = cap_result.ok().map(|c| DynamicImage::ImageRgba8(c.image).to_rgb8());

    let (frame, img) = match (frame_opt, img_opt) {
      (Some(f), Some(i)) => (f, i),
      _ => {
        sleep(Duration::from_millis(100));
        continue;
      }
    };

    // 整理 nearby_blocks 用于射线遮挡判定
    let mut nearby_map: HashMap<(i32, i32, i32), String> = HashMap::new();
    for nb in &frame.nearby_blocks {
      nearby_map.insert((nb.block_pos.x, nb.block_pos.y, nb.block_pos.z), nb.block_id.clone());
    }

    // 运行视觉推理（0.15 诊断下限）
    let dyn_img = DynamicImage::ImageRgb8(img);
    let raw_dets = detector.detect(&dyn_img).unwrap_or_default();

    let ct_dets_diag: Vec<RawDetection> = raw_dets
      .iter()
      .filter(|d| d.label == "crafting_table")
      .map(|d| {
        let (x1, y1, x2, y2) = d.bbox;
        RawDetection {
          conf: d.confidence,
          cx: (x1 + x2) * 0.5,
          cy: (y1 + y2) * 0.5,
          bbox: d.bbox,
        }
      })
      .collect();

    let ct_dets_prod: Vec<&RawDetection> = ct_dets_diag.iter().filter(|d| d.conf >= PROD_CONF_THRESH).collect();

    // 投影与真值匹配
    let eye = (frame.player_pose.eye_position.x, frame.player_pose.eye_position.y, frame.player_pose.eye_position.z);
    let projector = MinecraftProjector::new(frame.clone()).unwrap();
    let vw = frame.viewport.width as f64;
    let vh = frame.viewport.height as f64;

    let mut tick_samples: Vec<TruthObservationSample> = Vec::new();
    let mut tp_count = 0usize;
    let mut fn_count = 0usize;

    for (lbl, (bx, by, bz)) in GROUND_TRUTH {
      let bpos = BlockPosition::new(*bx, *by, *bz);
      let target = MinecraftBlockTarget::new(bpos);

      // 中心点投影
      let proj_pt = match projector.project_block_target(&target) {
        Ok(p) => p,
        Err(_) => continue,
      };

      let (sx, sy) = match proj_pt.screen_point {
        Some(p) => (p.x, p.y),
        None => continue,
      };

      // 检查中心是否在视口内
      if sx < 0.0 || sx >= vw || sy < 0.0 || sy >= vh {
        continue;
      }

      // 投影 2D 边框与面积
      let (bbox_2d, area_px2) = match projector.project_block_2d_bbox(bpos) {
        Ok(Some(b)) => {
          let w = (b[2] - b[0]).max(0.0);
          let h = (b[3] - b[1]).max(0.0);
          (b, w * h)
        }
        _ => ([sx - 10.0, sy - 10.0, sx + 10.0, sy + 10.0], 400.0),
      };

      // 3D 距离
      let center_3d = (*bx as f64 + 0.5, *by as f64 + 0.5, *bz as f64 + 0.5);
      let dist_m = ((center_3d.0 - eye.0).powi(2) + (center_3d.1 - eye.1).powi(2) + (center_3d.2 - eye.2).powi(2)).sqrt();

      // 遮挡判定
      let is_occluded = check_occlusion(eye, (*bx, *by, *bz), &nearby_map);

      // 生产 0.50 匹配
      let nearest_prod_dist = ct_dets_prod.iter().map(|d| px_dist(d.cx, d.cy, sx, sy)).fold(f64::INFINITY, f64::min);
      let is_tp_prod = nearest_prod_dist < MATCH_RADIUS_PX;

      if is_tp_prod {
        tp_count += 1;
      } else {
        fn_count += 1;
      }

      // 诊断 0.15 口径最高 conf
      let mut max_conf_diag = 0.0;
      let mut nearest_diag_dist = f64::INFINITY;
      for d in &ct_dets_diag {
        let dist = px_dist(d.cx, d.cy, sx, sy);
        if dist < nearest_diag_dist {
          nearest_diag_dist = dist;
        }
        if dist < MATCH_RADIUS_PX && d.conf > max_conf_diag {
          max_conf_diag = d.conf;
        }
      }

      let sample = TruthObservationSample {
        tick,
        label: lbl.to_string(),
        block_pos: (*bx, *by, *bz),
        distance_m: dist_m,
        screen_x: sx,
        screen_y: sy,
        bbox_2d_px: bbox_2d,
        bbox_area_px2: area_px2,
        is_occluded,
        is_tp_prod,
        max_conf_diag,
        nearest_det_dist_px: nearest_diag_dist,
      };

      tick_samples.push(sample.clone());
      all_samples.push(sample);
    }

    let in_frustum_count = tick_samples.len();

    let details: Vec<String> = tick_samples
      .iter()
      .map(|s| {
        let status = if s.is_tp_prod { "TP" } else { "FN" };
        let occ = if s.is_occluded { "OCC" } else { "CLEAR" };
        format!("{}:{}({:.1}m,{:.0}px²,{},c={:.2})", s.label, status, s.distance_m, s.bbox_area_px2, occ, s.max_conf_diag)
      })
      .collect();

    println!(
      "| {:4} | {:5} | ({:5.1},{:4.1},{:5.1}) | {:4.0} | {:2} / {:2}             | {:9} | {:2} | {:2} | {}",
      tick,
      format!("Stg{}", cur_vantage.description.chars().nth(6).unwrap_or('?')),
      eye.0,
      eye.1,
      eye.2,
      frame.player_pose.yaw,
      ct_dets_prod.len(),
      ct_dets_diag.len(),
      in_frustum_count,
      tp_count,
      fn_count,
      details.join("; ")
    );

    all_tick_records.push(TickRecord {
      tick,
      player_pos: [eye.0, eye.1, eye.2],
      player_yaw: frame.player_pose.yaw,
      player_pitch: frame.player_pose.pitch,
      stage_desc: cur_vantage.description.to_string(),
      raw_detections_prod: ct_dets_prod.len(),
      raw_detections_diag: ct_dets_diag.len(),
      in_frustum_count,
      truth_samples: tick_samples,
    });

    let elapsed = t0.elapsed();
    let budget = Duration::from_millis(TICK_INTERVAL_MS);
    if elapsed < budget {
      sleep(budget - elapsed);
    }
  }

  // 4. 汇总与生成报告
  let total_in_frustum = all_samples.len();
  let total_tp = all_samples.iter().filter(|s| s.is_tp_prod).count();
  let total_fn = all_samples.iter().filter(|s| !s.is_tp_prod).count();
  let overall_recall = if total_in_frustum > 0 {
    total_tp as f64 / total_in_frustum as f64
  } else {
    0.0
  };

  println!();
  println!("============================================================");
  println!("  采样完成！总计采样 {} 帧，真值视锥样本 {} 例", ticks_total, total_in_frustum);
  println!("  总计 TP: {total_tp} | 总计 FN: {total_fn} | 总 Recall: {:.2}%", overall_recall * 100.0);
  println!("============================================================");

  let now_str = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs().to_string();

  let report = RecallGapReport {
    schema_version: 1,
    generated_at: now_str,
    game_mode: "Creative Mode (创造模式)".to_string(),
    total_ticks: ticks_total,
    prod_conf_threshold: PROD_CONF_THRESH,
    diag_conf_threshold: DIAG_CONF_THRESH,
    match_radius_px: MATCH_RADIUS_PX,
    total_in_frustum_samples: total_in_frustum,
    total_tp_samples: total_tp,
    total_fn_samples: total_fn,
    overall_live_recall: overall_recall,
    ticks: all_tick_records,
  };

  let json_str = serde_json::to_string_pretty(&report)?;
  fs::write(&report_out, json_str)?;
  println!("报告已写入: {}", report_out.display());

  Ok(())
}
