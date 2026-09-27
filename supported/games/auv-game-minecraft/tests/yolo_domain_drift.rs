//! YOLO-World 在 Minecraft 截图上的域漂移实测。
//!
//! 真值依据：
//! - v01/v02 的 grass_block 在屏幕中心附近（raycast 确认坐标 (-22, 81, 43)）。
//! - v03 为指天画面，无实心方块命中（raycast_hit = None），应无任何有效检出。
//!
//! prompt 列表为严格实验条件（测试定死，跑前确定，跑后不改）：
//! - 白名单：作战守则 #3 的生产 prompt
//! - 探针：已知幻觉类别 "bed"（Step 4 审计发现的误报类别）

use std::path::PathBuf;

use auv_game_minecraft::visual_perception::{YoloWorldConfig, YoloWorldDetector};

// 白名单（作战守则 #3 的生产 prompt）
pub const WHITELIST_PROMPTS: &[&str] = &[
  "grass block",
  "chest",
  "furnace",
  "crafting table",
  "door",
  "torch",
];

// 已知幻觉探针（Step 4 审计发现的误报类别，用来验证复现与否）
pub const DISTRACTOR_PROMPTS: &[&str] = &["bed"];

const M2_PROMPTS_EMBEDS_JSON: &str = include_str!("../assets/m2_prompts_embeds.json");

#[test]
fn yolo_domain_drift_on_m2_screenshots() {
  let model_path = PathBuf::from("F:/auv/.tmp/models/yolov8s-worldv2.onnx");
  assert!(model_path.is_file(), "YOLO-World ONNX model must exist at F:/auv/.tmp/models/yolov8s-worldv2.onnx");

  let mut all_prompts: Vec<String> = Vec::new();
  for p in WHITELIST_PROMPTS {
    all_prompts.push(p.to_string());
  }
  for p in DISTRACTOR_PROMPTS {
    all_prompts.push(p.to_string());
  }

  // Initialise YoloWorldDetector with low threshold (0.05) to capture raw detections
  let config = YoloWorldConfig {
    model_path,
    confidence_threshold: 0.05,
    iou_threshold: 0.45,
    input_size: 640,
    classes: all_prompts.clone(),
  };

  let detector = YoloWorldDetector::new(config)
    .expect("failed to load YOLO-World detector")
    .with_embeddings_json(M2_PROMPTS_EMBEDS_JSON)
    .expect("failed to load M2 prompt text embeddings");

  let frames = ["v01", "v02", "v03"];
  let screen_center = (427.0f64, 240.0f64);

  println!("\n================================================================================");
  println!("          YOLO-WORLD DOMAIN DRIFT EMPIRICAL MEASUREMENT ON MINECRAFT             ");
  println!("================================================================================\n");

  println!("Experimental Prompts (FROZEN):");
  println!("  Whitelist: {:?}", WHITELIST_PROMPTS);
  println!("  Distractor probe: {:?}", DISTRACTOR_PROMPTS);
  println!("  Screen center: ({:.1}, {:.1}) (854x480)\n", screen_center.0, screen_center.1);

  let mut summary_rows = Vec::new();

  for &frame_id in &frames {
    let img_path = format!("F:/auv/.tmp/m2-session/{frame_id}/screenshot.png");
    assert!(PathBuf::from(&img_path).is_file(), "screenshot must exist: {}", img_path);

    let img = image::open(&img_path).unwrap_or_else(|e| panic!("failed to open screenshot {img_path}: {e}"));
    let orig_w = img.width() as f64;
    let orig_h = img.height() as f64;

    // 1. Max confidence across entire frame for each prompt class
    let max_scores = detector.max_scores_by_class(&img).expect("max scores by class");
    let bed_max_conf = max_scores.iter().find(|(l, _)| l == "bed").map(|(_, s)| *s).unwrap_or(0.0);
    let grass_max_conf = max_scores.iter().find(|(l, _)| l == "grass block").map(|(_, s)| *s).unwrap_or(0.0);

    // 2. Raw detections (threshold >= 0.05)
    let raw_detections = detector.detect(&img).expect("detect raw");

    println!("--------------------------------------------------------------------------------");
    println!(">>> Frame: {} (dimensions: {}x{}) <<<", frame_id, orig_w, orig_h);
    println!("  Peak conf by prompt across all anchors:");
    for (label, conf) in &max_scores {
      println!("    - {:15}: {:.4}", label, conf);
    }

    println!("\n  Raw Detections (conf >= 0.05, NMS applied) [total = {}]:", raw_detections.len());
    if raw_detections.is_empty() {
      println!("    (none)");
    } else {
      for (i, det) in raw_detections.iter().enumerate() {
        let covers_center =
          det.bbox.0 <= screen_center.0 && screen_center.0 <= det.bbox.2 && det.bbox.1 <= screen_center.1 && screen_center.1 <= det.bbox.3;
        println!(
          "    #{:02}: label=\"{}\", conf={:.4}, bbox=({:.1}, {:.1}, {:.1}, {:.1}), covers_center={}",
          i + 1,
          det.label,
          det.confidence,
          det.bbox.0,
          det.bbox.1,
          det.bbox.2,
          det.bbox.3,
          covers_center
        );
      }
    }

    // 3. Apply operational gating: conf >= 0.50 && label in WHITELIST_PROMPTS
    let survivors: Vec<_> =
      raw_detections.iter().filter(|det| det.confidence >= 0.50 && WHITELIST_PROMPTS.contains(&det.label.as_str())).cloned().collect();

    println!("\n  Survivors after Gating (conf >= 0.50 + Whitelist) [total = {}]:", survivors.len());
    if survivors.is_empty() {
      println!("    (none)");
    } else {
      for (i, det) in survivors.iter().enumerate() {
        let covers_center =
          det.bbox.0 <= screen_center.0 && screen_center.0 <= det.bbox.2 && det.bbox.1 <= screen_center.1 && screen_center.1 <= det.bbox.3;
        println!(
          "    #{:02}: label=\"{}\", conf={:.4}, bbox=({:.1}, {:.1}, {:.1}, {:.1}), covers_center={}",
          i + 1,
          det.label,
          det.confidence,
          det.bbox.0,
          det.bbox.1,
          det.bbox.2,
          det.bbox.3,
          covers_center
        );
      }
    }

    // 4. Verdict determination
    let verdict = match frame_id {
      "v01" | "v02" => {
        let hit = survivors.iter().any(|det| {
          det.label == "grass block"
            && det.bbox.0 <= screen_center.0
            && screen_center.0 <= det.bbox.2
            && det.bbox.1 <= screen_center.1
            && screen_center.1 <= det.bbox.3
        });
        if hit { "HIT" } else { "MISS" }
      }
      "v03" => {
        if survivors.is_empty() {
          "CLEAN"
        } else {
          "FALSE_POSITIVE"
        }
      }
      _ => "UNKNOWN",
    };

    let summary_line = format!(
      "frame {}: raw_detections={} | survivors_after_gate={} | bed_probe_max_conf={:.4} | grass_block_max_conf={:.4} | verdict={}",
      frame_id,
      raw_detections.len(),
      survivors.len(),
      bed_max_conf,
      grass_max_conf,
      verdict
    );

    println!("\n  >>> VERDICT: {} <<<", verdict);
    println!("  {}", summary_line);
    summary_rows.push(summary_line);
  }

  println!("\n================================================================================");
  println!("                             FINAL ADJUDICATION TABLE                           ");
  println!("================================================================================");
  for row in &summary_rows {
    println!("{}", row);
  }
  println!("================================================================================\n");
}
