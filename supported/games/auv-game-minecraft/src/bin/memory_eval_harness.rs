use std::fs;
use std::path::{Path, PathBuf};

use auv_game_minecraft::spatial_memory_query::{LandmarkTarget, QueryKind, VisibilityClass};
use auv_game_minecraft::spatial_memory_store::{ObservationRef, SpatialMemoryConfig, SpatialMemoryStore};
use auv_game_minecraft::spatial_memory_tool::{
    MemoryGetInput, MemoryQueryInput, MemorySearchInput, memory_get, memory_query, memory_search,
};
use auv_game_minecraft::types::{BlockFace, BlockPosition, PlayerPose, RaycastHit, Vec3, Viewport};
use serde::{Deserialize, Serialize};

fn normalize_angle_deg(delta: f64) -> f64 {
    let mut normalized = delta % 360.0;
    if normalized > 180.0 {
        normalized -= 360.0;
    } else if normalized <= -180.0 {
        normalized += 360.0;
    }
    normalized
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CaptureFrameRecord {
    pub frame_index: usize,
    pub pass_index: usize,
    pub relative_image_path: String,
    pub eye_pos: [f64; 3],
    pub yaw: f64,
    pub pitch: f64,
    pub timestamp_ms: u64,
    pub skew_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvalQuestionType {
    T1RelativeBearing,
    T2CounterfactualFrustum,
    T3RecallBehind,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClosestLandmarkGt {
    pub landmark_id: String,
    pub label: String,
    pub position: BlockPosition,
    pub distance_m: f64,
    pub target_yaw: f64,
    pub yaw_delta: f64,
    pub quadrant: String, // "前" | "后" | "左" | "右"
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FrustumGt {
    pub counterfactual_yaw: f64,
    pub delta_yaw: f64,
    pub visible_landmarks: Vec<String>,
    pub visible_labels: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecallBehindGt {
    pub target_label: String,
    pub has_target_behind: bool,
    pub matching_landmarks: Vec<ClosestLandmarkGt>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EvalQuestion {
    pub id: String,
    pub question_type: EvalQuestionType,
    pub frame_index: usize,
    pub image_path: String,
    pub observer_pose: PlayerPose,
    pub prompt_text: String,
    pub closest_landmark_gt: Option<ClosestLandmarkGt>,
    pub frustum_gt: Option<FrustumGt>,
    pub recall_behind_gt: Option<RecallBehindGt>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EvalDataset {
    pub arena_name: String,
    pub total_questions: usize,
    pub questions: Vec<EvalQuestion>,
}

fn determine_quadrant(yaw_delta: f64) -> String {
    let norm = normalize_angle_deg(yaw_delta);
    let abs = norm.abs();
    if abs <= 45.0 {
        "前".to_string()
    } else if norm > 45.0 && norm < 135.0 {
        "右".to_string()
    } else if norm < -45.0 && norm > -135.0 {
        "左".to_string()
    } else {
        "后".to_string()
    }
}

fn build_frozen_store(store_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = store_path.parent() {
        fs::create_dir_all(parent)?;
    }
    if store_path.exists() {
        let _ = fs::remove_file(store_path);
    }

    let mut store = SpatialMemoryStore::open_with_config(store_path, SpatialMemoryConfig::default())?;

    let now_millis = 2453830u64;
    let obs_ref = ObservationRef {
        observation_id: "telemetry-init-pass".to_string(),
        captured_at_millis: now_millis,
    };

    // 1. Chests in GT Calibration Arena:
    // West chest: (-2, 95, -3)
    // Center chest: (0, 95, -3)
    let chests = [((-2, 95, -3), "chest-west"), ((0, 95, -3), "chest-center")];
    for ((x, y, z), _name) in chests {
        let hit = RaycastHit {
            block_pos: BlockPosition::new(x, y, z),
            face: BlockFace::South,
            block_id: "minecraft:chest".to_string(),
        };
        store.upsert_from_raycast(&hit, &obs_ref);
    }

    // 2. Stone Wall: Z = -5, X in [-2, 2], Y in [95, 98]
    for x in -2..=2 {
        for y in 95..=98 {
            let hit = RaycastHit {
                block_pos: BlockPosition::new(x, y, -5),
                face: BlockFace::South,
                block_id: "minecraft:stone".to_string(),
            };
            store.upsert_from_raycast(&hit, &obs_ref);
        }
    }

    // 3. Cobblestone Step: Z = -3, X in [1, 2], Y in [95, 96]
    for x in 1..=2 {
        for y in 95..=96 {
            let hit = RaycastHit {
                block_pos: BlockPosition::new(x, y, -3),
                face: BlockFace::South,
                block_id: "minecraft:cobblestone".to_string(),
            };
            store.upsert_from_raycast(&hit, &obs_ref);
        }
    }

    // 4. Ground grass blocks:
    for z in [-8, -4, 0, 4, 8, 12] {
        for x in [-2, 0, 2] {
            let hit = RaycastHit {
                block_pos: BlockPosition::new(x, 94, z),
                face: BlockFace::Up,
                block_id: "minecraft:grass_block".to_string(),
            };
            store.upsert_from_raycast(&hit, &obs_ref);
        }
    }

    store.save()?;
    println!("[Harness] Frozen store built successfully at {} with {} landmarks", store_path.display(), store.len());
    Ok(())
}

fn generate_dataset(metadata_path: &Path, store_path: &Path, output_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let meta_content = fs::read_to_string(metadata_path)?;
    let frames: Vec<CaptureFrameRecord> = serde_json::from_str(&meta_content)?;

    let store = SpatialMemoryStore::open(store_path)?;
    println!("[Harness] Loaded {} landmarks from store", store.len());

    let mut questions: Vec<EvalQuestion> = Vec::new();

    // Helper to compute chest GT for a pose
    let compute_chest_relative = |pose: &PlayerPose| -> (ClosestLandmarkGt, Vec<ClosestLandmarkGt>) {
        let mut all_chests = Vec::new();
        let chest_candidates = [
            BlockPosition::new(0, 95, -3),
            BlockPosition::new(-2, 95, -3),
        ];
        for pos in chest_candidates {
            let center = pos.center();
            let dx = center.x - pose.eye_position.x;
            let dy = center.y - pose.eye_position.y;
            let dz = center.z - pose.eye_position.z;
            let dist = (dx * dx + dy * dy + dz * dz).sqrt();
            let target_yaw = (-dx).atan2(dz).to_degrees();
            let yaw_delta = normalize_angle_deg(target_yaw - pose.yaw);
            let quad = determine_quadrant(yaw_delta);
            let lm_id = format!("lm-{}-{}-{}-static", pos.x, pos.y, pos.z);
            all_chests.push(ClosestLandmarkGt {
                landmark_id: lm_id,
                label: "chest".to_string(),
                position: pos,
                distance_m: (dist * 100.0).round() / 100.0,
                target_yaw: (target_yaw * 10.0).round() / 10.0,
                yaw_delta: (yaw_delta * 10.0).round() / 10.0,
                quadrant: quad,
            });
        }
        all_chests.sort_by(|a, b| a.distance_m.partial_cmp(&b.distance_m).unwrap());
        (all_chests[0].clone(), all_chests)
    };

    // Helper to compute frustum visibility
    let compute_frustum_vis = |pose: &PlayerPose, delta_yaw: f64| -> FrustumGt {
        let mut cf_pose = pose.clone();
        cf_pose.yaw = normalize_angle_deg(pose.yaw + delta_yaw);
        let mut visible_ids = Vec::new();
        let mut visible_labels = Vec::new();

        for lm in store.landmarks().values() {
            let query_in = MemoryQueryInput {
                target: LandmarkTarget::LandmarkId(lm.landmark_id.clone()),
                query_kind: QueryKind::Visibility,
                observer_pose: cf_pose.clone(),
                viewport: Some(Viewport::new(854, 480)),
                vertical_fov_deg: Some(70.0),
                now_millis: Some(2453830),
            };
            let ans = memory_query(&store, &query_in, None);
            if ans.visibility == VisibilityClass::Visible {
                visible_ids.push(lm.landmark_id.clone());
                let label = lm
                    .observations
                    .first()
                    .and_then(|o| o.block_id.as_deref())
                    .unwrap_or("unknown")
                    .strip_prefix("minecraft:")
                    .unwrap_or("unknown")
                    .to_string();
                visible_labels.push(label);
            }
        }
        FrustumGt {
            counterfactual_yaw: cf_pose.yaw,
            delta_yaw,
            visible_landmarks: visible_ids,
            visible_labels,
        }
    };

    // Helper to compute behind recall
    let compute_behind = |pose: &PlayerPose, max_dist: f64, label: &str| -> RecallBehindGt {
        let mut behind = Vec::new();
        let candidates = match label {
            "chest" => vec![
                BlockPosition::new(0, 95, -3),
                BlockPosition::new(-2, 95, -3),
            ],
            _ => vec![],
        };
        for pos in candidates {
            let center = pos.center();
            let dx = center.x - pose.eye_position.x;
            let dy = center.y - pose.eye_position.y;
            let dz = center.z - pose.eye_position.z;
            let dist = (dx * dx + dy * dy + dz * dz).sqrt();
            let target_yaw = (-dx).atan2(dz).to_degrees();
            let yaw_delta = normalize_angle_deg(target_yaw - pose.yaw);
            let quad = determine_quadrant(yaw_delta);
            if (quad == "后" || yaw_delta.abs() >= 90.0) && dist <= max_dist {
                let lm_id = format!("lm-{}-{}-{}-static", pos.x, pos.y, pos.z);
                behind.push(ClosestLandmarkGt {
                    landmark_id: lm_id,
                    label: label.to_string(),
                    position: pos,
                    distance_m: (dist * 100.0).round() / 100.0,
                    target_yaw: (target_yaw * 10.0).round() / 10.0,
                    yaw_delta: (yaw_delta * 10.0).round() / 10.0,
                    quadrant: quad,
                });
            }
        }
        behind.sort_by(|a, b| a.distance_m.partial_cmp(&b.distance_m).unwrap());
        RecallBehindGt {
            target_label: label.to_string(),
            has_target_behind: !behind.is_empty(),
            matching_landmarks: behind,
        }
    };

    // Question specifications across diverse frames (22 questions total):
    let sampled_indices = [0, 10, 20, 24, 25, 35, 45, 49, 50, 60, 65, 70, 72, 74];

    let mut q_counter = 1usize;

    for &idx in &sampled_indices {
        let frame = &frames[idx];
        let pose = PlayerPose {
            eye_position: Vec3::new(frame.eye_pos[0], frame.eye_pos[1], frame.eye_pos[2]),
            yaw: frame.yaw,
            pitch: frame.pitch,
        };

        // T1 Relative Bearing (8 questions: 0, 20, 25, 35, 45, 60, 70, 74)
        if [0, 20, 25, 35, 45, 60, 70, 74].contains(&idx) {
            let (closest, _) = compute_chest_relative(&pose);
            questions.push(EvalQuestion {
                id: format!("Q{:02}", q_counter),
                question_type: EvalQuestionType::T1RelativeBearing,
                frame_index: idx,
                image_path: frame.relative_image_path.clone(),
                observer_pose: pose.clone(),
                prompt_text: format!(
                    "你当前在 Minecraft 中，位姿为 eye_pos=({:.2}, {:.2}, {:.2}), yaw={:.1}°, pitch={:.1}°。\n请问：离你最近的箱子在什么方位（前/后/左/右）？距离几米？\n请给出方位和距离。",
                    pose.eye_position.x, pose.eye_position.y, pose.eye_position.z, pose.yaw, pose.pitch
                ),
                closest_landmark_gt: Some(closest),
                frustum_gt: None,
                recall_behind_gt: None,
            });
            q_counter += 1;
        }

        // T2 Counterfactual Frustum (7 questions: 10, 24, 35, 49, 60, 65, 72)
        if [10, 24, 35, 49, 60, 65, 72].contains(&idx) {
            let delta_yaw = if idx == 72 {
                180.0
            } else if idx % 2 == 0 {
                90.0
            } else {
                -90.0
            };
            let frustum_gt = compute_frustum_vis(&pose, delta_yaw);
            let turn_desc = if delta_yaw == 180.0 {
                "原地调头 180°"
            } else if delta_yaw > 0.0 {
                "原地向右转 90°"
            } else {
                "原地向左转 90°"
            };
            questions.push(EvalQuestion {
                id: format!("Q{:02}", q_counter),
                question_type: EvalQuestionType::T2CounterfactualFrustum,
                frame_index: idx,
                image_path: frame.relative_image_path.clone(),
                observer_pose: pose.clone(),
                prompt_text: format!(
                    "你当前在 Minecraft 中，位姿为 eye_pos=({:.2}, {:.2}, {:.2}), yaw={:.1}°, pitch={:.1}°。\n如果现在你{}（新 yaw 为 {:.1}°），你的视野视锥内能看到什么地标（如箱子）？\n请列出可见地标及其大致位置或 ID，若无则回答无。",
                    pose.eye_position.x, pose.eye_position.y, pose.eye_position.z, pose.yaw, pose.pitch,
                    turn_desc, frustum_gt.counterfactual_yaw
                ),
                closest_landmark_gt: None,
                frustum_gt: Some(frustum_gt),
                recall_behind_gt: None,
            });
            q_counter += 1;
        }

        // T3 Recall Behind (7 questions: 0, 24, 50, 65, 70, 72, 74)
        if [0, 24, 50, 65, 70, 72, 74].contains(&idx) {
            let is_stone_probe = idx == 74;
            let label = if is_stone_probe { "stone" } else { "chest" };
            let behind_gt = compute_behind(&pose, 5.0, label);
            let item_name = if is_stone_probe {
                "石头墙"
            } else {
                "箱子"
            };
            questions.push(EvalQuestion {
                id: format!("Q{:02}", q_counter),
                question_type: EvalQuestionType::T3RecallBehind,
                frame_index: idx,
                image_path: frame.relative_image_path.clone(),
                observer_pose: pose.clone(),
                prompt_text: format!(
                    "你当前在 Minecraft 中，位姿为 eye_pos=({:.2}, {:.2}, {:.2}), yaw={:.1}°, pitch={:.1}°。\n请问：你背后 5 米内有没有{}？如果有，具体在哪个坐标？\n请回答有或无，并给出具体坐标与距离（若有）。",
                    pose.eye_position.x, pose.eye_position.y, pose.eye_position.z, pose.yaw, pose.pitch,
                    item_name
                ),
                closest_landmark_gt: None,
                frustum_gt: None,
                recall_behind_gt: Some(behind_gt),
            });
            q_counter += 1;
        }
    }

    let dataset = EvalDataset {
        arena_name: "traj_geo_gt_arena".to_string(),
        total_questions: questions.len(),
        questions,
    };

    if let Some(p) = output_path.parent() {
        fs::create_dir_all(p)?;
    }
    let json_str = serde_json::to_string_pretty(&dataset)?;
    fs::write(output_path, json_str)?;

    println!("[Harness] Eval dataset generated with {} questions at {}", dataset.total_questions, output_path.display());
    Ok(())
}

fn execute_tool(store_path: &Path, tool_name: &str, input_json: &str) -> Result<String, Box<dyn std::error::Error>> {
    let store = SpatialMemoryStore::open(store_path)?;

    match tool_name {
        "memory_search" => {
            let input: MemorySearchInput = serde_json::from_str(input_json).map_err(|e| format!("Invalid MemorySearchInput JSON: {e}"))?;
            let output = memory_search(&store, &input);
            Ok(serde_json::to_string_pretty(&output)?)
        }
        "memory_query" => {
            let input: MemoryQueryInput = serde_json::from_str(input_json).map_err(|e| format!("Invalid MemoryQueryInput JSON: {e}"))?;
            let output = memory_query(&store, &input, None);
            Ok(serde_json::to_string_pretty(&output)?)
        }
        "memory_get" => {
            let input: MemoryGetInput = serde_json::from_str(input_json).map_err(|e| format!("Invalid MemoryGetInput JSON: {e}"))?;
            let output = memory_get(&store, &input);
            Ok(serde_json::to_string_pretty(&output)?)
        }
        other => Err(format!("Unknown tool: {other}. Available: memory_search, memory_query, memory_get").into()),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: memory_eval_harness <command> [options]");
        eprintln!("Commands:");
        eprintln!("  --build-store --output <store.json>");
        eprintln!("  --generate-dataset --metadata <metadata.json> --store <store.json> --output <dataset.json>");
        eprintln!("  --tool-exec --store <store.json> --tool <tool_name> --input '<json>'");
        return Ok(());
    }

    let command = &args[1];
    match command.as_str() {
        "--build-store" => {
            let out_idx = args.iter().position(|a| a == "--output").expect("--output required");
            let out_path = PathBuf::from(&args[out_idx + 1]);
            build_frozen_store(&out_path)?;
        }
        "--generate-dataset" => {
            let meta_idx = args.iter().position(|a| a == "--metadata").expect("--metadata required");
            let store_idx = args.iter().position(|a| a == "--store").expect("--store required");
            let out_idx = args.iter().position(|a| a == "--output").expect("--output required");
            let meta_path = PathBuf::from(&args[meta_idx + 1]);
            let store_path = PathBuf::from(&args[store_idx + 1]);
            let out_path = PathBuf::from(&args[out_idx + 1]);
            generate_dataset(&meta_path, &store_path, &out_path)?;
        }
        "--tool-exec" => {
            let store_idx = args.iter().position(|a| a == "--store").expect("--store required");
            let tool_idx = args.iter().position(|a| a == "--tool").expect("--tool required");
            let input_idx = args.iter().position(|a| a == "--input").expect("--input required");
            let store_path = PathBuf::from(&args[store_idx + 1]);
            let tool_name = &args[tool_idx + 1];
            let input_json = &args[input_idx + 1];
            let res = execute_tool(&store_path, tool_name, input_json)?;
            println!("{res}");
        }
        other => {
            eprintln!("Unknown command: {other}");
        }
    }

    Ok(())
}
