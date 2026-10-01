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
use crate::spatial_memory_store::{SpatialLandmark, SpatialMemoryStore};
use crate::types::{BlockPosition, PlayerPose, Viewport};

pub const DEFAULT_STALE_THRESHOLD_MILLIS: u64 = 300_000;

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

    // 3. Joint Search: description == label OR stripped block_id contains label
    let match_desc = lm.description.as_deref().map(|d| d.eq_ignore_ascii_case(target_label)).unwrap_or(false);

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

    // 3. Visual perception landmark: crafting table at (-5, 95, 10), description="crafting_table"
    let obs_3 = ObservationRef {
      observation_id: "obs-vis-3".to_string(),
      captured_at_millis: 150_000,
    };
    store.upsert_from_perception(BlockPosition::new(-5, 95, 10), "crafting_table", 0.90, &obs_3);

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
}
