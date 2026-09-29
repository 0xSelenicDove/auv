//! Production Integration Differential Test & Benchmark for Spatial Hash in SpatialMemoryStore.
//!
//! Verifies:
//! 1. Mathematical and behavioral parity (100.0%) between:
//!    - Existing production SpatialMemoryStore
//!    - PureLinearScanStore (O(N) with deterministic insertion-order early-break)
//!    - SpatialHashSlotStore (Candidate A: HashMap<CellKey, Vec<usize>> + Slot table)
//!    - SpatialHashStringStore (Candidate B: HashMap<CellKey, Vec<String>> + order map)
//! 2. Zero ghost indexes across all lifecycle mutations:
//!    - Insertions
//!    - Deduplication merges
//!    - Visual perception authority upgrades
//!    - Dynamic landmark position changes (cell transitions)
//!    - Pruning of stale/low-confidence/TTL landmarks
//!    - Post-pruning re-insertions
//! 3. Performance benchmarks at 1k, 10k, and 100k scale (p50, p95, max latency, memory).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use auv_game_minecraft::types::{BlockFace, BlockPosition, RaycastHit};
use auv_game_minecraft::{
  LandmarkKind, LandmarkObservation, LandmarkSource, ObservationRef, SpatialClaimStatus, SpatialLandmark, SpatialMemoryConfig,
  SpatialMemoryStore, SpatialMemoryStoreError,
};

// ============================================================================
// 1. REPRODUCIBLE PRNG: CPython MT19937 compatible
// ============================================================================

#[derive(Clone)]
pub struct Mt19937Rng {
  mt: [u32; 624],
  mti: usize,
}

impl Mt19937Rng {
  pub fn new_with_seed(seed: u32) -> Self {
    let mut mt = [0u32; 624];
    mt[0] = 19650218u32;
    for i in 1..624 {
      mt[i] = (1812433253u32.wrapping_mul(mt[i - 1] ^ (mt[i - 1] >> 30))).wrapping_add(i as u32);
    }
    let init_key = [seed];
    let mut i = 1usize;
    let mut j = 0usize;
    let k = 624usize;
    for _ in 0..k {
      mt[i] = ((mt[i] ^ ((mt[i - 1] ^ (mt[i - 1] >> 30)).wrapping_mul(1664525u32))).wrapping_add(init_key[j])).wrapping_add(j as u32);
      i += 1;
      j += 1;
      if i >= 624 {
        mt[0] = mt[623];
        i = 1;
      }
      if j >= init_key.len() {
        j = 0;
      }
    }
    for _ in 0..623 {
      mt[i] = (mt[i] ^ ((mt[i - 1] ^ (mt[i - 1] >> 30)).wrapping_mul(1566083941u32))).wrapping_sub(i as u32);
      i += 1;
      if i >= 624 {
        mt[0] = mt[623];
        i = 1;
      }
    }
    mt[0] = 0x80000000;
    Self { mt, mti: 624 }
  }

  pub fn next_u32(&mut self) -> u32 {
    const MAG01: [u32; 2] = [0x0, 0x9908b0df];
    if self.mti >= 624 {
      for kk in 0..(624 - 397) {
        let y = (self.mt[kk] & 0x80000000) | (self.mt[kk + 1] & 0x7fffffff);
        self.mt[kk] = self.mt[kk + 397] ^ (y >> 1) ^ MAG01[(y & 1) as usize];
      }
      for kk in (624 - 397)..623 {
        let y = (self.mt[kk] & 0x80000000) | (self.mt[kk + 1] & 0x7fffffff);
        self.mt[kk] = self.mt[kk + 397 - 624] ^ (y >> 1) ^ MAG01[(y & 1) as usize];
      }
      let y = (self.mt[623] & 0x80000000) | (self.mt[0] & 0x7fffffff);
      self.mt[623] = self.mt[396] ^ (y >> 1) ^ MAG01[(y & 1) as usize];
      self.mti = 0;
    }
    let mut y = self.mt[self.mti];
    self.mti += 1;
    y ^= y >> 11;
    y ^= (y << 7) & 0x9d2c5680;
    y ^= (y << 15) & 0xefc60000;
    y ^= y >> 18;
    y
  }

  pub fn random_f64(&mut self) -> f64 {
    let a = self.next_u32() >> 5;
    let b = self.next_u32() >> 6;
    ((a as f64) * 67108864.0 + (b as f64)) * (1.0 / 9007199254740992.0)
  }

  pub fn uniform(&mut self, min: f64, max: f64) -> f64 {
    min + (max - min) * self.random_f64()
  }

  pub fn random_block_position(&mut self, min_coord: i32, max_coord: i32) -> BlockPosition {
    BlockPosition::new(
      self.uniform(min_coord as f64, max_coord as f64).round() as i32,
      self.uniform(0.0, 256.0).round() as i32,
      self.uniform(min_coord as f64, max_coord as f64).round() as i32,
    )
  }
}

// ============================================================================
// 2. GRID COORDINATE HELPERS
// ============================================================================

pub type CellKey = (i64, i64, i64);

#[inline]
pub fn cell_coords(pos: BlockPosition, cell_size: f64) -> CellKey {
  (
    (f64::from(pos.x) / cell_size).floor() as i64,
    (f64::from(pos.y) / cell_size).floor() as i64,
    (f64::from(pos.z) / cell_size).floor() as i64,
  )
}

// ============================================================================
// 3. REFERENCE: PURE LINEAR SCAN STORE (O(N) with deterministic insertion order)
// ============================================================================

#[derive(Clone, Debug, PartialEq)]
pub struct PureLinearScanStore {
  pub landmarks: HashMap<String, SpatialLandmark>,
  pub insertion_order: Vec<String>,
  pub path: PathBuf,
  pub config: SpatialMemoryConfig,
}

impl PureLinearScanStore {
  pub fn new(path: impl AsRef<Path>, config: SpatialMemoryConfig) -> Self {
    Self {
      landmarks: HashMap::new(),
      insertion_order: Vec::new(),
      path: path.as_ref().to_path_buf(),
      config,
    }
  }

  fn find_matching_landmark(&self, query_pos: BlockPosition) -> Option<String> {
    for id in &self.insertion_order {
      if let Some(lm) = self.landmarks.get(id) {
        let diff_x = f64::from(lm.position.x - query_pos.x);
        let diff_y = f64::from(lm.position.y - query_pos.y);
        let diff_z = f64::from(lm.position.z - query_pos.z);
        let dist = (diff_x * diff_x + diff_y * diff_y + diff_z * diff_z).sqrt();
        if dist < self.config.dedup_radius_m {
          return Some(id.clone());
        }
      }
    }
    None
  }

  pub fn upsert_from_raycast(&mut self, hit: &RaycastHit, obs: &ObservationRef) -> String {
    let matching_id = self.find_matching_landmark(hit.block_pos);

    if let Some(id) = matching_id {
      let _ = self.record_observation(&id, obs.captured_at_millis);
      let landmark = self.landmarks.get_mut(&id).expect("landmark exists");
      landmark.observations.push(LandmarkObservation {
        observation_ref: obs.clone(),
        source: LandmarkSource::TelemetryRaycast,
        hit_face: Some(hit.face),
        block_id: Some(hit.block_id.clone()),
      });
      landmark.surface_face = Some(hit.face);
      id
    } else {
      let kind = LandmarkKind::Static;
      let landmark_id = format!("lm-{}-{}-{}-{}", hit.block_pos.x, hit.block_pos.y, hit.block_pos.z, kind.as_str());
      let landmark = SpatialLandmark {
        landmark_id: landmark_id.clone(),
        kind,
        position: hit.block_pos,
        continuous_position: None,
        first_observed: obs.clone(),
        observations: vec![LandmarkObservation {
          observation_ref: obs.clone(),
          source: LandmarkSource::TelemetryRaycast,
          hit_face: Some(hit.face),
          block_id: Some(hit.block_id.clone()),
        }],
        status: SpatialClaimStatus::Confirmed,
        source: LandmarkSource::TelemetryRaycast,
        description: None,
        surface_face: Some(hit.face),
        observation_count: 1,
        last_observed_millis: obs.captured_at_millis,
        consecutive_misses: 0,
        confidence: 0.90,
      };
      self.landmarks.insert(landmark_id.clone(), landmark);
      self.insertion_order.push(landmark_id.clone());
      landmark_id
    }
  }

  pub fn upsert_from_perception(
    &mut self,
    block_pos: BlockPosition,
    label: &str,
    detection_confidence: f64,
    obs: &ObservationRef,
  ) -> String {
    let matching_id = self.find_matching_landmark(block_pos);

    if let Some(id) = matching_id {
      let landmark = self.landmarks.get_mut(&id).expect("landmark exists");
      landmark.observations.push(LandmarkObservation {
        observation_ref: obs.clone(),
        source: LandmarkSource::VisualPerception,
        hit_face: None,
        block_id: None,
      });
      landmark.observation_count += 1;
      landmark.consecutive_misses = 0;
      landmark.last_observed_millis = obs.captured_at_millis;

      if landmark.status == SpatialClaimStatus::Confirmed {
        let extra = (landmark.observation_count.saturating_sub(1) as f64) * 0.02;
        landmark.confidence = (0.90 + extra).min(0.99);
      } else {
        let candidate_conf = (detection_confidence * 0.5).clamp(0.1, 0.5);
        landmark.confidence = landmark.confidence.max(candidate_conf);
      }

      landmark.description = Some(label.to_string());
      id
    } else {
      let kind = LandmarkKind::Static;
      let landmark_id = format!("lm-{}-{}-{}-{}", block_pos.x, block_pos.y, block_pos.z, kind.as_str());
      let confidence = (detection_confidence * 0.5).clamp(0.1, 0.5);
      let landmark = SpatialLandmark {
        landmark_id: landmark_id.clone(),
        kind,
        position: block_pos,
        continuous_position: None,
        first_observed: obs.clone(),
        observations: vec![LandmarkObservation {
          observation_ref: obs.clone(),
          source: LandmarkSource::VisualPerception,
          hit_face: None,
          block_id: None,
        }],
        status: SpatialClaimStatus::Candidate,
        source: LandmarkSource::VisualPerception,
        description: Some(label.to_string()),
        surface_face: None,
        observation_count: 1,
        last_observed_millis: obs.captured_at_millis,
        consecutive_misses: 0,
        confidence,
      };
      self.landmarks.insert(landmark_id.clone(), landmark);
      self.insertion_order.push(landmark_id.clone());
      landmark_id
    }
  }

  pub fn upsert_dynamic_landmark(
    &mut self,
    track_id: u64,
    ttl_millis: u64,
    block_pos: BlockPosition,
    continuous_pos: Option<(f64, f64, f64)>,
    label: &str,
    detection_confidence: f64,
    obs: &ObservationRef,
  ) -> String {
    let matching_id = self.landmarks.iter().find_map(|(id, lm)| {
      if let LandmarkKind::Dynamic { track_id: tid, .. } = lm.kind {
        if tid == track_id {
          return Some(id.clone());
        }
      }
      None
    });

    if let Some(id) = matching_id {
      let landmark = self.landmarks.get_mut(&id).expect("landmark exists");
      landmark.position = block_pos;
      landmark.continuous_position = continuous_pos;
      landmark.observations.push(LandmarkObservation {
        observation_ref: obs.clone(),
        source: LandmarkSource::VisualPerception,
        hit_face: None,
        block_id: None,
      });
      landmark.observation_count += 1;
      landmark.consecutive_misses = 0;
      landmark.last_observed_millis = obs.captured_at_millis;

      if landmark.status == SpatialClaimStatus::Confirmed {
        let extra = (landmark.observation_count.saturating_sub(1) as f64) * 0.02;
        landmark.confidence = (0.90 + extra).min(0.99);
      } else {
        let candidate_conf = (detection_confidence * 0.5).clamp(0.1, 0.5);
        landmark.confidence = landmark.confidence.max(candidate_conf);
      }

      landmark.description = Some(label.to_string());
      landmark.kind = LandmarkKind::Dynamic {
        track_id,
        ttl_millis,
      };
      id
    } else {
      let kind = LandmarkKind::Dynamic {
        track_id,
        ttl_millis,
      };
      let landmark_id = format!("lm-track-{}-{}", track_id, kind.as_str());
      let confidence = (detection_confidence * 0.5).clamp(0.1, 0.5);
      let landmark = SpatialLandmark {
        landmark_id: landmark_id.clone(),
        kind,
        position: block_pos,
        continuous_position: continuous_pos,
        first_observed: obs.clone(),
        observations: vec![LandmarkObservation {
          observation_ref: obs.clone(),
          source: LandmarkSource::VisualPerception,
          hit_face: None,
          block_id: None,
        }],
        status: SpatialClaimStatus::Candidate,
        source: LandmarkSource::VisualPerception,
        description: Some(label.to_string()),
        surface_face: None,
        observation_count: 1,
        last_observed_millis: obs.captured_at_millis,
        consecutive_misses: 0,
        confidence,
      };
      self.landmarks.insert(landmark_id.clone(), landmark);
      self.insertion_order.push(landmark_id.clone());
      landmark_id
    }
  }

  pub fn record_observation(&mut self, landmark_id: &str, now_millis: u64) -> Result<(), SpatialMemoryStoreError> {
    let landmark = self.landmarks.get_mut(landmark_id).ok_or_else(|| SpatialMemoryStoreError::LandmarkNotFound(landmark_id.to_string()))?;

    if landmark.observation_count == 0 && !landmark.observations.is_empty() {
      landmark.observation_count = landmark.observations.len() as u32;
    }
    landmark.observation_count += 1;
    landmark.consecutive_misses = 0;
    landmark.last_observed_millis = now_millis;
    let extra = (landmark.observation_count.saturating_sub(1) as f64) * 0.02;
    landmark.confidence = (0.90 + extra).min(0.99);
    Ok(())
  }

  pub fn record_miss(&mut self, landmark_id: &str) -> Result<(), SpatialMemoryStoreError> {
    let landmark = self.landmarks.get_mut(landmark_id).ok_or_else(|| SpatialMemoryStoreError::LandmarkNotFound(landmark_id.to_string()))?;

    landmark.consecutive_misses += 1;
    landmark.confidence = (landmark.confidence - 0.1).max(0.0);
    Ok(())
  }

  pub fn prune_stale(&mut self, now_millis: u64) -> usize {
    let before_len = self.landmarks.len();
    let stale_threshold = self.config.stale_threshold_millis;
    let min_conf = self.config.min_confidence;
    let max_misses = self.config.max_consecutive_misses;

    self.landmarks.retain(|_id, lm| match lm.kind {
      LandmarkKind::Dynamic { ttl_millis, .. } => {
        let is_expired = ttl_millis > 0 && lm.last_observed_millis > 0 && now_millis.saturating_sub(lm.last_observed_millis) > ttl_millis;
        !is_expired
      }
      LandmarkKind::Static => {
        let is_expired =
          stale_threshold > 0 && lm.last_observed_millis > 0 && now_millis.saturating_sub(lm.last_observed_millis) > stale_threshold;
        let is_low_confidence = lm.confidence < min_conf;
        let is_too_many_misses = max_misses > 0 && lm.consecutive_misses >= max_misses;

        !is_expired && !is_low_confidence && !is_too_many_misses
      }
    });

    self.insertion_order.retain(|id| self.landmarks.contains_key(id));
    before_len - self.landmarks.len()
  }

  pub fn len(&self) -> usize {
    self.landmarks.len()
  }
}

// ============================================================================
// 4. CANDIDATE A: SPATIAL HASH SLOT STORE (HashMap<CellKey, Vec<usize>>)
// ============================================================================

#[derive(Clone, Debug, PartialEq)]
pub struct LandmarkSlot {
  pub id: String,
  pub position: BlockPosition,
  pub is_alive: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpatialHashSlotStore {
  pub landmarks: HashMap<String, SpatialLandmark>,
  pub path: PathBuf,
  pub config: SpatialMemoryConfig,
  // Internal Spatial Hash structures:
  pub grid: HashMap<CellKey, Vec<usize>>,
  pub slots: Vec<LandmarkSlot>,
  pub id_to_slot: HashMap<String, usize>,
}

impl SpatialHashSlotStore {
  pub fn new(path: impl AsRef<Path>, config: SpatialMemoryConfig) -> Self {
    Self {
      landmarks: HashMap::new(),
      path: path.as_ref().to_path_buf(),
      config,
      grid: HashMap::new(),
      slots: Vec::new(),
      id_to_slot: HashMap::new(),
    }
  }

  pub fn rebuild_grid(&mut self) {
    self.grid.clear();
    self.slots.clear();
    self.id_to_slot.clear();

    // When rebuilding from existing landmarks, maintain deterministic ordering:
    let mut sorted_landmarks: Vec<_> = self.landmarks.values().collect();
    sorted_landmarks.sort_by(|a, b| {
      a.first_observed.captured_at_millis.cmp(&b.first_observed.captured_at_millis).then_with(|| a.landmark_id.cmp(&b.landmark_id))
    });

    for lm in sorted_landmarks {
      let slot_idx = self.slots.len();
      self.slots.push(LandmarkSlot {
        id: lm.landmark_id.clone(),
        position: lm.position,
        is_alive: true,
      });
      self.id_to_slot.insert(lm.landmark_id.clone(), slot_idx);
      let cell = cell_coords(lm.position, self.config.dedup_radius_m);
      self.grid.entry(cell).or_default().push(slot_idx);
    }
  }

  #[inline]
  fn find_matching_landmark(&self, query_pos: BlockPosition) -> Option<String> {
    let (ix, iy, iz) = cell_coords(query_pos, self.config.dedup_radius_m);
    let mut best_matching_slot: Option<usize> = None;

    // 27 neighbor cells
    for dx in -1..=1 {
      for dy in -1..=1 {
        for dz in -1..=1 {
          let neighbor_cell = (ix + dx, iy + dy, iz + dz);
          if let Some(indices) = self.grid.get(&neighbor_cell) {
            for &slot_idx in indices {
              let slot = &self.slots[slot_idx];
              if !slot.is_alive {
                continue;
              }
              let diff_x = f64::from(slot.position.x - query_pos.x);
              let diff_y = f64::from(slot.position.y - query_pos.y);
              let diff_z = f64::from(slot.position.z - query_pos.z);
              let dist = (diff_x * diff_x + diff_y * diff_y + diff_z * diff_z).sqrt();
              if dist < self.config.dedup_radius_m {
                if best_matching_slot.map_or(true, |curr| slot_idx < curr) {
                  best_matching_slot = Some(slot_idx);
                }
              }
            }
          }
        }
      }
    }

    best_matching_slot.map(|idx| self.slots[idx].id.clone())
  }

  #[inline]
  fn index_new_landmark(&mut self, landmark_id: &str, pos: BlockPosition) {
    let slot_idx = self.slots.len();
    self.slots.push(LandmarkSlot {
      id: landmark_id.to_string(),
      position: pos,
      is_alive: true,
    });
    self.id_to_slot.insert(landmark_id.to_string(), slot_idx);
    let cell = cell_coords(pos, self.config.dedup_radius_m);
    self.grid.entry(cell).or_default().push(slot_idx);
  }

  #[inline]
  fn remove_from_index(&mut self, landmark_id: &str, pos: BlockPosition) {
    if let Some(slot_idx) = self.id_to_slot.remove(landmark_id) {
      self.slots[slot_idx].is_alive = false;
      let cell = cell_coords(pos, self.config.dedup_radius_m);
      if let Some(indices) = self.grid.get_mut(&cell) {
        indices.retain(|&idx| idx != slot_idx);
        if indices.is_empty() {
          self.grid.remove(&cell);
        }
      }
    }
  }

  #[inline]
  fn update_landmark_position(&mut self, landmark_id: &str, old_pos: BlockPosition, new_pos: BlockPosition) {
    if old_pos == new_pos {
      return;
    }
    if let Some(&slot_idx) = self.id_to_slot.get(landmark_id) {
      self.slots[slot_idx].position = new_pos;
      let old_cell = cell_coords(old_pos, self.config.dedup_radius_m);
      let new_cell = cell_coords(new_pos, self.config.dedup_radius_m);
      if old_cell != new_cell {
        if let Some(indices) = self.grid.get_mut(&old_cell) {
          indices.retain(|&idx| idx != slot_idx);
          if indices.is_empty() {
            self.grid.remove(&old_cell);
          }
        }
        self.grid.entry(new_cell).or_default().push(slot_idx);
      }
    }
  }

  pub fn upsert_from_raycast(&mut self, hit: &RaycastHit, obs: &ObservationRef) -> String {
    let matching_id = self.find_matching_landmark(hit.block_pos);

    if let Some(id) = matching_id {
      let _ = self.record_observation(&id, obs.captured_at_millis);
      let landmark = self.landmarks.get_mut(&id).expect("landmark exists");
      landmark.observations.push(LandmarkObservation {
        observation_ref: obs.clone(),
        source: LandmarkSource::TelemetryRaycast,
        hit_face: Some(hit.face),
        block_id: Some(hit.block_id.clone()),
      });
      landmark.surface_face = Some(hit.face);
      id
    } else {
      let kind = LandmarkKind::Static;
      let landmark_id = format!("lm-{}-{}-{}-{}", hit.block_pos.x, hit.block_pos.y, hit.block_pos.z, kind.as_str());
      let landmark = SpatialLandmark {
        landmark_id: landmark_id.clone(),
        kind,
        position: hit.block_pos,
        continuous_position: None,
        first_observed: obs.clone(),
        observations: vec![LandmarkObservation {
          observation_ref: obs.clone(),
          source: LandmarkSource::TelemetryRaycast,
          hit_face: Some(hit.face),
          block_id: Some(hit.block_id.clone()),
        }],
        status: SpatialClaimStatus::Confirmed,
        source: LandmarkSource::TelemetryRaycast,
        description: None,
        surface_face: Some(hit.face),
        observation_count: 1,
        last_observed_millis: obs.captured_at_millis,
        consecutive_misses: 0,
        confidence: 0.90,
      };
      self.landmarks.insert(landmark_id.clone(), landmark);
      self.index_new_landmark(&landmark_id, hit.block_pos);
      landmark_id
    }
  }

  pub fn upsert_from_perception(
    &mut self,
    block_pos: BlockPosition,
    label: &str,
    detection_confidence: f64,
    obs: &ObservationRef,
  ) -> String {
    let matching_id = self.find_matching_landmark(block_pos);

    if let Some(id) = matching_id {
      let landmark = self.landmarks.get_mut(&id).expect("landmark exists");
      landmark.observations.push(LandmarkObservation {
        observation_ref: obs.clone(),
        source: LandmarkSource::VisualPerception,
        hit_face: None,
        block_id: None,
      });
      landmark.observation_count += 1;
      landmark.consecutive_misses = 0;
      landmark.last_observed_millis = obs.captured_at_millis;

      if landmark.status == SpatialClaimStatus::Confirmed {
        let extra = (landmark.observation_count.saturating_sub(1) as f64) * 0.02;
        landmark.confidence = (0.90 + extra).min(0.99);
      } else {
        let candidate_conf = (detection_confidence * 0.5).clamp(0.1, 0.5);
        landmark.confidence = landmark.confidence.max(candidate_conf);
      }

      landmark.description = Some(label.to_string());
      id
    } else {
      let kind = LandmarkKind::Static;
      let landmark_id = format!("lm-{}-{}-{}-{}", block_pos.x, block_pos.y, block_pos.z, kind.as_str());
      let confidence = (detection_confidence * 0.5).clamp(0.1, 0.5);
      let landmark = SpatialLandmark {
        landmark_id: landmark_id.clone(),
        kind,
        position: block_pos,
        continuous_position: None,
        first_observed: obs.clone(),
        observations: vec![LandmarkObservation {
          observation_ref: obs.clone(),
          source: LandmarkSource::VisualPerception,
          hit_face: None,
          block_id: None,
        }],
        status: SpatialClaimStatus::Candidate,
        source: LandmarkSource::VisualPerception,
        description: Some(label.to_string()),
        surface_face: None,
        observation_count: 1,
        last_observed_millis: obs.captured_at_millis,
        consecutive_misses: 0,
        confidence,
      };
      self.landmarks.insert(landmark_id.clone(), landmark);
      self.index_new_landmark(&landmark_id, block_pos);
      landmark_id
    }
  }

  pub fn upsert_dynamic_landmark(
    &mut self,
    track_id: u64,
    ttl_millis: u64,
    block_pos: BlockPosition,
    continuous_pos: Option<(f64, f64, f64)>,
    label: &str,
    detection_confidence: f64,
    obs: &ObservationRef,
  ) -> String {
    let matching_id = self.landmarks.iter().find_map(|(id, lm)| {
      if let LandmarkKind::Dynamic { track_id: tid, .. } = lm.kind {
        if tid == track_id {
          return Some(id.clone());
        }
      }
      None
    });

    if let Some(id) = matching_id {
      let old_pos = self.landmarks.get(&id).unwrap().position;
      self.update_landmark_position(&id, old_pos, block_pos);

      let landmark = self.landmarks.get_mut(&id).expect("landmark exists");
      landmark.position = block_pos;
      landmark.continuous_position = continuous_pos;
      landmark.observations.push(LandmarkObservation {
        observation_ref: obs.clone(),
        source: LandmarkSource::VisualPerception,
        hit_face: None,
        block_id: None,
      });
      landmark.observation_count += 1;
      landmark.consecutive_misses = 0;
      landmark.last_observed_millis = obs.captured_at_millis;

      if landmark.status == SpatialClaimStatus::Confirmed {
        let extra = (landmark.observation_count.saturating_sub(1) as f64) * 0.02;
        landmark.confidence = (0.90 + extra).min(0.99);
      } else {
        let candidate_conf = (detection_confidence * 0.5).clamp(0.1, 0.5);
        landmark.confidence = landmark.confidence.max(candidate_conf);
      }

      landmark.description = Some(label.to_string());
      landmark.kind = LandmarkKind::Dynamic {
        track_id,
        ttl_millis,
      };
      id
    } else {
      let kind = LandmarkKind::Dynamic {
        track_id,
        ttl_millis,
      };
      let landmark_id = format!("lm-track-{}-{}", track_id, kind.as_str());
      let confidence = (detection_confidence * 0.5).clamp(0.1, 0.5);
      let landmark = SpatialLandmark {
        landmark_id: landmark_id.clone(),
        kind,
        position: block_pos,
        continuous_position: continuous_pos,
        first_observed: obs.clone(),
        observations: vec![LandmarkObservation {
          observation_ref: obs.clone(),
          source: LandmarkSource::VisualPerception,
          hit_face: None,
          block_id: None,
        }],
        status: SpatialClaimStatus::Candidate,
        source: LandmarkSource::VisualPerception,
        description: Some(label.to_string()),
        surface_face: None,
        observation_count: 1,
        last_observed_millis: obs.captured_at_millis,
        consecutive_misses: 0,
        confidence,
      };
      self.landmarks.insert(landmark_id.clone(), landmark);
      self.index_new_landmark(&landmark_id, block_pos);
      landmark_id
    }
  }

  pub fn record_observation(&mut self, landmark_id: &str, now_millis: u64) -> Result<(), SpatialMemoryStoreError> {
    let landmark = self.landmarks.get_mut(landmark_id).ok_or_else(|| SpatialMemoryStoreError::LandmarkNotFound(landmark_id.to_string()))?;

    if landmark.observation_count == 0 && !landmark.observations.is_empty() {
      landmark.observation_count = landmark.observations.len() as u32;
    }
    landmark.observation_count += 1;
    landmark.consecutive_misses = 0;
    landmark.last_observed_millis = now_millis;
    let extra = (landmark.observation_count.saturating_sub(1) as f64) * 0.02;
    landmark.confidence = (0.90 + extra).min(0.99);
    Ok(())
  }

  pub fn record_miss(&mut self, landmark_id: &str) -> Result<(), SpatialMemoryStoreError> {
    let landmark = self.landmarks.get_mut(landmark_id).ok_or_else(|| SpatialMemoryStoreError::LandmarkNotFound(landmark_id.to_string()))?;

    landmark.consecutive_misses += 1;
    landmark.confidence = (landmark.confidence - 0.1).max(0.0);
    Ok(())
  }

  pub fn prune_stale(&mut self, now_millis: u64) -> usize {
    let stale_threshold = self.config.stale_threshold_millis;
    let min_conf = self.config.min_confidence;
    let max_misses = self.config.max_consecutive_misses;

    let to_remove: Vec<(String, BlockPosition)> = self
      .landmarks
      .values()
      .filter_map(|lm| {
        let should_prune = match lm.kind {
          LandmarkKind::Dynamic { ttl_millis, .. } => {
            ttl_millis > 0 && lm.last_observed_millis > 0 && now_millis.saturating_sub(lm.last_observed_millis) > ttl_millis
          }
          LandmarkKind::Static => {
            let is_expired =
              stale_threshold > 0 && lm.last_observed_millis > 0 && now_millis.saturating_sub(lm.last_observed_millis) > stale_threshold;
            let is_low_confidence = lm.confidence < min_conf;
            let is_too_many_misses = max_misses > 0 && lm.consecutive_misses >= max_misses;

            is_expired || is_low_confidence || is_too_many_misses
          }
        };
        if should_prune {
          Some((lm.landmark_id.clone(), lm.position))
        } else {
          None
        }
      })
      .collect();

    let pruned_count = to_remove.len();
    for (id, pos) in to_remove {
      self.landmarks.remove(&id);
      self.remove_from_index(&id, pos);
    }

    // Compaction when dead slots exceed 50% threshold on scale > 10k
    let dead_slots = self.slots.len().saturating_sub(self.landmarks.len());
    if self.slots.len() > 10_000 && dead_slots > self.slots.len() / 2 {
      self.rebuild_grid();
    }

    pruned_count
  }

  pub fn len(&self) -> usize {
    self.landmarks.len()
  }

  pub fn estimate_memory_bytes(&self) -> (usize, usize, usize) {
    let grid_hashmap_bytes = self.grid.capacity() * (std::mem::size_of::<CellKey>() + std::mem::size_of::<Vec<usize>>() + 1);
    let vec_heap_bytes: usize = self.grid.values().map(|v| v.capacity() * std::mem::size_of::<usize>()).sum();
    let slots_bytes =
      self.slots.capacity() * std::mem::size_of::<LandmarkSlot>() + self.slots.iter().map(|s| s.id.capacity()).sum::<usize>();
    let id_to_slot_bytes = self.id_to_slot.capacity() * (std::mem::size_of::<String>() + std::mem::size_of::<usize>() + 1)
      + self.id_to_slot.keys().map(|k| k.capacity()).sum::<usize>();

    let index_bytes = grid_hashmap_bytes + vec_heap_bytes + slots_bytes + id_to_slot_bytes;
    let lm_bytes = self.landmarks.capacity() * (std::mem::size_of::<String>() + std::mem::size_of::<SpatialLandmark>() + 1)
      + self.landmarks.keys().map(|k| k.capacity()).sum::<usize>();
    let total_bytes = index_bytes + lm_bytes;

    (index_bytes, lm_bytes, total_bytes)
  }
}

// ============================================================================
// 5. CANDIDATE B: SPATIAL HASH STRING STORE (HashMap<CellKey, Vec<String>>)
// ============================================================================

#[derive(Clone, Debug, PartialEq)]
pub struct SpatialHashStringStore {
  pub landmarks: HashMap<String, SpatialLandmark>,
  pub path: PathBuf,
  pub config: SpatialMemoryConfig,
  pub grid: HashMap<CellKey, Vec<String>>,
  pub landmark_order: HashMap<String, usize>,
  pub next_order: usize,
}

impl SpatialHashStringStore {
  pub fn new(path: impl AsRef<Path>, config: SpatialMemoryConfig) -> Self {
    Self {
      landmarks: HashMap::new(),
      path: path.as_ref().to_path_buf(),
      config,
      grid: HashMap::new(),
      landmark_order: HashMap::new(),
      next_order: 0,
    }
  }

  #[inline]
  fn find_matching_landmark(&self, query_pos: BlockPosition) -> Option<String> {
    let (ix, iy, iz) = cell_coords(query_pos, self.config.dedup_radius_m);
    let mut best_id: Option<String> = None;
    let mut best_order = usize::MAX;

    for dx in -1..=1 {
      for dy in -1..=1 {
        for dz in -1..=1 {
          let neighbor_cell = (ix + dx, iy + dy, iz + dz);
          if let Some(ids) = self.grid.get(&neighbor_cell) {
            for id in ids {
              if let Some(lm) = self.landmarks.get(id) {
                let diff_x = f64::from(lm.position.x - query_pos.x);
                let diff_y = f64::from(lm.position.y - query_pos.y);
                let diff_z = f64::from(lm.position.z - query_pos.z);
                let dist = (diff_x * diff_x + diff_y * diff_y + diff_z * diff_z).sqrt();
                if dist < self.config.dedup_radius_m {
                  let order = self.landmark_order.get(id).copied().unwrap_or(usize::MAX);
                  if order < best_order {
                    best_order = order;
                    best_id = Some(id.clone());
                  }
                }
              }
            }
          }
        }
      }
    }

    best_id
  }

  pub fn upsert_from_raycast(&mut self, hit: &RaycastHit, obs: &ObservationRef) -> String {
    let matching_id = self.find_matching_landmark(hit.block_pos);

    if let Some(id) = matching_id {
      let _ = self.record_observation(&id, obs.captured_at_millis);
      let landmark = self.landmarks.get_mut(&id).expect("landmark exists");
      landmark.observations.push(LandmarkObservation {
        observation_ref: obs.clone(),
        source: LandmarkSource::TelemetryRaycast,
        hit_face: Some(hit.face),
        block_id: Some(hit.block_id.clone()),
      });
      landmark.surface_face = Some(hit.face);
      id
    } else {
      let kind = LandmarkKind::Static;
      let landmark_id = format!("lm-{}-{}-{}-{}", hit.block_pos.x, hit.block_pos.y, hit.block_pos.z, kind.as_str());
      let landmark = SpatialLandmark {
        landmark_id: landmark_id.clone(),
        kind,
        position: hit.block_pos,
        continuous_position: None,
        first_observed: obs.clone(),
        observations: vec![LandmarkObservation {
          observation_ref: obs.clone(),
          source: LandmarkSource::TelemetryRaycast,
          hit_face: Some(hit.face),
          block_id: Some(hit.block_id.clone()),
        }],
        status: SpatialClaimStatus::Confirmed,
        source: LandmarkSource::TelemetryRaycast,
        description: None,
        surface_face: Some(hit.face),
        observation_count: 1,
        last_observed_millis: obs.captured_at_millis,
        consecutive_misses: 0,
        confidence: 0.90,
      };
      self.landmarks.insert(landmark_id.clone(), landmark);
      self.landmark_order.insert(landmark_id.clone(), self.next_order);
      self.next_order += 1;
      let cell = cell_coords(hit.block_pos, self.config.dedup_radius_m);
      self.grid.entry(cell).or_default().push(landmark_id.clone());
      landmark_id
    }
  }

  pub fn upsert_from_perception(
    &mut self,
    block_pos: BlockPosition,
    label: &str,
    detection_confidence: f64,
    obs: &ObservationRef,
  ) -> String {
    let matching_id = self.find_matching_landmark(block_pos);

    if let Some(id) = matching_id {
      let landmark = self.landmarks.get_mut(&id).expect("landmark exists");
      landmark.observations.push(LandmarkObservation {
        observation_ref: obs.clone(),
        source: LandmarkSource::VisualPerception,
        hit_face: None,
        block_id: None,
      });
      landmark.observation_count += 1;
      landmark.consecutive_misses = 0;
      landmark.last_observed_millis = obs.captured_at_millis;

      if landmark.status == SpatialClaimStatus::Confirmed {
        let extra = (landmark.observation_count.saturating_sub(1) as f64) * 0.02;
        landmark.confidence = (0.90 + extra).min(0.99);
      } else {
        let candidate_conf = (detection_confidence * 0.5).clamp(0.1, 0.5);
        landmark.confidence = landmark.confidence.max(candidate_conf);
      }

      landmark.description = Some(label.to_string());
      id
    } else {
      let kind = LandmarkKind::Static;
      let landmark_id = format!("lm-{}-{}-{}-{}", block_pos.x, block_pos.y, block_pos.z, kind.as_str());
      let confidence = (detection_confidence * 0.5).clamp(0.1, 0.5);
      let landmark = SpatialLandmark {
        landmark_id: landmark_id.clone(),
        kind,
        position: block_pos,
        continuous_position: None,
        first_observed: obs.clone(),
        observations: vec![LandmarkObservation {
          observation_ref: obs.clone(),
          source: LandmarkSource::VisualPerception,
          hit_face: None,
          block_id: None,
        }],
        status: SpatialClaimStatus::Candidate,
        source: LandmarkSource::VisualPerception,
        description: Some(label.to_string()),
        surface_face: None,
        observation_count: 1,
        last_observed_millis: obs.captured_at_millis,
        consecutive_misses: 0,
        confidence,
      };
      self.landmarks.insert(landmark_id.clone(), landmark);
      self.landmark_order.insert(landmark_id.clone(), self.next_order);
      self.next_order += 1;
      let cell = cell_coords(block_pos, self.config.dedup_radius_m);
      self.grid.entry(cell).or_default().push(landmark_id.clone());
      landmark_id
    }
  }

  pub fn upsert_dynamic_landmark(
    &mut self,
    track_id: u64,
    ttl_millis: u64,
    block_pos: BlockPosition,
    continuous_pos: Option<(f64, f64, f64)>,
    label: &str,
    detection_confidence: f64,
    obs: &ObservationRef,
  ) -> String {
    let matching_id = self.landmarks.iter().find_map(|(id, lm)| {
      if let LandmarkKind::Dynamic { track_id: tid, .. } = lm.kind {
        if tid == track_id {
          return Some(id.clone());
        }
      }
      None
    });

    if let Some(id) = matching_id {
      let old_pos = self.landmarks.get(&id).unwrap().position;
      if old_pos != block_pos {
        let old_cell = cell_coords(old_pos, self.config.dedup_radius_m);
        let new_cell = cell_coords(block_pos, self.config.dedup_radius_m);
        if old_cell != new_cell {
          if let Some(ids) = self.grid.get_mut(&old_cell) {
            ids.retain(|item| item != &id);
            if ids.is_empty() {
              self.grid.remove(&old_cell);
            }
          }
          self.grid.entry(new_cell).or_default().push(id.clone());
        }
      }

      let landmark = self.landmarks.get_mut(&id).expect("landmark exists");
      landmark.position = block_pos;
      landmark.continuous_position = continuous_pos;
      landmark.observations.push(LandmarkObservation {
        observation_ref: obs.clone(),
        source: LandmarkSource::VisualPerception,
        hit_face: None,
        block_id: None,
      });
      landmark.observation_count += 1;
      landmark.consecutive_misses = 0;
      landmark.last_observed_millis = obs.captured_at_millis;

      if landmark.status == SpatialClaimStatus::Confirmed {
        let extra = (landmark.observation_count.saturating_sub(1) as f64) * 0.02;
        landmark.confidence = (0.90 + extra).min(0.99);
      } else {
        let candidate_conf = (detection_confidence * 0.5).clamp(0.1, 0.5);
        landmark.confidence = landmark.confidence.max(candidate_conf);
      }

      landmark.description = Some(label.to_string());
      landmark.kind = LandmarkKind::Dynamic {
        track_id,
        ttl_millis,
      };
      id
    } else {
      let kind = LandmarkKind::Dynamic {
        track_id,
        ttl_millis,
      };
      let landmark_id = format!("lm-track-{}-{}", track_id, kind.as_str());
      let confidence = (detection_confidence * 0.5).clamp(0.1, 0.5);
      let landmark = SpatialLandmark {
        landmark_id: landmark_id.clone(),
        kind,
        position: block_pos,
        continuous_position: continuous_pos,
        first_observed: obs.clone(),
        observations: vec![LandmarkObservation {
          observation_ref: obs.clone(),
          source: LandmarkSource::VisualPerception,
          hit_face: None,
          block_id: None,
        }],
        status: SpatialClaimStatus::Candidate,
        source: LandmarkSource::VisualPerception,
        description: Some(label.to_string()),
        surface_face: None,
        observation_count: 1,
        last_observed_millis: obs.captured_at_millis,
        consecutive_misses: 0,
        confidence,
      };
      self.landmarks.insert(landmark_id.clone(), landmark);
      self.landmark_order.insert(landmark_id.clone(), self.next_order);
      self.next_order += 1;
      let cell = cell_coords(block_pos, self.config.dedup_radius_m);
      self.grid.entry(cell).or_default().push(landmark_id.clone());
      landmark_id
    }
  }

  pub fn record_observation(&mut self, landmark_id: &str, now_millis: u64) -> Result<(), SpatialMemoryStoreError> {
    let landmark = self.landmarks.get_mut(landmark_id).ok_or_else(|| SpatialMemoryStoreError::LandmarkNotFound(landmark_id.to_string()))?;

    if landmark.observation_count == 0 && !landmark.observations.is_empty() {
      landmark.observation_count = landmark.observations.len() as u32;
    }
    landmark.observation_count += 1;
    landmark.consecutive_misses = 0;
    landmark.last_observed_millis = now_millis;
    let extra = (landmark.observation_count.saturating_sub(1) as f64) * 0.02;
    landmark.confidence = (0.90 + extra).min(0.99);
    Ok(())
  }

  pub fn record_miss(&mut self, landmark_id: &str) -> Result<(), SpatialMemoryStoreError> {
    let landmark = self.landmarks.get_mut(landmark_id).ok_or_else(|| SpatialMemoryStoreError::LandmarkNotFound(landmark_id.to_string()))?;

    landmark.consecutive_misses += 1;
    landmark.confidence = (landmark.confidence - 0.1).max(0.0);
    Ok(())
  }

  pub fn prune_stale(&mut self, now_millis: u64) -> usize {
    let stale_threshold = self.config.stale_threshold_millis;
    let min_conf = self.config.min_confidence;
    let max_misses = self.config.max_consecutive_misses;

    let to_remove: Vec<(String, BlockPosition)> = self
      .landmarks
      .values()
      .filter_map(|lm| {
        let should_prune = match lm.kind {
          LandmarkKind::Dynamic { ttl_millis, .. } => {
            ttl_millis > 0 && lm.last_observed_millis > 0 && now_millis.saturating_sub(lm.last_observed_millis) > ttl_millis
          }
          LandmarkKind::Static => {
            let is_expired =
              stale_threshold > 0 && lm.last_observed_millis > 0 && now_millis.saturating_sub(lm.last_observed_millis) > stale_threshold;
            let is_low_confidence = lm.confidence < min_conf;
            let is_too_many_misses = max_misses > 0 && lm.consecutive_misses >= max_misses;

            is_expired || is_low_confidence || is_too_many_misses
          }
        };
        if should_prune {
          Some((lm.landmark_id.clone(), lm.position))
        } else {
          None
        }
      })
      .collect();

    let pruned_count = to_remove.len();
    for (id, pos) in to_remove {
      self.landmarks.remove(&id);
      self.landmark_order.remove(&id);
      let cell = cell_coords(pos, self.config.dedup_radius_m);
      if let Some(ids) = self.grid.get_mut(&cell) {
        ids.retain(|item| item != &id);
        if ids.is_empty() {
          self.grid.remove(&cell);
        }
      }
    }

    pruned_count
  }

  pub fn len(&self) -> usize {
    self.landmarks.len()
  }
}

// ============================================================================
// 6. UNIT TESTS: GHOST INDEX & BOUNDARY REGRESSIONS
// ============================================================================

#[test]
fn test_ghost_index_pruning_and_reinsertion_regression() {
  let config = SpatialMemoryConfig {
    dedup_radius_m: 0.6,
    stale_threshold_millis: 1000,
    min_confidence: 0.3,
    max_consecutive_misses: 3,
  };
  let mut store = SpatialHashSlotStore::new("memory.json", config);

  let pos = BlockPosition::new(10, 64, 10);
  let hit = RaycastHit {
    block_pos: pos,
    face: BlockFace::Up,
    block_id: "minecraft:stone".to_string(),
  };
  let obs1 = ObservationRef {
    observation_id: "obs-1".to_string(),
    captured_at_millis: 100,
  };

  // 1. Initial Insert
  let id1 = store.upsert_from_raycast(&hit, &obs1);
  assert_eq!(store.len(), 1);
  let cell = cell_coords(pos, 0.6);
  assert_eq!(store.grid.get(&cell).map(|v| v.len()), Some(1));

  // 2. Cause misses and prune
  for _ in 0..4 {
    store.record_miss(&id1).unwrap();
  }
  let pruned = store.prune_stale(200);
  assert_eq!(pruned, 1);
  assert_eq!(store.len(), 0);

  // CRITICAL INVARIANT: Grid MUST be completely cleaned up, NO ghost indexes!
  assert!(store.grid.get(&cell).is_none(), "Pruned landmark must not leave ghost index or empty vector in grid!");
  assert!(store.id_to_slot.get(&id1).is_none());

  // 3. Re-insert at the EXACT same position
  let obs2 = ObservationRef {
    observation_id: "obs-2".to_string(),
    captured_at_millis: 300,
  };
  let id2 = store.upsert_from_raycast(&hit, &obs2);
  assert_eq!(store.len(), 1);
  assert_eq!(id1, id2); // Same deterministic ID

  let lm = store.landmarks.get(&id2).unwrap();
  assert_eq!(lm.observations.len(), 1, "Must be a fresh landmark, NOT merged with ghost!");
  assert_eq!(lm.observation_count, 1);
  assert_eq!(lm.confidence, 0.90);
  assert_eq!(lm.first_observed.observation_id, "obs-2");
}

#[test]
fn test_ghost_index_dynamic_movement_across_cells() {
  let config = SpatialMemoryConfig::default();
  let mut store = SpatialHashSlotStore::new("memory.json", config);

  let obs1 = ObservationRef {
    observation_id: "obs-dyn-1".to_string(),
    captured_at_millis: 1000,
  };
  let pos1 = BlockPosition::new(0, 64, 0);
  let id = store.upsert_dynamic_landmark(42, 10_000, pos1, None, "mob", 0.8, &obs1);

  let cell1 = cell_coords(pos1, config.dedup_radius_m);
  assert_eq!(store.grid.get(&cell1).map(|v| v.len()), Some(1));

  // Move mob to a far block (new cell)
  let pos2 = BlockPosition::new(10, 64, 10);
  let obs2 = ObservationRef {
    observation_id: "obs-dyn-2".to_string(),
    captured_at_millis: 2000,
  };
  let id_moved = store.upsert_dynamic_landmark(42, 10_000, pos2, None, "mob", 0.85, &obs2);
  assert_eq!(id, id_moved);

  let cell2 = cell_coords(pos2, config.dedup_radius_m);
  assert_ne!(cell1, cell2);

  // CRITICAL INVARIANT: Old cell MUST NOT contain the mob index anymore
  assert!(store.grid.get(&cell1).is_none(), "Old cell must be completely evacuated!");
  assert_eq!(store.grid.get(&cell2).map(|v| v.len()), Some(1));

  // Raycast hitting old pos must NOT merge with the mob!
  let hit_old = RaycastHit {
    block_pos: pos1,
    face: BlockFace::Up,
    block_id: "minecraft:dirt".to_string(),
  };
  let id_static = store.upsert_from_raycast(&hit_old, &obs2);
  assert_ne!(id_static, id, "Raycast at old location must not merge with moved entity!");
  assert_eq!(store.len(), 2);
}

// ============================================================================
// 7. DIFFERENTIAL TESTS: PROD STORE VS PURE O(N) VS SPATIAL HASH
// ============================================================================

#[test]
fn test_differential_parity_prod_store_vs_spatial_hash() {
  // Verifies that existing SpatialMemoryStore and SpatialHashSlotStore
  // behave with 100.0% parity on standard Minecraft raycasts and observations.
  let config = SpatialMemoryConfig::default();
  let mut prod_store = SpatialMemoryStore::open_with_config("memory.json", config).unwrap();
  let mut slot_store = SpatialHashSlotStore::new("memory.json", config);
  let mut str_store = SpatialHashStringStore::new("memory.json", config);
  let mut lin_store = PureLinearScanStore::new("memory.json", config);

  let mut rng = Mt19937Rng::new_with_seed(42);

  for i in 0..1000 {
    let pos = rng.random_block_position(-200, 200);
    let hit = RaycastHit {
      block_pos: pos,
      face: BlockFace::Up,
      block_id: "minecraft:stone".to_string(),
    };
    let obs = ObservationRef {
      observation_id: format!("obs-{}", i),
      captured_at_millis: 1000 + i as u64 * 10,
    };

    let id_prod = prod_store.upsert_from_raycast(&hit, &obs);
    let id_slot = slot_store.upsert_from_raycast(&hit, &obs);
    let id_str = str_store.upsert_from_raycast(&hit, &obs);
    let id_lin = lin_store.upsert_from_raycast(&hit, &obs);

    assert_eq!(id_prod, id_slot, "ID mismatch at step {i}");
    assert_eq!(id_prod, id_str, "ID mismatch at step {i}");
    assert_eq!(id_prod, id_lin, "ID mismatch at step {i}");
  }

  assert_eq!(prod_store.len(), slot_store.len());
  assert_eq!(prod_store.len(), str_store.len());
  assert_eq!(prod_store.len(), lin_store.len());

  for (id, lm_prod) in prod_store.landmarks() {
    let lm_slot = slot_store.landmarks.get(id).expect("landmark in slot store");
    assert_eq!(lm_prod, lm_slot, "Full landmark struct mismatch for {id}");
  }
}

#[test]
fn test_differential_interleaved_full_lifecycle() {
  // Heavy stress test: 5,000 interleaved operations
  // (Raycast upserts, perception upserts, dynamic moves, misses, and prune_stale)
  // Compares PureLinearScanStore vs SpatialHashSlotStore vs SpatialHashStringStore
  let config = SpatialMemoryConfig {
    dedup_radius_m: 0.6,
    stale_threshold_millis: 50_000,
    min_confidence: 0.3,
    max_consecutive_misses: 4,
  };

  let mut lin_store = PureLinearScanStore::new("memory.json", config);
  let mut slot_store = SpatialHashSlotStore::new("memory.json", config);
  let mut str_store = SpatialHashStringStore::new("memory.json", config);
  let mut prod_store = SpatialMemoryStore::open_with_config("memory.json", config).unwrap();

  let mut rng = Mt19937Rng::new_with_seed(2026_09_29);
  let mut now_millis = 1000u64;

  let mut pool_positions: Vec<BlockPosition> = Vec::new();
  for _ in 0..100 {
    pool_positions.push(rng.random_block_position(-50, 50));
  }

  let total_steps = 5000;
  for step in 0..total_steps {
    now_millis += (rng.uniform(10.0, 50.0)) as u64;
    let op = rng.uniform(0.0, 100.0);

    let obs = ObservationRef {
      observation_id: format!("obs-{}", step),
      captured_at_millis: now_millis,
    };

    if op < 45.0 {
      // 1. Raycast Upsert (45%): mix of random positions and existing pool positions (to trigger merges)
      let pos = if !pool_positions.is_empty() && rng.random_f64() < 0.40 {
        let idx = (rng.random_f64() * pool_positions.len() as f64) as usize;
        pool_positions[idx.min(pool_positions.len() - 1)]
      } else {
        let p = rng.random_block_position(-50, 50);
        pool_positions.push(p);
        p
      };

      let hit = RaycastHit {
        block_pos: pos,
        face: BlockFace::Up,
        block_id: "minecraft:stone".to_string(),
      };

      let id_lin = lin_store.upsert_from_raycast(&hit, &obs);
      let id_slot = slot_store.upsert_from_raycast(&hit, &obs);
      let id_str = str_store.upsert_from_raycast(&hit, &obs);
      let id_prod = prod_store.upsert_from_raycast(&hit, &obs);

      assert_eq!(id_lin, id_slot, "Raycast mismatch at step {step}");
      assert_eq!(id_lin, id_str, "Raycast mismatch at step {step}");
      assert_eq!(id_lin, id_prod, "Raycast prod mismatch at step {step}");
    } else if op < 70.0 {
      // 2. Perception Upsert (25%): testing visual perception Candidate status & authority upgrade
      let pos = if !pool_positions.is_empty() && rng.random_f64() < 0.40 {
        let idx = (rng.random_f64() * pool_positions.len() as f64) as usize;
        pool_positions[idx.min(pool_positions.len() - 1)]
      } else {
        let p = rng.random_block_position(-50, 50);
        pool_positions.push(p);
        p
      };

      let conf = rng.uniform(0.6, 0.95);
      let id_lin = lin_store.upsert_from_perception(pos, "chest", conf, &obs);
      let id_slot = slot_store.upsert_from_perception(pos, "chest", conf, &obs);
      let id_str = str_store.upsert_from_perception(pos, "chest", conf, &obs);
      let id_prod = prod_store.upsert_from_perception(pos, "chest", conf, &obs);

      assert_eq!(id_lin, id_slot, "Perception mismatch at step {step}");
      assert_eq!(id_lin, id_str, "Perception mismatch at step {step}");
      assert_eq!(id_lin, id_prod, "Perception prod mismatch at step {step}");
    } else if op < 82.0 {
      // 3. Dynamic Landmark Upsert (12%): tracking moving mobs across cells
      let track_id = (rng.uniform(1.0, 10.0)) as u64; // small set of tracks to force movements
      let pos = rng.random_block_position(-50, 50);
      let ttl = 15_000u64;

      let id_lin = lin_store.upsert_dynamic_landmark(track_id, ttl, pos, None, "cow", 0.8, &obs);
      let id_slot = slot_store.upsert_dynamic_landmark(track_id, ttl, pos, None, "cow", 0.8, &obs);
      let id_str = str_store.upsert_dynamic_landmark(track_id, ttl, pos, None, "cow", 0.8, &obs);
      let id_prod = prod_store.upsert_dynamic_landmark(track_id, ttl, pos, None, "cow", 0.8, &obs);

      assert_eq!(id_lin, id_slot, "Dynamic mismatch at step {step}");
      assert_eq!(id_lin, id_str, "Dynamic mismatch at step {step}");
      assert_eq!(id_lin, id_prod, "Dynamic prod mismatch at step {step}");
    } else if op < 92.0 {
      // 4. Record Miss (10%): penalizing existing landmarks
      if !lin_store.landmarks.is_empty() {
        let target_idx = (rng.random_f64() * lin_store.insertion_order.len() as f64) as usize;
        let id = lin_store.insertion_order[target_idx.min(lin_store.insertion_order.len() - 1)].clone();

        let _ = lin_store.record_miss(&id);
        let _ = slot_store.record_miss(&id);
        let _ = str_store.record_miss(&id);
        let _ = prod_store.record_miss(&id);
      }
    } else {
      // 5. Prune Stale (8%): advancing time and pruning expired / low confidence landmarks
      let pruned_lin = lin_store.prune_stale(now_millis);
      let pruned_slot = slot_store.prune_stale(now_millis);
      let pruned_str = str_store.prune_stale(now_millis);
      let pruned_prod = prod_store.prune_stale(now_millis);

      assert_eq!(pruned_lin, pruned_slot, "Prune count mismatch at step {step}");
      assert_eq!(pruned_lin, pruned_str, "Prune count mismatch at step {step}");
      assert_eq!(pruned_lin, pruned_prod, "Prune count prod mismatch at step {step}");
    }

    // Intermediate state assertions every 250 steps
    if step % 250 == 0 {
      assert_eq!(lin_store.len(), slot_store.len(), "Store length mismatch at step {step}");
      assert_eq!(lin_store.len(), str_store.len(), "Store length mismatch at step {step}");
      assert_eq!(lin_store.len(), prod_store.len(), "Store length prod mismatch at step {step}");

      for (id, lm_lin) in &lin_store.landmarks {
        let lm_slot = slot_store.landmarks.get(id).expect("landmark in slot store");
        assert_eq!(lm_lin.position, lm_slot.position);
        assert_eq!(lm_lin.observation_count, lm_slot.observation_count);
        assert!((lm_lin.confidence - lm_slot.confidence).abs() < 1e-6);
        assert_eq!(lm_lin.status, lm_slot.status);

        let lm_prod = prod_store.landmarks().get(id).expect("landmark in prod store");
        assert_eq!(lm_lin.position, lm_prod.position);
        assert_eq!(lm_lin.observation_count, lm_prod.observation_count);
        assert!((lm_lin.confidence - lm_prod.confidence).abs() < 1e-6);
        assert_eq!(lm_lin.status, lm_prod.status);
      }
    }
  }

  // Final exact comparison across all remaining landmarks
  assert_eq!(lin_store.len(), slot_store.len());
  assert_eq!(lin_store.len(), str_store.len());
  assert_eq!(lin_store.len(), prod_store.len());
  println!("\nDifferential 5,000 steps PASSED! Final live landmarks count: {}", prod_store.len());

  for (id, lm_lin) in &lin_store.landmarks {
    let lm_slot = slot_store.landmarks.get(id).unwrap();
    let lm_str = str_store.landmarks.get(id).unwrap();
    let lm_prod = prod_store.landmarks().get(id).unwrap();

    assert_eq!(lm_lin, lm_slot, "Final landmark equality failed for {id}");
    assert_eq!(lm_lin, lm_str, "Final landmark equality failed for {id}");
    assert_eq!(lm_lin, lm_prod, "Final prod landmark equality failed for {id}");
  }
}

// ============================================================================
// 8. BENCHMARK SUITE: 1K / 10K / 100K WITH LATENCY & MEMORY REPORTING
// ============================================================================

#[derive(Debug, Clone)]
pub struct LatencyStats {
  pub p50_ns: f64,
  pub p95_ns: f64,
  pub p99_ns: f64,
  pub max_ns: f64,
  pub total_ms: f64,
}

pub fn compute_stats(mut latencies: Vec<f64>) -> LatencyStats {
  latencies.sort_by(|a, b| a.total_cmp(b));
  let n = latencies.len();
  let total_ms = latencies.iter().sum::<f64>() / 1_000_000.0;
  let p50_ns = latencies[(n as f64 * 0.50) as usize];
  let p95_ns = latencies[((n as f64 * 0.95) as usize).min(n - 1)];
  let p99_ns = latencies[((n as f64 * 0.99) as usize).min(n - 1)];
  let max_ns = latencies[n - 1];
  LatencyStats {
    p50_ns,
    p95_ns,
    p99_ns,
    max_ns,
    total_ms,
  }
}

#[test]
#[ignore = "100k scale benchmark takes ~15-25m in debug mode; run with --release -- --ignored"]
fn test_benchmark_suite_production_candidates() {
  let scales = [1_000, 10_000, 100_000];
  let config = SpatialMemoryConfig::default();

  println!("\n================================================================================");
  println!("S2 PRODUCTION INTEGRATION BENCHMARK REPORT");
  println!("Dedup radius R = {} m", config.dedup_radius_m);
  println!("Seed: 7 (CPython MT19937 compatible)");
  println!("================================================================================\n");

  println!("| Scale (N) | Store Backend                   | Total (ms) | p50 (µs) | p95 (µs) | Max (µs) |");
  println!("| :--- | :--- | :--- | :--- | :--- | :--- |");

  for &scale in &scales {
    let mut rng = Mt19937Rng::new_with_seed(7);
    let mut points = Vec::with_capacity(scale);
    for _ in 0..scale {
      points.push(rng.random_block_position(-1000, 1000));
    }

    // 1. Pure Linear Scan Store (Old Baseline)
    let mut lin_store = PureLinearScanStore::new("memory.json", config);
    let mut lin_lats = Vec::with_capacity(scale);
    for (i, &pos) in points.iter().enumerate() {
      let hit = RaycastHit {
        block_pos: pos,
        face: BlockFace::Up,
        block_id: "minecraft:stone".to_string(),
      };
      let obs = ObservationRef {
        observation_id: format!("obs-{}", i),
        captured_at_millis: i as u64,
      };
      let t0 = Instant::now();
      lin_store.upsert_from_raycast(&hit, &obs);
      lin_lats.push(t0.elapsed().as_nanos() as f64);
    }
    let lin_stats = compute_stats(lin_lats);

    println!(
      "| {:>9} | Linear O(N) Baseline            | {:>10.2} | {:>8.2} | {:>8.2} | {:>8.2} |",
      scale,
      lin_stats.total_ms,
      lin_stats.p50_ns / 1000.0,
      lin_stats.p95_ns / 1000.0,
      lin_stats.max_ns / 1000.0
    );

    // 2. Production Store (SpatialMemoryStore with merged spatial hash)
    let mut prod_store = SpatialMemoryStore::open_with_config("memory.json", config).unwrap();
    let mut prod_lats = Vec::with_capacity(scale);
    for (i, &pos) in points.iter().enumerate() {
      let hit = RaycastHit {
        block_pos: pos,
        face: BlockFace::Up,
        block_id: "minecraft:stone".to_string(),
      };
      let obs = ObservationRef {
        observation_id: format!("obs-{}", i),
        captured_at_millis: i as u64,
      };
      let t0 = Instant::now();
      prod_store.upsert_from_raycast(&hit, &obs);
      prod_lats.push(t0.elapsed().as_nanos() as f64);
    }
    let prod_stats = compute_stats(prod_lats);

    println!(
      "| {:>9} | Production Store (Spatial Hash) | {:>10.2} | {:>8.2} | {:>8.2} | {:>8.2} |",
      scale,
      prod_stats.total_ms,
      prod_stats.p50_ns / 1000.0,
      prod_stats.p95_ns / 1000.0,
      prod_stats.max_ns / 1000.0
    );

    // 3. Candidate A: SpatialHashSlotStore (HashMap<CellKey, Vec<usize>>)
    let mut slot_store = SpatialHashSlotStore::new("memory.json", config);
    let mut slot_lats = Vec::with_capacity(scale);
    for (i, &pos) in points.iter().enumerate() {
      let hit = RaycastHit {
        block_pos: pos,
        face: BlockFace::Up,
        block_id: "minecraft:stone".to_string(),
      };
      let obs = ObservationRef {
        observation_id: format!("obs-{}", i),
        captured_at_millis: i as u64,
      };
      let t0 = Instant::now();
      slot_store.upsert_from_raycast(&hit, &obs);
      slot_lats.push(t0.elapsed().as_nanos() as f64);
    }
    let slot_stats = compute_stats(slot_lats);

    println!(
      "| {:>9} | Candidate A Prototype           | {:>10.2} | {:>8.2} | {:>8.2} | {:>8.2} |",
      scale,
      slot_stats.total_ms,
      slot_stats.p50_ns / 1000.0,
      slot_stats.p95_ns / 1000.0,
      slot_stats.max_ns / 1000.0
    );

    if scale == 100_000 {
      let (idx_b, lm_b, total_b) = slot_store.estimate_memory_bytes();
      println!("\n--- 100k Memory & Speedup Inspection ---");
      println!("Production Index Memory Est: {:.2} MB", idx_b as f64 / (1024.0 * 1024.0));
      println!("Production Landmark Memory Est: {:.2} MB", lm_b as f64 / (1024.0 * 1024.0));
      println!("Production Total Memory Est: {:.2} MB", total_b as f64 / (1024.0 * 1024.0));

      let speedup_total = lin_stats.total_ms / prod_stats.total_ms.max(0.001);
      let speedup_p95 = lin_stats.p95_ns / prod_stats.p95_ns.max(1.0);
      println!("Production Speedup vs O(N) at 100k:");
      println!("  Total Time Speedup: {:.1}x", speedup_total);
      println!("  p95 Latency Speedup: {:.1}x", speedup_p95);

      // Verify GO gate: p95 < 1.0ms
      assert!(
        prod_stats.p95_ns / 1_000_000.0 < 1.0,
        "Production Store p95 at 100k must be < 1.0ms, got {:.3}ms",
        prod_stats.p95_ns / 1_000_000.0
      );
    }
  }
}
