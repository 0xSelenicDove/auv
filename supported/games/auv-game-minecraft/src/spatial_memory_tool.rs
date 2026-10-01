//! Typed Tool API interfaces for LLM-directed spatial memory read & inspection (Slice 1).
//!
//! Conforms strictly to `docs/ai/references/3dgs/2026-10-01-spatial-memory-tool-api-spec.md`.
//!
//! Design invariants:
//! - Purely typed arguments and responses; zero natural language parsing.
//! - Completely stateless query execution; every query supplies observer pose.
//! - Joint search across semantic `description` and observation `block_id`.
//! - Never fabricates coordinates on misses; unknown targets return explicit `Unknown`.

use serde::{Deserialize, Serialize};

use crate::occlusion::MetricDepthMap;
use crate::spatial_memory_observation::SpatialClaimStatus;
use crate::spatial_memory_query::{
  AnswerStatus, FovSource, LandmarkTarget, QueryKind, SpatialMemoryAnswer, SpatialMemoryQuery, VisibilityClass, query_spatial_memory,
};
use crate::spatial_memory_store::{ObservationRef, PrunedLandmark, SpatialLandmark, SpatialMemoryStore};
use crate::types::{BlockFace, BlockPosition, PlayerPose, RaycastHit, Viewport};

// -----------------------------------------------------------------------------
// Common Primitives
// -----------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StalenessStatus {
  Fresh,
  Stale,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FreshnessInfo {
  pub last_observed_millis: u64,
  pub age_millis: u64,
  pub staleness: StalenessStatus,
}

impl FreshnessInfo {
  pub fn compute(last_observed_millis: u64, now_millis: u64, stale_threshold_millis: u64) -> Self {
    let age_millis = now_millis.saturating_sub(last_observed_millis);
    let staleness = if age_millis > stale_threshold_millis {
      StalenessStatus::Stale
    } else {
      StalenessStatus::Fresh
    };
    Self {
      last_observed_millis,
      age_millis,
      staleness,
    }
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClosedSetLabel {
  Chest,
  CraftingTable,
  Furnace,
  Door,
  Torch,
  GrassBlock,
}

impl ClosedSetLabel {
  pub fn as_str(&self) -> &'static str {
    match self {
      Self::Chest => "chest",
      Self::CraftingTable => "crafting_table",
      Self::Furnace => "furnace",
      Self::Door => "door",
      Self::Torch => "torch",
      Self::GrassBlock => "grass_block",
    }
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchSource {
  Description,
  BlockId,
  Both,
}

// -----------------------------------------------------------------------------
// Tool 1: memory_query
// -----------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MemoryQueryInput {
  pub target: LandmarkTarget,
  pub query_kind: QueryKind,
  pub observer_pose: PlayerPose,
  #[serde(default)]
  pub viewport: Option<Viewport>,
  #[serde(default)]
  pub vertical_fov_deg: Option<f64>,
  #[serde(default)]
  pub now_millis: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MemoryQueryOutput {
  pub status: AnswerStatus,
  pub visibility: VisibilityClass,
  pub screen_xy: Option<(f64, f64)>,
  pub yaw_pitch_delta: Option<(f64, f64)>,
  pub confidence: f64,
  pub evidence_observation_ids: Vec<String>,
  pub freshness: Option<FreshnessInfo>,
  pub fov_source: FovSource,
  pub effective_fov_deg: f64,
  pub limitations: Vec<String>,
}

pub fn memory_query(store: &SpatialMemoryStore, input: &MemoryQueryInput, depth_map: Option<&MetricDepthMap>) -> MemoryQueryOutput {
  let effective_viewport = input.viewport.unwrap_or_else(|| Viewport::new(854, 480));

  let q = SpatialMemoryQuery {
    observer_viewpoint: input.observer_pose.clone(),
    target: input.target.clone(),
    query_kind: input.query_kind,
    observer_frame: None,
    viewport: Some(effective_viewport),
    vertical_fov_deg: input.vertical_fov_deg,
  };

  let mut answer: SpatialMemoryAnswer = query_spatial_memory(store, &q, depth_map);

  // Compute freshness if target landmark exists and now_millis is provided
  let mut freshness = None;
  if let Some(now_ms) = input.now_millis {
    let lm = match &input.target {
      LandmarkTarget::LandmarkId(id) => store.get(id),
      LandmarkTarget::BlockPos(pos) => store.find_matching_landmark(*pos).and_then(|id| store.get(&id)),
    };
    if let Some(l) = lm {
      freshness = Some(FreshnessInfo::compute(l.last_observed_millis, now_ms, store.config().stale_threshold_millis));
    }
  }

  // Ensure mandatory honest limitations are recorded
  if depth_map.is_none() && !answer.limitations.iter().any(|l| l.contains("physical occlusion")) {
    answer.limitations.push("Unassessed physical occlusion: depth map was not provided for this query.".to_string());
  }

  MemoryQueryOutput {
    status: answer.status,
    visibility: answer.visibility,
    screen_xy: answer.screen_xy,
    yaw_pitch_delta: answer.yaw_pitch_delta,
    confidence: answer.confidence,
    evidence_observation_ids: answer.evidence_observation_ids,
    freshness,
    fov_source: answer.fov_source,
    effective_fov_deg: answer.effective_fov_deg,
    limitations: answer.limitations,
  }
}

// -----------------------------------------------------------------------------
// Tool 2: memory_search
// -----------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MemorySearchInput {
  pub label: ClosedSetLabel,
  #[serde(default)]
  pub near: Option<BlockPosition>,
  #[serde(default)]
  pub radius_m: Option<f64>,
  #[serde(default)]
  pub min_confidence: Option<f64>,
  #[serde(default)]
  pub status_filter: Option<SpatialClaimStatus>,
  #[serde(default)]
  pub now_millis: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MemorySearchItem {
  pub landmark_id: String,
  pub block_pos: BlockPosition,
  pub label: String,
  pub matched_by: MatchSource,
  pub status: SpatialClaimStatus,
  pub confidence: f64,
  pub freshness: Option<FreshnessInfo>,
  pub evidence_count: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MemorySearchOutput {
  pub items: Vec<MemorySearchItem>,
}

pub fn memory_search(store: &SpatialMemoryStore, input: &MemorySearchInput) -> MemorySearchOutput {
  let target_label = input.label.as_str();

  let candidate_landmarks: Vec<&SpatialLandmark> = match (input.near, input.radius_m) {
    (Some(center), Some(r)) => store.query_radius(center, r),
    _ => store.landmarks().values().collect(),
  };

  let min_conf = input.min_confidence.unwrap_or(0.0);
  let mut results = Vec::new();

  for lm in candidate_landmarks {
    // 1. Confidence threshold check
    if lm.confidence < min_conf {
      continue;
    }

    // 2. Status filter check
    if let Some(req_status) = input.status_filter {
      if lm.status != req_status {
        continue;
      }
    }

    // 3. Joint Search: description == label (accounting for production "label (0.xx)" format)
    //    OR stripped block_id contains label
    let match_desc = lm
      .description
      .as_deref()
      .map(|d| {
        let desc_label = d.split(" (").next().unwrap_or(d).trim();
        desc_label.eq_ignore_ascii_case(target_label)
      })
      .unwrap_or(false);

    let match_block = lm.observations.iter().filter_map(|o| o.block_id.as_deref()).any(|bid| {
      let clean = bid.strip_prefix("minecraft:").unwrap_or(bid);
      clean.contains(target_label)
    });

    let matched_by = match (match_desc, match_block) {
      (true, true) => Some(MatchSource::Both),
      (true, false) => Some(MatchSource::Description),
      (false, true) => Some(MatchSource::BlockId),
      (false, false) => None,
    };

    let Some(source) = matched_by else {
      continue;
    };

    let freshness =
      input.now_millis.map(|now_ms| FreshnessInfo::compute(lm.last_observed_millis, now_ms, store.config().stale_threshold_millis));

    results.push(MemorySearchItem {
      landmark_id: lm.landmark_id.clone(),
      block_pos: lm.position,
      label: target_label.to_string(),
      matched_by: source,
      status: lm.status,
      confidence: lm.confidence,
      freshness,
      evidence_count: lm.observation_count as usize,
    });
  }

  // Sort by confidence descending
  results.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));

  MemorySearchOutput { items: results }
}

// -----------------------------------------------------------------------------
// Tool 3: memory_get
// -----------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryGetInput {
  pub landmark_id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum MemoryGetOutput {
  Found { landmark: SpatialLandmark },
  Unknown { landmark_id: String, reason: String },
}

pub fn memory_get(store: &SpatialMemoryStore, input: &MemoryGetInput) -> MemoryGetOutput {
  match store.get(&input.landmark_id) {
    Some(lm) => MemoryGetOutput::Found {
      landmark: lm.clone(),
    },
    None => MemoryGetOutput::Unknown {
      landmark_id: input.landmark_id.clone(),
      reason: format!("Target landmark '{}' not found in spatial memory store", input.landmark_id),
    },
  }
}

// -----------------------------------------------------------------------------
// Tool 4: memory_ingest (Slice 2)
// -----------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IngestSource {
  TelemetryRaycast,
  VisualPerception,
  VlmHypothesis,
  MultiViewTriangulation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IngestDepthMethod {
  DepthModel,
  Unknown,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MemoryIngestInput {
  pub source: IngestSource,
  pub observer_pose: PlayerPose,
  #[serde(default)]
  pub block_pos: Option<BlockPosition>,
  #[serde(default)]
  pub block_face: Option<BlockFace>,
  #[serde(default)]
  pub block_id: Option<String>,
  #[serde(default)]
  pub screen_bbox: Option<[f64; 4]>,
  #[serde(default)]
  pub label: Option<ClosedSetLabel>,
  #[serde(default)]
  pub detection_confidence: Option<f64>,
  #[serde(default)]
  pub depth_m: Option<f64>,
  #[serde(default)]
  pub depth_method: Option<IngestDepthMethod>,
  #[serde(default)]
  pub viewport: Option<Viewport>,
  #[serde(default)]
  pub vertical_fov_deg: Option<f64>,
  pub observation_id: String,
  pub captured_at_millis: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MemoryIngestOutput {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub landmark_id: Option<String>,
  pub created: bool,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub status: Option<SpatialClaimStatus>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub confidence: Option<f64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub block_pos: Option<BlockPosition>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub depth_method: Option<IngestDepthMethod>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub effective_viewport: Option<Viewport>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub effective_fov_deg: Option<f64>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub limitations: Vec<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub rejected: Option<String>,
}

pub fn memory_ingest(store: &mut SpatialMemoryStore, input: &MemoryIngestInput) -> MemoryIngestOutput {
  match input.source {
    IngestSource::VlmHypothesis | IngestSource::MultiViewTriangulation => MemoryIngestOutput {
      landmark_id: None,
      created: false,
      status: None,
      confidence: None,
      block_pos: None,
      depth_method: input.depth_method,
      effective_viewport: None,
      effective_fov_deg: None,
      limitations: vec![],
      rejected: Some("unsupported_source".to_string()),
    },
    IngestSource::TelemetryRaycast => {
      let (Some(pos), Some(face), Some(bid)) = (input.block_pos, input.block_face, &input.block_id) else {
        return MemoryIngestOutput {
          landmark_id: None,
          created: false,
          status: None,
          confidence: None,
          block_pos: None,
          depth_method: input.depth_method,
          effective_viewport: None,
          effective_fov_deg: None,
          limitations: vec![],
          rejected: Some("missing_raycast_payload".to_string()),
        };
      };

      let is_created = store.find_matching_landmark(pos).is_none();
      let hit = RaycastHit {
        block_pos: pos,
        face,
        block_id: bid.clone(),
      };
      let obs = ObservationRef {
        observation_id: input.observation_id.clone(),
        captured_at_millis: input.captured_at_millis,
      };
      let landmark_id = store.upsert_from_raycast(&hit, &obs);
      let lm = store.get(&landmark_id);
      let status = lm.as_ref().map(|l| l.status);
      let confidence = lm.as_ref().map(|l| l.confidence);

      MemoryIngestOutput {
        landmark_id: Some(landmark_id),
        created: is_created,
        status,
        confidence,
        block_pos: Some(pos),
        depth_method: input.depth_method,
        effective_viewport: input.viewport,
        effective_fov_deg: input.vertical_fov_deg,
        limitations: vec![],
        rejected: None,
      }
    }
    IngestSource::VisualPerception => match input.depth_method {
      Some(IngestDepthMethod::DepthModel) => {
        let (Some(bbox), Some(label), Some(det_conf), Some(depth_m)) =
          (input.screen_bbox, input.label, input.detection_confidence, input.depth_m)
        else {
          return MemoryIngestOutput {
            landmark_id: None,
            created: false,
            status: None,
            confidence: None,
            block_pos: None,
            depth_method: input.depth_method,
            effective_viewport: None,
            effective_fov_deg: None,
            limitations: vec![],
            rejected: Some("missing_visual_payload".to_string()),
          };
        };

        // NOTICE (UNCALIBRATED_PROPOSED): Clamp detection confidence to 0.60 so that
        // store's candidate formula (det_conf * 0.5) caps confidence at <= 0.30.
        let clamped_det_conf = det_conf.min(0.60);
        let effective_viewport = input.viewport.unwrap_or_else(|| Viewport::new(854, 480));
        let effective_fov = input.vertical_fov_deg.unwrap_or(70.0);

        let cx = (bbox[0] + bbox[2]) / 2.0;
        let cy = (bbox[1] + bbox[3]) / 2.0;

        let world_pos = crate::visual_perception::back_project((cx, cy), depth_m, effective_viewport, &input.observer_pose, effective_fov);

        let block_pos = BlockPosition::new(world_pos.x.round() as i32, world_pos.y.round() as i32, world_pos.z.round() as i32);

        let is_created = store.find_matching_landmark(block_pos).is_none();
        let label_desc = format!("{} ({:.2})", label.as_str(), clamped_det_conf);
        let obs = ObservationRef {
          observation_id: input.observation_id.clone(),
          captured_at_millis: input.captured_at_millis,
        };

        let landmark_id = store.upsert_from_perception(block_pos, &label_desc, clamped_det_conf, &obs);
        let lm = store.get(&landmark_id);
        let status = lm.as_ref().map(|l| l.status);
        let confidence = lm.as_ref().map(|l| l.confidence);

        let limitations = vec![
          "uncalibrated_monocular_depth_scale".to_string(),
          "back_projection_error_bound_m: 2.0".to_string(),
        ];

        MemoryIngestOutput {
          landmark_id: Some(landmark_id),
          created: is_created,
          status,
          confidence,
          block_pos: Some(block_pos),
          depth_method: Some(IngestDepthMethod::DepthModel),
          effective_viewport: Some(effective_viewport),
          effective_fov_deg: Some(effective_fov),
          limitations,
          rejected: None,
        }
      }
      Some(IngestDepthMethod::Unknown) | None => MemoryIngestOutput {
        landmark_id: None,
        created: false,
        status: None,
        confidence: None,
        block_pos: None,
        depth_method: input.depth_method,
        effective_viewport: None,
        effective_fov_deg: None,
        limitations: vec![],
        rejected: Some("no_depth_anchor".to_string()),
      },
    },
  }
}

// -----------------------------------------------------------------------------
// Tool 5: memory_record_miss (Slice 2)
// -----------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryRecordMissInput {
  pub landmark_id: String,
  #[serde(default)]
  pub observation_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MemoryRecordMissOutput {
  pub landmark_id: String,
  pub found: bool,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub consecutive_misses: Option<u32>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub confidence: Option<f64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub reason: Option<String>,
}

pub fn memory_record_miss(store: &mut SpatialMemoryStore, input: &MemoryRecordMissInput) -> MemoryRecordMissOutput {
  match store.record_miss(&input.landmark_id) {
    Ok(()) => {
      let lm = store.get(&input.landmark_id);
      MemoryRecordMissOutput {
        landmark_id: input.landmark_id.clone(),
        found: true,
        consecutive_misses: lm.as_ref().map(|l| l.consecutive_misses),
        confidence: lm.as_ref().map(|l| l.confidence),
        reason: None,
      }
    }
    Err(_) => MemoryRecordMissOutput {
      landmark_id: input.landmark_id.clone(),
      found: false,
      consecutive_misses: None,
      confidence: None,
      reason: Some("landmark_not_found".to_string()),
    },
  }
}

// -----------------------------------------------------------------------------
// Tool 6: memory_maintain (Slice 2)
// -----------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryMaintainInput {
  pub now_millis: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryMaintainOutput {
  pub evicted: Vec<PrunedLandmark>,
  pub evicted_count: usize,
  pub retained_count: usize,
}

pub fn memory_maintain(store: &mut SpatialMemoryStore, input: &MemoryMaintainInput) -> MemoryMaintainOutput {
  let evicted = store.prune_stale_with_reasons(input.now_millis);
  let evicted_count = evicted.len();
  let retained_count = store.len();
  MemoryMaintainOutput {
    evicted,
    evicted_count,
    retained_count,
  }
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
  use super::*;
  use crate::spatial_memory_store::ObservationRef;
  use crate::types::{BlockFace, RaycastHit, Vec3};

  fn create_fixture_store() -> SpatialMemoryStore {
    let mut store = SpatialMemoryStore::open_with_config(
      "test_store.json",
      crate::spatial_memory_store::SpatialMemoryConfig {
        dedup_radius_m: 0.6,
        stale_threshold_millis: 300_000,
        min_confidence: 0.3,
        max_consecutive_misses: 5,
      },
    )
    .expect("open store");

    // 1. Telemetry raycast landmark: chest at (0, 95, 10), description=None
    let hit_chest = RaycastHit {
      block_pos: BlockPosition::new(0, 95, 10),
      face: BlockFace::South,
      block_id: "minecraft:chest".to_string(),
    };
    let obs_1 = ObservationRef {
      observation_id: "obs-tele-1".to_string(),
      captured_at_millis: 100_000,
    };
    store.upsert_from_raycast(&hit_chest, &obs_1);

    // 2. Telemetry raycast landmark: oak door at (5, 95, 10), description=None
    let hit_door = RaycastHit {
      block_pos: BlockPosition::new(5, 95, 10),
      face: BlockFace::South,
      block_id: "minecraft:oak_door".to_string(),
    };
    let obs_2 = ObservationRef {
      observation_id: "obs-tele-2".to_string(),
      captured_at_millis: 100_000,
    };
    store.upsert_from_raycast(&hit_door, &obs_2);

    // 3. Visual perception landmark: crafting table at (-5, 95, 10), description="crafting_table (0.90)" (production format)
    let obs_3 = ObservationRef {
      observation_id: "obs-vis-3".to_string(),
      captured_at_millis: 150_000,
    };
    store.upsert_from_perception(BlockPosition::new(-5, 95, 10), "crafting_table (0.90)", 0.90, &obs_3);

    // 4. Dual-source landmark: furnace with both raycast block_id and visual perception description
    let hit_furnace = RaycastHit {
      block_pos: BlockPosition::new(10, 95, 10),
      face: BlockFace::South,
      block_id: "minecraft:furnace".to_string(),
    };
    let obs_4a = ObservationRef {
      observation_id: "obs-tele-4".to_string(),
      captured_at_millis: 100_000,
    };
    store.upsert_from_raycast(&hit_furnace, &obs_4a);
    let obs_4b = ObservationRef {
      observation_id: "obs-vis-4".to_string(),
      captured_at_millis: 150_000,
    };
    store.upsert_from_perception(BlockPosition::new(10, 95, 10), "furnace (0.88)", 0.88, &obs_4b);

    store
  }

  #[test]
  fn test_memory_search_joint_retrieval() {
    let store = create_fixture_store();

    // Search for chest: should find raycast landmark via block_id even though description is None!
    let out_chest = memory_search(
      &store,
      &MemorySearchInput {
        label: ClosedSetLabel::Chest,
        near: None,
        radius_m: None,
        min_confidence: None,
        status_filter: None,
        now_millis: Some(120_000),
      },
    );
    assert_eq!(out_chest.items.len(), 1, "Must find chest landmark via joint search");
    assert_eq!(out_chest.items[0].matched_by, MatchSource::BlockId);
    assert_eq!(out_chest.items[0].block_pos, BlockPosition::new(0, 95, 10));
    assert_eq!(out_chest.items[0].status, SpatialClaimStatus::Confirmed);
    assert_eq!(out_chest.items[0].freshness.as_ref().unwrap().staleness, StalenessStatus::Fresh);

    // Search for door: matches 'minecraft:oak_door'
    let out_door = memory_search(
      &store,
      &MemorySearchInput {
        label: ClosedSetLabel::Door,
        near: None,
        radius_m: None,
        min_confidence: None,
        status_filter: None,
        now_millis: None,
      },
    );
    assert_eq!(out_door.items.len(), 1);
    assert_eq!(out_door.items[0].matched_by, MatchSource::BlockId);

    // Search for crafting_table: matches visual perception description
    let out_ct = memory_search(
      &store,
      &MemorySearchInput {
        label: ClosedSetLabel::CraftingTable,
        near: None,
        radius_m: None,
        min_confidence: None,
        status_filter: None,
        now_millis: None,
      },
    );
    assert_eq!(out_ct.items.len(), 1);
    assert_eq!(out_ct.items[0].matched_by, MatchSource::Description);
    assert_eq!(out_ct.items[0].status, SpatialClaimStatus::Candidate);

    // Search for furnace: matches BOTH block_id ("minecraft:furnace") AND description ("furnace (0.88)")
    let out_furnace = memory_search(
      &store,
      &MemorySearchInput {
        label: ClosedSetLabel::Furnace,
        near: None,
        radius_m: None,
        min_confidence: None,
        status_filter: None,
        now_millis: None,
      },
    );
    assert_eq!(out_furnace.items.len(), 1);
    assert_eq!(out_furnace.items[0].matched_by, MatchSource::Both);
    assert_eq!(out_furnace.items[0].label, "furnace");
    assert_eq!(out_furnace.items[0].status, SpatialClaimStatus::Confirmed);

    // Search with radius filter: only within 4m of (0,95,10)
    let out_radius = memory_search(
      &store,
      &MemorySearchInput {
        label: ClosedSetLabel::Chest,
        near: Some(BlockPosition::new(0, 95, 10)),
        radius_m: Some(4.0),
        min_confidence: None,
        status_filter: None,
        now_millis: None,
      },
    );
    assert_eq!(out_radius.items.len(), 1);

    let out_radius_far = memory_search(
      &store,
      &MemorySearchInput {
        label: ClosedSetLabel::Chest,
        near: Some(BlockPosition::new(100, 95, 100)),
        radius_m: Some(4.0),
        min_confidence: None,
        status_filter: None,
        now_millis: None,
      },
    );
    assert_eq!(out_radius_far.items.len(), 0, "Far search must return empty");
  }

  #[test]
  fn test_memory_query_stateless_projection() {
    let store = create_fixture_store();
    let chest_id = store.find_matching_landmark(BlockPosition::new(0, 95, 10)).unwrap();

    // Observer at (0, 96.62, 0) looking North (yaw=0, +Z in front)
    let observer = PlayerPose {
      eye_position: Vec3 {
        x: 0.0,
        y: 96.62,
        z: 0.0,
      },
      yaw: 0.0,
      pitch: 0.0,
    };

    let q_in = MemoryQueryInput {
      target: LandmarkTarget::LandmarkId(chest_id.clone()),
      query_kind: QueryKind::ScreenProjection,
      observer_pose: observer,
      viewport: Some(Viewport::new(854, 480)),
      vertical_fov_deg: Some(70.0),
      now_millis: Some(150_000),
    };

    let q_out = memory_query(&store, &q_in, None);
    assert_eq!(q_out.status, AnswerStatus::Answered);
    assert_eq!(q_out.visibility, VisibilityClass::Visible);
    assert!(q_out.screen_xy.is_some());
    let (px, py) = q_out.screen_xy.unwrap();
    // Center of 854 is 427, target at x=0.5 should be roughly centered
    assert!((px - 427.0).abs() < 50.0);
    assert!((py - 240.0).abs() < 50.0);
    assert!(q_out.limitations.iter().any(|l| l.contains("occlusion")));
    assert_eq!(q_out.freshness.unwrap().staleness, StalenessStatus::Fresh);

    // Stale check: query with now = 500_000ms (age = 400s > 300s)
    let q_stale_in = MemoryQueryInput {
      target: LandmarkTarget::LandmarkId(chest_id),
      query_kind: QueryKind::Visibility,
      observer_pose: PlayerPose {
        eye_position: Vec3 {
          x: 0.0,
          y: 96.62,
          z: 0.0,
        },
        yaw: 0.0,
        pitch: 0.0,
      },
      viewport: None,
      vertical_fov_deg: None,
      now_millis: Some(500_000),
    };
    let q_stale_out = memory_query(&store, &q_stale_in, None);
    assert_eq!(q_stale_out.freshness.unwrap().staleness, StalenessStatus::Stale);

    // Unknown target test: zero fabrication
    let q_unknown = MemoryQueryInput {
      target: LandmarkTarget::LandmarkId("non-existent-id".to_string()),
      query_kind: QueryKind::Visibility,
      observer_pose: PlayerPose {
        eye_position: Vec3 {
          x: 0.0,
          y: 96.62,
          z: 0.0,
        },
        yaw: 0.0,
        pitch: 0.0,
      },
      viewport: None,
      vertical_fov_deg: None,
      now_millis: None,
    };
    let q_un_out = memory_query(&store, &q_unknown, None);
    assert_eq!(q_un_out.status, AnswerStatus::Unknown);
    assert_eq!(q_un_out.visibility, VisibilityClass::Unknown);
    assert!(q_un_out.screen_xy.is_none());
  }

  #[test]
  fn test_memory_get_inspection() {
    let store = create_fixture_store();
    let chest_id = store.find_matching_landmark(BlockPosition::new(0, 95, 10)).unwrap();

    let get_found = memory_get(
      &store,
      &MemoryGetInput {
        landmark_id: chest_id.clone(),
      },
    );
    match get_found {
      MemoryGetOutput::Found { landmark } => {
        assert_eq!(landmark.landmark_id, chest_id);
        assert_eq!(landmark.position, BlockPosition::new(0, 95, 10));
      }
      _ => panic!("Expected Found"),
    }

    let get_unknown = memory_get(
      &store,
      &MemoryGetInput {
        landmark_id: "lm-missing".to_string(),
      },
    );
    match get_unknown {
      MemoryGetOutput::Unknown {
        landmark_id,
        reason,
      } => {
        assert_eq!(landmark_id, "lm-missing");
        assert!(reason.contains("not found"));
      }
      _ => panic!("Expected Unknown"),
    }
  }

  #[test]
  fn test_memory_ingest_visual_and_depth_clamping() {
    let mut store = create_fixture_store();
    let initial_len = store.len();

    let observer = PlayerPose {
      eye_position: Vec3 {
        x: 0.0,
        y: 96.62,
        z: 0.0,
      },
      yaw: 0.0,
      pitch: 0.0,
    };

    // 1. Visual + DepthModel with high detection confidence (0.95)
    // Clamping to 0.60 ensures confidence <= 0.30
    let ingest_in = MemoryIngestInput {
      source: IngestSource::VisualPerception,
      observer_pose: observer.clone(),
      block_pos: None,
      block_face: None,
      block_id: None,
      screen_bbox: Some([400.0, 200.0, 454.0, 280.0]),
      label: Some(ClosedSetLabel::Torch),
      detection_confidence: Some(0.95),
      depth_m: Some(5.0),
      depth_method: Some(IngestDepthMethod::DepthModel),
      viewport: Some(Viewport::new(854, 480)),
      vertical_fov_deg: Some(70.0),
      observation_id: "obs-vis-ingest-1".to_string(),
      captured_at_millis: 200_000,
    };

    let out = memory_ingest(&mut store, &ingest_in);
    assert!(out.rejected.is_none());
    assert!(out.created, "First ingestion should create new landmark");
    assert_eq!(out.status, Some(SpatialClaimStatus::Candidate));
    assert!(
      out.confidence.unwrap() <= 0.30,
      "Monocular depth ingest confidence must be clamped to <= 0.30, got {}",
      out.confidence.unwrap()
    );
    assert_eq!(out.confidence.unwrap(), 0.30); // 0.60 * 0.5 = 0.30
    assert!(out.limitations.iter().any(|l| l.contains("uncalibrated_monocular_depth_scale")));
    assert!(out.limitations.iter().any(|l| l.contains("back_projection_error_bound_m: 2.0")));

    // Condition 6: verify internal back-projection against manual calculation
    let cx = (400.0 + 454.0) / 2.0;
    let cy = (200.0 + 280.0) / 2.0;
    let manual_world = crate::visual_perception::back_project((cx, cy), 5.0, Viewport::new(854, 480), &observer, 70.0);
    let expected_pos = BlockPosition::new(manual_world.x.round() as i32, manual_world.y.round() as i32, manual_world.z.round() as i32);
    assert_eq!(out.block_pos, Some(expected_pos));

    // Condition 5: Ingesting same block_pos again triggers merge (created == false)
    let out_merge = memory_ingest(&mut store, &ingest_in);
    assert!(!out_merge.created, "Second ingestion at same position must merge (created == false)");
    assert_eq!(out_merge.landmark_id, out.landmark_id);
    assert_eq!(store.len(), initial_len + 1);
  }

  #[test]
  fn test_memory_ingest_rejections_and_raycast() {
    let mut store = create_fixture_store();
    let initial_len = store.len();

    let observer = PlayerPose {
      eye_position: Vec3 {
        x: 0.0,
        y: 96.62,
        z: 0.0,
      },
      yaw: 0.0,
      pitch: 0.0,
    };

    // Condition 2: Visual + unknown depth method -> rejected, no block_pos
    let ingest_unknown_depth = MemoryIngestInput {
      source: IngestSource::VisualPerception,
      observer_pose: observer.clone(),
      block_pos: None,
      block_face: None,
      block_id: None,
      screen_bbox: Some([400.0, 200.0, 454.0, 280.0]),
      label: Some(ClosedSetLabel::Chest),
      detection_confidence: Some(0.80),
      depth_m: Some(5.0),
      depth_method: Some(IngestDepthMethod::Unknown),
      viewport: None,
      vertical_fov_deg: None,
      observation_id: "obs-unknown-depth".to_string(),
      captured_at_millis: 200_000,
    };
    let out_unknown = memory_ingest(&mut store, &ingest_unknown_depth);
    assert_eq!(out_unknown.rejected, Some("no_depth_anchor".to_string()));
    assert!(!out_unknown.created);
    assert!(out_unknown.block_pos.is_none(), "Rejected output must never fabricate coordinates");
    assert_eq!(store.len(), initial_len);

    // Condition 4: VLM hypothesis and multi_view_triangulation -> rejected as unsupported_source
    let ingest_vlm = MemoryIngestInput {
      source: IngestSource::VlmHypothesis,
      observer_pose: observer.clone(),
      block_pos: None,
      block_face: None,
      block_id: None,
      screen_bbox: None,
      label: None,
      detection_confidence: None,
      depth_m: None,
      depth_method: None,
      viewport: None,
      vertical_fov_deg: None,
      observation_id: "obs-vlm".to_string(),
      captured_at_millis: 200_000,
    };
    let out_vlm = memory_ingest(&mut store, &ingest_vlm);
    assert_eq!(out_vlm.rejected, Some("unsupported_source".to_string()));
    assert!(out_vlm.block_pos.is_none());

    let ingest_mvt = MemoryIngestInput {
      source: IngestSource::MultiViewTriangulation,
      observer_pose: observer.clone(),
      block_pos: None,
      block_face: None,
      block_id: None,
      screen_bbox: None,
      label: None,
      detection_confidence: None,
      depth_m: None,
      depth_method: None,
      viewport: None,
      vertical_fov_deg: None,
      observation_id: "obs-mvt".to_string(),
      captured_at_millis: 200_000,
    };
    let out_mvt = memory_ingest(&mut store, &ingest_mvt);
    assert_eq!(out_mvt.rejected, Some("unsupported_source".to_string()));
    assert!(out_mvt.block_pos.is_none());
    assert_eq!(store.len(), initial_len);

    // Condition 3: Telemetry raycast -> Confirmed with confidence >= 0.90
    let ingest_raycast = MemoryIngestInput {
      source: IngestSource::TelemetryRaycast,
      observer_pose: observer,
      block_pos: Some(BlockPosition::new(20, 95, 20)),
      block_face: Some(BlockFace::North),
      block_id: Some("minecraft:crafting_table".to_string()),
      screen_bbox: None,
      label: None,
      detection_confidence: None,
      depth_m: None,
      depth_method: None,
      viewport: None,
      vertical_fov_deg: None,
      observation_id: "obs-tele-new".to_string(),
      captured_at_millis: 200_000,
    };
    let out_raycast = memory_ingest(&mut store, &ingest_raycast);
    assert!(out_raycast.rejected.is_none());
    assert!(out_raycast.created);
    assert_eq!(out_raycast.status, Some(SpatialClaimStatus::Confirmed));
    assert!(out_raycast.confidence.unwrap() >= 0.90);
    assert_eq!(out_raycast.block_pos, Some(BlockPosition::new(20, 95, 20)));
    assert_eq!(store.len(), initial_len + 1);
  }

  #[test]
  fn test_memory_record_miss_lifecycle() {
    let mut store = create_fixture_store();
    let chest_id = store.find_matching_landmark(BlockPosition::new(0, 95, 10)).unwrap();

    // Condition 7: Unknown ID -> found: false, explicit reason
    let out_missing = memory_record_miss(
      &mut store,
      &MemoryRecordMissInput {
        landmark_id: "lm-non-existent".to_string(),
        observation_id: None,
      },
    );
    assert!(!out_missing.found);
    assert_eq!(out_missing.reason, Some("landmark_not_found".to_string()));

    // Condition 8: Known ID -> consecutive_misses + 1, confidence - 0.1
    let out_miss1 = memory_record_miss(
      &mut store,
      &MemoryRecordMissInput {
        landmark_id: chest_id.clone(),
        observation_id: Some("obs-miss-1".to_string()),
      },
    );
    assert!(out_miss1.found);
    assert_eq!(out_miss1.consecutive_misses, Some(1));
    assert!((out_miss1.confidence.unwrap() - 0.80).abs() < 1e-4);

    let out_miss2 = memory_record_miss(
      &mut store,
      &MemoryRecordMissInput {
        landmark_id: chest_id.clone(),
        observation_id: Some("obs-miss-2".to_string()),
      },
    );
    assert!(out_miss2.found);
    assert_eq!(out_miss2.consecutive_misses, Some(2));
    assert!((out_miss2.confidence.unwrap() - 0.70).abs() < 1e-4);

    // Calling record_miss does not delete the landmark
    assert!(store.get(&chest_id).is_some());
  }

  #[test]
  fn test_memory_maintain_attribution_and_step11_regression() {
    let mut store = create_fixture_store();
    let chest_id = store.find_matching_landmark(BlockPosition::new(0, 95, 10)).unwrap();

    // Condition 9 (Step 11 Regression):
    // Deliver 5 misses on chest landmark with fresh timestamp (now_millis = 105_000, last_observed = 100_000)
    for i in 1..=5 {
      let _ = memory_record_miss(
        &mut store,
        &MemoryRecordMissInput {
          landmark_id: chest_id.clone(),
          observation_id: Some(format!("obs-miss-{}", i)),
        },
      );
    }
    assert_eq!(store.get(&chest_id).unwrap().consecutive_misses, 5);

    // Maintain at fresh timestamp (diff = 5s << 300s stale threshold)
    let maintain_out = memory_maintain(
      &mut store,
      &MemoryMaintainInput {
        now_millis: 105_000,
      },
    );
    let chest_eviction = maintain_out.evicted.iter().find(|p| p.landmark_id == chest_id);
    assert!(chest_eviction.is_some(), "Chest with 5 misses must be evicted");
    assert_eq!(
      chest_eviction.unwrap().reason,
      crate::spatial_memory_store::PruneReason::TooManyMisses,
      "Reason must be TooManyMisses, NOT StaleTimeout!"
    );

    // Condition 10: Static landmark exceeding stale threshold -> StaleTimeout
    let door_id = store.find_matching_landmark(BlockPosition::new(5, 95, 10)).unwrap();
    // Door observed at 100_000, maintain at 500_000 (diff 400s > 300s)
    let maintain_stale = memory_maintain(
      &mut store,
      &MemoryMaintainInput {
        now_millis: 500_000,
      },
    );
    let door_eviction = maintain_stale.evicted.iter().find(|p| p.landmark_id == door_id);
    assert!(door_eviction.is_some());
    assert_eq!(door_eviction.unwrap().reason, crate::spatial_memory_store::PruneReason::StaleTimeout);

    // Condition 11: Dynamic landmark with expired TTL -> ExpiredTtl
    let dynamic_obs = ObservationRef {
      observation_id: "obs-dyn-1".to_string(),
      captured_at_millis: 600_000,
    };
    let dyn_id = store.upsert_dynamic_landmark(
      42,
      10_000, // 10s TTL
      BlockPosition::new(30, 95, 30),
      Some((30.5, 95.0, 30.5)),
      "zombie",
      0.80,
      &dynamic_obs,
    );
    // At 605_000 (5s), dynamic landmark should still be kept
    let maintain_fresh_dyn = memory_maintain(
      &mut store,
      &MemoryMaintainInput {
        now_millis: 605_000,
      },
    );
    assert!(maintain_fresh_dyn.evicted.iter().all(|p| p.landmark_id != dyn_id));

    // At 615_000 (15s > 10s TTL), dynamic landmark evicted as ExpiredTtl
    let maintain_dyn_expire = memory_maintain(
      &mut store,
      &MemoryMaintainInput {
        now_millis: 615_000,
      },
    );
    let dyn_eviction = maintain_dyn_expire.evicted.iter().find(|p| p.landmark_id == dyn_id);
    assert_eq!(dyn_eviction.unwrap().reason, crate::spatial_memory_store::PruneReason::ExpiredTtl);

    // Condition 12: LowConfidence on landmark with misses < max_misses
    let low_conf_obs = ObservationRef {
      observation_id: "obs-low-conf".to_string(),
      captured_at_millis: 700_000,
    };
    // Perception with conf 0.40 -> store confidence = 0.20 < min_conf (0.30)
    let low_id = store.upsert_from_perception(BlockPosition::new(40, 95, 40), "chest (0.40)", 0.40, &low_conf_obs);
    let maintain_low = memory_maintain(
      &mut store,
      &MemoryMaintainInput {
        now_millis: 701_000,
      },
    );
    let low_eviction = maintain_low.evicted.iter().find(|p| p.landmark_id == low_id);
    assert!(low_eviction.is_some());
    assert_eq!(low_eviction.unwrap().reason, crate::spatial_memory_store::PruneReason::LowConfidence);

    // Condition 13: No evictable items -> evicted is empty, retained_count matches store.len()
    let maintain_empty = memory_maintain(
      &mut store,
      &MemoryMaintainInput {
        now_millis: 701_000,
      },
    );
    assert!(maintain_empty.evicted.is_empty());
    assert_eq!(maintain_empty.evicted_count, 0);
    assert_eq!(maintain_empty.retained_count, store.len());
  }
}
