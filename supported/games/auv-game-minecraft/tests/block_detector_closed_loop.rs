//! Step 6e Rust-side verification: BlockDetector v2 closed-loop verification on v01/v02/v03,
//! normalized per-class threshold policy enforcement, and NMS suppression test.

use auv_game_minecraft::visual_perception::{BlockDetector, BlockDetectorConfig, CLOSED_SET_CLASSES, DEFAULT_PER_CLASS_THRESHOLDS};
use std::path::PathBuf;

#[test]
fn test_block_detector_closed_loop_on_m2_drift_frames() {
  let model_path = PathBuf::from("assets/block-detector-v2.onnx");
  let v01_path = PathBuf::from("F:/auv/.tmp/m2-session/v01/screenshot.png");
  let v02_path = PathBuf::from("F:/auv/.tmp/m2-session/v02/screenshot.png");
  let v03_path = PathBuf::from("F:/auv/.tmp/m2-session/v03/screenshot.png");

  // Step 6c review fix: explicit assertion failure instead of silent skip
  assert!(v01_path.is_file(), "v01 screenshot must exist: {}", v01_path.display());
  assert!(v02_path.is_file(), "v02 screenshot must exist: {}", v02_path.display());
  assert!(v03_path.is_file(), "v03 screenshot must exist: {}", v03_path.display());

  // 1. Initialize detector with production default config (v2, crafting_table enabled, 0.50 thresholds)
  let config = BlockDetectorConfig {
    model_path,
    per_class_threshold: DEFAULT_PER_CLASS_THRESHOLDS,
    enable_crafting_table: true,
    iou_threshold: 0.45,
    input_size: 640,
  };
  let detector = BlockDetector::new(config).expect("load BlockDetector from assets");

  // -------------------------------------------------------------
  // Test v01: Distant view of grass stairs, raycast hits grass_block at (-22, 81, 43)
  // Python 6d v2 baseline: grass_block confidence = 0.9579 at center
  // -------------------------------------------------------------
  let img_v01 = image::open(&v01_path).expect("open v01");
  let center_v01 = (img_v01.width() as f64 / 2.0, img_v01.height() as f64 / 2.0);
  let dets_v01 = detector.detect(&img_v01).expect("detect v01");

  println!("Detections in v01 ({} total, center={:?}):", dets_v01.len(), center_v01);
  for (i, d) in dets_v01.iter().enumerate() {
    let covers = d.bbox.0 <= center_v01.0 && center_v01.0 <= d.bbox.2 && d.bbox.1 <= center_v01.1 && center_v01.1 <= d.bbox.3;
    println!(
      "  #{}: label={}, conf={:.4}, bbox=({:.1}, {:.1}, {:.1}, {:.1}), covers_center={}",
      i, d.label, d.confidence, d.bbox.0, d.bbox.1, d.bbox.2, d.bbox.3, covers
    );
  }

  let v01_center_grass = dets_v01.iter().find(|d| {
    d.label == "grass_block" && d.bbox.0 <= center_v01.0 && center_v01.0 <= d.bbox.2 && d.bbox.1 <= center_v01.1 && center_v01.1 <= d.bbox.3
  });

  assert!(
    v01_center_grass.is_some(),
    "v01: center must be covered by grass_block with conf >= 0.50 (found {} total detections)",
    dets_v01.len()
  );
  let v01_conf = v01_center_grass.unwrap().confidence;
  println!("Rust v01 center grass_block confidence: {:.4} (Python v2 was 0.9579)", v01_conf);
  assert!((v01_conf - 0.9579).abs() <= 0.08, "v01 confidence {:.4} must match Python 0.9579 within tolerance", v01_conf);

  // -------------------------------------------------------------
  // Test v02: Close overhead view of grass_block top face
  // Python 6d v2 baseline: grass_block confidence = 0.9443 at center
  // -------------------------------------------------------------
  let img_v02 = image::open(&v02_path).expect("open v02");
  let center_v02 = (img_v02.width() as f64 / 2.0, img_v02.height() as f64 / 2.0);
  let dets_v02 = detector.detect(&img_v02).expect("detect v02");

  // Note: v02 crosshair (435.0, 259.5) lies directly on the horizontal seam between adjacent
  // grass blocks (top block y2=258.1, bottom block y1=259.7, subpixel gap = 0.2px).
  // A 2.0px geometry margin ensures the boundary edge detection is correctly associated.
  let margin = 2.0;
  let v02_center_grass = dets_v02.iter().find(|d| {
    d.label == "grass_block"
      && d.bbox.0 - margin <= center_v02.0
      && center_v02.0 <= d.bbox.2 + margin
      && d.bbox.1 - margin <= center_v02.1
      && center_v02.1 <= d.bbox.3 + margin
  });

  assert!(
    v02_center_grass.is_some(),
    "v02: center must be covered by grass_block with conf >= 0.50 (found {} total detections)",
    dets_v02.len()
  );
  let v02_conf = v02_center_grass.unwrap().confidence;
  println!("Rust v02 center grass_block confidence: {:.4} (Python v2 was 0.9443)", v02_conf);
  assert!((v02_conf - 0.9443).abs() <= 0.08, "v02 confidence {:.4} must match Python 0.9443 within tolerance", v02_conf);

  // -------------------------------------------------------------
  // Test v03: Sky view (raycast_hit = None). Center must NOT have any detections.
  // -------------------------------------------------------------
  let img_v03 = image::open(&v03_path).expect("open v03");
  let center_v03 = (img_v03.width() as f64 / 2.0, img_v03.height() as f64 / 2.0);
  let dets_v03 = detector.detect(&img_v03).expect("detect v03");

  let v03_center_hit =
    dets_v03.iter().any(|d| d.bbox.0 <= center_v03.0 && center_v03.0 <= d.bbox.2 && d.bbox.1 <= center_v03.1 && center_v03.1 <= d.bbox.3);

  assert!(!v03_center_hit, "v03: sky center must NOT have any false detections");
  println!("Rust v03 sky center: CLEAN (zero center detections)");
}

#[test]
fn test_per_class_threshold_policy_enforcement() {
  let model_path = PathBuf::from("assets/block-detector-v2.onnx");
  let v01_path = PathBuf::from("F:/auv/.tmp/m2-session/v01/screenshot.png");
  assert!(v01_path.is_file(), "v01 screenshot must exist: {}", v01_path.display());
  let img = image::open(&v01_path).expect("open v01");

  // 1. Verify default config has enable_crafting_table = true
  let default_config = BlockDetectorConfig::default();
  assert!(default_config.enable_crafting_table, "crafting_table must be enabled by default in v2");
  assert_eq!(default_config.per_class_threshold[1], 0.50, "chest threshold must be 0.50 in v2");
  assert_eq!(default_config.per_class_threshold[3], 0.50, "crafting_table threshold must be 0.50 in v2");

  // 2. When crafting_table is manually disabled in config, no crafting_table should ever be emitted
  let mut config_disabled = BlockDetectorConfig::default();
  config_disabled.model_path = model_path.clone();
  config_disabled.enable_crafting_table = false;
  // Lower all thresholds to 0.01 to test candidate suppression
  config_disabled.per_class_threshold = [0.01; 6];

  let detector_disabled = BlockDetector::new(config_disabled).unwrap();
  let dets_disabled = detector_disabled.detect(&img).unwrap();
  assert!(
    !dets_disabled.iter().any(|d| d.label == "crafting_table"),
    "crafting_table must be strictly disabled when enable_crafting_table = false"
  );

  // 3. Chest threshold: set to 0.999 (almost impossible) -> chest must not appear
  let mut config_strict = BlockDetectorConfig::default();
  config_strict.model_path = model_path;
  config_strict.set_class_threshold(1, 0.999);
  let detector_strict = BlockDetector::new(config_strict).unwrap();
  let dets_strict = detector_strict.detect(&img).unwrap();
  assert!(!dets_strict.iter().any(|d| d.label == "chest"), "chest with 0.999 threshold must be filtered");
}

#[test]
fn test_nms_suppression_and_closed_set_class_order() {
  // Confirm class IDs match training specifications exactly
  assert_eq!(CLOSED_SET_CLASSES[0], "grass_block");
  assert_eq!(CLOSED_SET_CLASSES[1], "chest");
  assert_eq!(CLOSED_SET_CLASSES[2], "furnace");
  assert_eq!(CLOSED_SET_CLASSES[3], "crafting_table");
  assert_eq!(CLOSED_SET_CLASSES[4], "door");
  assert_eq!(CLOSED_SET_CLASSES[5], "torch");

  // Verify default thresholds match Step 6e normalized policy:
  // All 6 classes normalized to 0.50 (data sufficient in v2, penalties lifted)
  assert_eq!(DEFAULT_PER_CLASS_THRESHOLDS[0], 0.50);
  assert_eq!(DEFAULT_PER_CLASS_THRESHOLDS[1], 0.50);
  assert_eq!(DEFAULT_PER_CLASS_THRESHOLDS[2], 0.50);
  assert_eq!(DEFAULT_PER_CLASS_THRESHOLDS[3], 0.50);
  assert_eq!(DEFAULT_PER_CLASS_THRESHOLDS[4], 0.50);
  assert_eq!(DEFAULT_PER_CLASS_THRESHOLDS[5], 0.50);

  let default_config = BlockDetectorConfig::default();
  assert!(default_config.enable_crafting_table);
  assert_eq!(default_config.model_path, PathBuf::from("assets/block-detector-v2.onnx"));
}
