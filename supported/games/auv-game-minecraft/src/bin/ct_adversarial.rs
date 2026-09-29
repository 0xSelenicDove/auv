//! crafting_table 对抗场景 TP/FP 分析二进制（Brief: crafting_table adversarial）
//!
//! 流程：
//!   60 ticks × 1Hz，每 tick：
//!     1. 捕获 Minecraft 窗口帧 + 读最新 telemetry
//!     2. 用 BlockDetector v2 推理，拿 crafting_table detection (bbox)
//!     3. 用 MinecraftProjector 把 5 个真值块坐标投影到屏幕
//!     4. bbox 中心到最近投影点距离 < 60px → TP，否则 → FP
//!        真值投影在视锥内但无匹配 detection → FN
//!   汇总 precision_live = TP / (TP + FP)
//!
//! 预承诺决策规则（跑之前定死）：
//!   precision_live >= 0.80 → MAINTAIN_0.50（6e 政策不变）
//!   precision_live <  0.80 → RECOMMEND_0.70（建议，不执行）
//!
//! 用法：
//!   ct_adversarial [OPTIONS]
//!     --ticks <N>         ticks 数（默认 60）
//!     --model <PATH>      v2 模型路径
//!     --telemetry <PATH>  telemetry.jsonl 路径
//!     --report-out <PATH> 报告输出路径

use std::env;
use std::fs;
use std::path::PathBuf;
use std::thread::sleep;
use std::time::{Duration, Instant};

use image::DynamicImage;
use serde::{Deserialize, Serialize};

use auv_game_minecraft::ingest::read_latest_spatial_frame_from_tail;
use auv_game_minecraft::projection::MinecraftProjector;
use auv_game_minecraft::types::{BlockPosition, MinecraftBlockTarget};
use auv_game_minecraft::visual_perception::{BlockDetector, BlockDetectorConfig};

// ── 真值坐标（来自 F3 截图） ──────────────────────────────────────────────────
// 5 个 crafting_table 精确块坐标（整数网格）
const GROUND_TRUTH: &[(&str, (i32, i32, i32))] = &[
  ("CT-A", (-47, 95, 265)),
  ("CT-B", (-43, 95, 269)),
  ("CT-C", (-54, 95, 264)),
  ("CT-D", (-55, 95, 257)),
  ("CT-E", (-47, 95, 256)),
];

const CONF_THRESH: f64 = 0.50;
const MATCH_RADIUS_PX: f64 = 60.0;
const DEFAULT_TICKS: usize = 60;
const TICK_INTERVAL_MS: u64 = 1000;

// ── 报告结构 ──────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
struct DetectionRecord {
  conf: f64,
  cx: f64,
  cy: f64,
  x1: f64,
  y1: f64,
  x2: f64,
  y2: f64,
  classification: String, // "TP" | "FP"
  nearest_truth: Option<String>,
  nearest_dist_px: f64,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  saved_image_path: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ProjectedTruth {
  label: String,
  block_pos: (i32, i32, i32),
  screen_x: Option<f64>,
  screen_y: Option<f64>,
  in_frustum: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct TickLog {
  tick: usize,
  latency_ms: u64,
  telemetry_ts: u64,
  detections: Vec<DetectionRecord>,
  projected_truths: Vec<ProjectedTruth>,
  tp: usize,
  fp: usize,
  fn_count: usize,
}

#[derive(Debug, Serialize, Deserialize)]
struct AdversarialReport {
  schema_version: u32,
  generated_at: String,
  game_mode: String,
  ground_truth: Vec<(String, [i32; 3])>,
  conf_threshold: f64,
  match_radius_px: f64,
  ticks: usize,
  tp_total: usize,
  fp_total: usize,
  fn_total: usize,
  precision_live: Option<f64>,
  all_truths_observed: bool,
  truth_frustum_counts: std::collections::HashMap<String, usize>,
  decision: String,
  recommendation: String,
  fp_sample: Vec<DetectionRecord>,
  tick_logs: Vec<TickLog>,
}

// ── 参数解析 ──────────────────────────────────────────────────────────────────

struct Args {
  ticks: usize,
  model_path: PathBuf,
  telemetry_path: PathBuf,
  report_out: PathBuf,
  target_title: String,
}

fn parse_args() -> Args {
  let mut ticks = DEFAULT_TICKS;
  let mut model_path = PathBuf::from(r"F:\auv\.tmp\yolo-runs\block_detector_v2\weights\best.onnx");
  let mut telemetry_path = PathBuf::from(r"F:\pcl\.minecraft\versions\1.21.1-Fabric 0.16.10\auv\telemetry.jsonl");
  let mut report_out = PathBuf::from(r"F:\auv\.tmp\ct_adversarial_report.json");
  let mut target_title = "Minecraft".to_string();

  let mut it = env::args().skip(1);
  while let Some(arg) = it.next() {
    match arg.as_str() {
      "--ticks" => {
        if let Some(v) = it.next() {
          ticks = v.parse().unwrap_or(DEFAULT_TICKS);
        }
      }
      "--model" => {
        if let Some(v) = it.next() {
          model_path = PathBuf::from(v);
        }
      }
      "--telemetry" => {
        if let Some(v) = it.next() {
          telemetry_path = PathBuf::from(v);
        }
      }
      "--report-out" => {
        if let Some(v) = it.next() {
          report_out = PathBuf::from(v);
        }
      }
      "--target-title" => {
        if let Some(v) = it.next() {
          target_title = v;
        }
      }
      "--help" | "-h" => {
        println!("ct_adversarial — crafting_table TP/FP adversarial benchmark");
        println!("  --ticks <N>          Number of ticks (default: 60)");
        println!("  --model <PATH>       v2 ONNX model path");
        println!("  --telemetry <PATH>   telemetry.jsonl path");
        println!("  --report-out <PATH>  JSON report output path");
        std::process::exit(0);
      }
      other => {
        eprintln!("Unknown arg: {other}");
        std::process::exit(1);
      }
    }
  }
  Args {
    ticks,
    model_path,
    telemetry_path,
    report_out,
    target_title,
  }
}

// ── 距离计算 ──────────────────────────────────────────────────────────────────

fn px_dist(ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
  ((ax - bx).powi(2) + (ay - by).powi(2)).sqrt()
}

// ── 主程序 ────────────────────────────────────────────────────────────────────

fn main() -> Result<(), String> {
  let args = parse_args();

  println!("============================================================");
  println!("  crafting_table 对抗场景 TP/FP 分析 (Brief adversarial)");
  println!("============================================================");
  println!("Ground truth: {} crafting_tables", GROUND_TRUTH.len());
  for (lbl, (x, y, z)) in GROUND_TRUTH {
    println!("  {lbl}: ({x}, {y}, {z})");
  }
  println!("Conf threshold:  {CONF_THRESH}");
  println!("Match radius:    {MATCH_RADIUS_PX}px");
  println!("Ticks:           {}", args.ticks);
  println!();

  // ── 打开 driver session ──────────────────────────────────────────────────
  println!("[Init] Opening local driver session...");
  let driver_session = auv_driver::open_local().map_err(|e| format!("driver session: {e}"))?;

  println!("[Init] Listing desktop windows...");
  let windows = driver_session.window().list().map_err(|e| format!("failed to list windows: {e}"))?;

  let window = windows
    .iter()
    .find(|w| {
      w.title.as_deref().map(|t| t.to_lowercase().contains(&args.target_title.to_lowercase())).unwrap_or(false)
        || w.app_name.as_deref().map(|a| a.to_lowercase().contains("minecraft") || a.to_lowercase().contains("javaw")).unwrap_or(false)
    })
    .ok_or_else(|| {
      let titles: Vec<_> = windows
        .iter()
        .map(|w| format!("app={:?} title={:?}", w.app_name.as_deref().unwrap_or("?"), w.title.as_deref().unwrap_or("?")))
        .collect();
      format!("Minecraft window not found. Visible windows: {}", titles.join(", "))
    })?
    .clone();

  println!("[Init] Window: {:?}", window.title.as_deref().unwrap_or("untitled"));

  // ── 加载模型 ─────────────────────────────────────────────────────────────
  println!("[Init] Loading BlockDetector v2 from {}...", args.model_path.display());
  let mut detector_config = BlockDetectorConfig::default();
  detector_config.model_path = args.model_path.clone();
  // Per-class threshold: set all 6 classes to CONF_THRESH (matches 6e policy: 0.50).
  detector_config.per_class_threshold = [CONF_THRESH; 6];
  let mut detector = BlockDetector::new(detector_config).map_err(|e| format!("BlockDetector load: {e}"))?;
  detector.set_confidence_threshold(CONF_THRESH);
  println!("[Init] Model loaded.");

  // ── 主循环 ───────────────────────────────────────────────────────────────
  println!();
  println!("| Tick | ms  | CT_dets | TP | FP | FN | in_frustum | Notes");
  println!("|------|-----|---------|----|----|----|----|-----------|------");

  let mut tick_logs: Vec<TickLog> = Vec::new();
  let mut tp_total = 0usize;
  let mut fp_total = 0usize;
  let mut fn_total = 0usize;
  let mut fp_saved_count = 0usize;
  let mut truth_frustum_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

  for tick in 1..=args.ticks {
    let t0 = Instant::now();

    // 读 telemetry
    let frame_opt = read_latest_spatial_frame_from_tail(&args.telemetry_path).ok().flatten();
    let telemetry_ts = frame_opt.as_ref().map(|f| f.monotonic_timestamp_ms).unwrap_or(0);

    // 捕获窗口帧
    let cap_result = driver_session.window().capture(&window);
    let img_opt = cap_result.ok().map(|c| DynamicImage::ImageRgba8(c.image).to_rgb8());

    // 运行检测
    let mut all_dets: Vec<DetectionRecord> = Vec::new();
    if let (Some(img), Some(_)) = (&img_opt, &frame_opt) {
      let dyn_img = DynamicImage::ImageRgb8(img.clone());
      match detector.detect(&dyn_img) {
        Ok(raw_dets) => {
          for det in raw_dets.iter().filter(|d| d.label == "crafting_table") {
            let (x1, y1, x2, y2) = det.bbox;
            let cx = (x1 + x2) * 0.5;
            let cy = (y1 + y2) * 0.5;
            all_dets.push(DetectionRecord {
              conf: det.confidence,
              cx,
              cy,
              x1,
              y1,
              x2,
              y2,
              classification: String::new(), // filled below
              nearest_truth: None,
              nearest_dist_px: f64::INFINITY,
              saved_image_path: None,
            });
          }
        }
        Err(e) => {
          eprintln!("[{tick:3}] WARN: detector error: {e}");
        }
      }
    }

    // 投影 5 个真值到屏幕
    let mut projected_truths: Vec<ProjectedTruth> = Vec::new();
    if let Some(frame) = &frame_opt {
      if let Ok(projector) = MinecraftProjector::new(frame.clone()) {
        for (lbl, (bx, by, bz)) in GROUND_TRUTH {
          let block_pos = BlockPosition {
            x: *bx,
            y: *by,
            z: *bz,
          };
          let target = MinecraftBlockTarget::new(block_pos);
          match projector.project_block_target(&target) {
            Ok(pp) => {
              let in_frustum = pp.screen_point.is_some();
              if in_frustum {
                *truth_frustum_counts.entry(lbl.to_string()).or_insert(0) += 1;
              }
              let (screen_x, screen_y) = match pp.screen_point {
                Some(p) => (Some(p.x), Some(p.y)),
                None => (None, None),
              };
              projected_truths.push(ProjectedTruth {
                label: lbl.to_string(),
                block_pos: (*bx, *by, *bz),
                screen_x,
                screen_y,
                in_frustum,
              });
            }
            Err(_) => {
              projected_truths.push(ProjectedTruth {
                label: lbl.to_string(),
                block_pos: (*bx, *by, *bz),
                screen_x: None,
                screen_y: None,
                in_frustum: false,
              });
            }
          }
        }
      }
    }

    // TP/FP 分类
    let mut matched_truths: Vec<String> = Vec::new();
    let mut tp_tick = 0usize;
    let mut fp_tick = 0usize;

    for det in all_dets.iter_mut() {
      let mut best_dist = f64::INFINITY;
      let mut best_lbl: Option<String> = None;

      for pt in &projected_truths {
        if let (Some(sx), Some(sy)) = (pt.screen_x, pt.screen_y) {
          let d = px_dist(det.cx, det.cy, sx, sy);
          if d < best_dist {
            best_dist = d;
            best_lbl = Some(pt.label.clone());
          }
        }
      }

      det.nearest_dist_px = best_dist;
      det.nearest_truth = best_lbl.clone();

      if best_dist < MATCH_RADIUS_PX {
        det.classification = "TP".to_string();
        tp_tick += 1;
        if let Some(lbl) = best_lbl {
          if !matched_truths.contains(&lbl) {
            matched_truths.push(lbl);
          }
        }
      } else {
        det.classification = "FP".to_string();
        fp_tick += 1;
        if fp_saved_count < 20 {
          if let Some(img) = &img_opt {
            let out_path = format!("F:\\auv\\.tmp\\ct_fp_sample_{}.png", fp_saved_count + 1);
            let dyn_img = DynamicImage::ImageRgb8(img.clone());
            if dyn_img.save(&out_path).is_ok() {
              det.saved_image_path = Some(out_path);
              fp_saved_count += 1;
            }
          }
        }
      }
    }

    // FN：视锥内真值但无匹配检测
    let fn_tick = projected_truths.iter().filter(|pt| pt.in_frustum && !matched_truths.contains(&pt.label)).count();

    tp_total += tp_tick;
    fp_total += fp_tick;
    fn_total += fn_tick;

    let in_frustum_n = projected_truths.iter().filter(|p| p.in_frustum).count();
    let elapsed_ms = t0.elapsed().as_millis() as u64;

    // 每 tick 打印结果
    let notes: Vec<String> = all_dets
      .iter()
      .map(|d| format!("conf={:.2} {}→{}({:.0}px)", d.conf, d.classification, d.nearest_truth.as_deref().unwrap_or("?"), d.nearest_dist_px))
      .collect();
    println!(
      "| {:4} | {:3} | {:7} | {:2} | {:2} | {:2} | {:10} | {}",
      tick,
      elapsed_ms,
      all_dets.len(),
      tp_tick,
      fp_tick,
      fn_tick,
      in_frustum_n,
      notes.join("; ")
    );

    tick_logs.push(TickLog {
      tick,
      latency_ms: elapsed_ms,
      telemetry_ts,
      detections: all_dets,
      projected_truths,
      tp: tp_tick,
      fp: fp_tick,
      fn_count: fn_tick,
    });

    // 等到下一 tick
    let elapsed = t0.elapsed();
    let budget = Duration::from_millis(TICK_INTERVAL_MS);
    if elapsed < budget {
      sleep(budget - elapsed);
    }
  }

  // ── 汇总 ─────────────────────────────────────────────────────────────────
  let precision_live = if tp_total + fp_total > 0 {
    Some(tp_total as f64 / (tp_total + fp_total) as f64)
  } else {
    None
  };

  let (decision, recommendation) = match precision_live {
    None => ("INSUFFICIENT_DATA".to_string(), "no crafting_table detections captured — cannot judge threshold policy".to_string()),
    Some(p) if p >= 0.80 => {
      ("MAINTAIN_0.50".to_string(), format!("precision_live={p:.4} >= 0.80: threshold 0.50 is valid, 6e policy unchanged"))
    }
    Some(p) => (
      "RECOMMEND_0.70".to_string(),
      format!(
        "precision_live={p:.4} < 0.80: recommend raising crafting_table threshold to 0.70 — \
         execute separately after owner review"
      ),
    ),
  };

  println!();
  println!("============================================================");
  println!("  crafting_table 对抗场景最终结果");
  println!("============================================================");
  println!("Ticks:          {}", args.ticks);
  println!("CT detections:  {}", tp_total + fp_total);
  println!("TP:             {tp_total}");
  println!("FP:             {fp_total}");
  println!("FN:             {fn_total}");
  if let Some(p) = precision_live {
    println!("precision_live: {p:.4}");
  } else {
    println!("precision_live: N/A (no detections)");
  }
  let all_truths_observed = GROUND_TRUTH.iter().all(|(lbl, _)| truth_frustum_counts.get(*lbl).copied().unwrap_or(0) > 0);

  println!("All 5 truths observed in frustum: {all_truths_observed}");
  for (lbl, _) in GROUND_TRUTH {
    println!("  {lbl}: observed in frustum {} tick(s)", truth_frustum_counts.get(*lbl).copied().unwrap_or(0));
  }
  println!("Decision:       {decision}");
  println!("Recommendation: {recommendation}");
  println!("============================================================");

  // FP 样本（最多 20）
  let fp_sample: Vec<DetectionRecord> =
    tick_logs.iter().flat_map(|t| t.detections.iter()).filter(|d| d.classification == "FP").take(20).cloned().collect();

  let report = AdversarialReport {
    schema_version: 1,
    generated_at: {
      use std::time::{SystemTime, UNIX_EPOCH};
      let secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
      format!("unix_ts={secs}")
    },
    game_mode: "creative".to_string(),
    ground_truth: GROUND_TRUTH.iter().map(|(l, (x, y, z))| (l.to_string(), [*x, *y, *z])).collect(),
    conf_threshold: CONF_THRESH,
    match_radius_px: MATCH_RADIUS_PX,
    ticks: args.ticks,
    tp_total,
    fp_total,
    fn_total,
    precision_live,
    all_truths_observed,
    truth_frustum_counts: truth_frustum_counts.clone(),
    decision: decision.clone(),
    recommendation: recommendation.clone(),
    fp_sample,
    tick_logs,
  };

  if let Some(parent) = args.report_out.parent() {
    let _ = fs::create_dir_all(parent);
  }
  let json = serde_json::to_string_pretty(&report).map_err(|e| format!("json: {e}"))?;
  fs::write(&args.report_out, json).map_err(|e| format!("write report: {e}"))?;
  println!("\n报告已写入: {}", args.report_out.display());

  Ok(())
}
