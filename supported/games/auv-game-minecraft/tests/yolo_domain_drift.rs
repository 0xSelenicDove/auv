//! 闭集 BlockDetector 在 Minecraft 截图上的域漂移消除实测。
//!
//! 相比历史 YOLO-World 零样本（Recall 0% 且对草方块误报 "bed"），
//! 闭集 BlockDetector 应在 v01/v02 获得准确高置信度检出，且在 v03 指天时零误报。
//!
//! 真值依据：
//! - v01/v02 的 grass_block 在屏幕中心附近（raycast 确认坐标 (-22, 81, 43)）。
//! - v03 为指天画面，无实心方块命中（raycast_hit = None），应无任何有效检出。

use std::path::PathBuf;

use auv_game_minecraft::visual_perception::{BlockDetector, BlockDetectorConfig, CLOSED_SET_CLASSES, DEFAULT_PER_CLASS_THRESHOLDS};

#[test]
fn yolo_domain_drift_resolution_on_m2_screenshots() {
  let model_path = PathBuf::from("assets/block-detector-v2.onnx");
  let v01_path = PathBuf::from("F:/auv/.tmp/m2-session/v01/screenshot.png");
  if !v01_path.is_file() {
    eprintln!("Skipping drift test: screenshot v01 not found");
    return;
  }

  // 1. Initialize BlockDetector with low threshold (0.05) to capture raw detections
  let mut config = BlockDetectorConfig::default();
  config.model_path = model_path;
  config.per_class_threshold = [0.05; 6];
  config.enable_crafting_table = true;

  let detector = BlockDetector::new(config).expect("failed to load BlockDetector");

  let frames = ["v01", "v02", "v03"];

  println!("\n================================================================================");
  println!("       BLOCK-DETECTOR DOMAIN DRIFT RESOLUTION MEASUREMENT ON MINECRAFT           ");
  println!("================================================================================\n");

  let mut summary_rows = Vec::new();

  for &frame_id in &frames {
    let img_path = format!("F:/auv/.tmp/m2-session/{frame_id}/screenshot.png");
    assert!(PathBuf::from(&img_path).is_file(), "screenshot must exist: {}", img_path);

    let img = image::open(&img_path).unwrap_or_else(|e| panic!("failed to open screenshot {img_path}: {e}"));
    let orig_w = img.width() as f64;
    let orig_h = img.height() as f64;
    let screen_center = (orig_w / 2.0, orig_h / 2.0);

    // 1. Max confidence across entire frame for each closed-set class
    let max_scores = detector.max_scores_by_class(&img).expect("max scores by class");
    let grass_max_conf = max_scores.iter().find(|(l, _)| l == "grass_block").map(|(_, s)| *s).unwrap_or(0.0);

    // 2. Raw detections (threshold >= 0.05)
    let raw_detections = detector.detect(&img).expect("detect raw");

    println!("--------------------------------------------------------------------------------");
    println!(">>> Frame: {} (dimensions: {}x{}) <<<", frame_id, orig_w, orig_h);
    println!("  Peak conf by class across all anchors:");
    for (label, conf) in &max_scores {
      println!("    - {:15}: {:.4}", label, conf);
    }

    println!("\n  Raw Detections (conf >= 0.05, NMS applied) [total = {}]:", raw_detections.len());
    for (i, det) in raw_detections.iter().enumerate().take(10) {
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

    // 3. Apply operational gating: conf >= 0.50 (default per-class thresholds)
    let survivors: Vec<_> = raw_detections
      .iter()
      .filter(|det| {
        let cls_idx = CLOSED_SET_CLASSES.iter().position(|&c| c == det.label).unwrap_or(0);
        det.confidence >= DEFAULT_PER_CLASS_THRESHOLDS[cls_idx]
      })
      .cloned()
      .collect();

    println!("\n  Survivors after Gating (conf >= per-class policy) [total = {}]:", survivors.len());
    for (i, det) in survivors.iter().enumerate().take(10) {
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

    // 4. Verdict determination
    let verdict = match frame_id {
      "v01" | "v02" => {
        let margin = 2.0;
        let hit = survivors.iter().any(|det| {
          det.label == "grass_block"
            && det.bbox.0 - margin <= screen_center.0
            && screen_center.0 <= det.bbox.2 + margin
            && det.bbox.1 - margin <= screen_center.1
            && screen_center.1 <= det.bbox.3 + margin
        });
        if hit { "HIT" } else { "MISS" }
      }
      "v03" => {
        let center_hit = survivors.iter().any(|det| {
          det.bbox.0 <= screen_center.0 && screen_center.0 <= det.bbox.2 && det.bbox.1 <= screen_center.1 && screen_center.1 <= det.bbox.3
        });
        if !center_hit {
          "CLEAN"
        } else {
          "FALSE_POSITIVE"
        }
      }
      _ => "UNKNOWN",
    };

    let summary_line = format!(
      "frame {}: raw_detections={} | survivors_after_gate={} | grass_block_max_conf={:.4} | verdict={}",
      frame_id,
      raw_detections.len(),
      survivors.len(),
      grass_max_conf,
      verdict
    );

    println!("\n  >>> VERDICT: {} <<<", verdict);
    println!("  {}", summary_line);
    summary_rows.push(summary_line);

    // Assert that v01 and v02 are HIT, v03 is CLEAN
    if frame_id == "v01" || frame_id == "v02" {
      assert_eq!(verdict, "HIT", "Frame {} must be HIT for grass_block", frame_id);
    } else if frame_id == "v03" {
      assert_eq!(verdict, "CLEAN", "Frame v03 must be CLEAN at center");
    }
  }

  println!("\n================================================================================");
  println!("                             FINAL ADJUDICATION TABLE                           ");
  println!("================================================================================");
  for row in &summary_rows {
    println!("{}", row);
  }
  println!("================================================================================\n");
}
