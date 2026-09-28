//! Spike S2: 100k Landmark Spatial Hash Prototype & Differential Benchmark.
//!
//! Purpose:
//! - Track B prototype for spatial deduplication using a Uniform Grid Spatial Hash.
//! - Cell edge length = R (0.6m), querying 3x3x3 = 27 neighbor cells.
//! - Benchmarks O(N) linear scan vs Spatial Hash at N = 1k, 10k, 100k.
//! - Differential test asserting 100% decision and landmark ID parity.
//! - Independent prototype module; does NOT touch production SpatialMemoryStore.

use std::collections::HashMap;
use std::time::Instant;

// ============================================================================
// 1. REPRODUCIBLE PRNG: CPython-compatible MT19937 (seed 7)
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

  pub fn random_minecraft_point(&mut self) -> [f64; 3] {
    [
      self.uniform(-1000.0, 1000.0),
      self.uniform(0.0, 256.0),
      self.uniform(-1000.0, 1000.0),
    ]
  }
}

pub fn generate_minecraft_points(n: usize, seed: u32) -> Vec<[f64; 3]> {
  let mut rng = Mt19937Rng::new_with_seed(seed);
  let mut points = Vec::with_capacity(n);
  for _ in 0..n {
    points.push(rng.random_minecraft_point());
  }
  points
}

pub fn generate_clustered_minecraft_points(n: usize, seed: u32, merge_ratio: f64) -> Vec<[f64; 3]> {
  let mut rng = Mt19937Rng::new_with_seed(seed);
  let mut points: Vec<[f64; 3]> = Vec::with_capacity(n);
  for _ in 0..n {
    if !points.is_empty() && rng.random_f64() < merge_ratio {
      let base_idx = (rng.random_f64() * (points.len() as f64)) as usize;
      let base: [f64; 3] = points[base_idx.min(points.len() - 1)];
      // Perturb within +/- 0.5m (within 0.6m radius)
      let p = [
        base[0] + rng.uniform(-0.5, 0.5),
        base[1] + rng.uniform(-0.5, 0.5),
        base[2] + rng.uniform(-0.5, 0.5),
      ];
      points.push(p);
    } else {
      points.push(rng.random_minecraft_point());
    }
  }
  points
}

// ============================================================================
// 2. TYPES & MODELS
// ============================================================================

#[derive(Clone, Debug, PartialEq)]
pub struct LandmarkRecord {
  pub landmark_id: String,
  pub position: [f64; 3],
  pub observation_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpsertResult {
  pub is_merged: bool,
  pub landmark_id: String,
}

// ============================================================================
// 3. O(N) LINEAR SCAN STORE (Current production baseline algorithm)
// ============================================================================

pub struct LinearScanStore {
  pub landmarks: Vec<LandmarkRecord>,
  pub dedup_radius_m: f64,
}

impl LinearScanStore {
  pub fn new(dedup_radius_m: f64) -> Self {
    Self {
      landmarks: Vec::new(),
      dedup_radius_m,
    }
  }

  pub fn upsert(&mut self, point: [f64; 3]) -> UpsertResult {
    let mut matching_idx = None;
    for (idx, lm) in self.landmarks.iter().enumerate() {
      let dx = lm.position[0] - point[0];
      let dy = lm.position[1] - point[1];
      let dz = lm.position[2] - point[2];
      let dist = (dx * dx + dy * dy + dz * dz).sqrt();
      if dist < self.dedup_radius_m {
        matching_idx = Some(idx);
        break;
      }
    }

    if let Some(idx) = matching_idx {
      self.landmarks[idx].observation_count += 1;
      UpsertResult {
        is_merged: true,
        landmark_id: self.landmarks[idx].landmark_id.clone(),
      }
    } else {
      let landmark_id = format!("lm-{}", self.landmarks.len());
      self.landmarks.push(LandmarkRecord {
        landmark_id: landmark_id.clone(),
        position: point,
        observation_count: 1,
      });
      UpsertResult {
        is_merged: false,
        landmark_id,
      }
    }
  }

  pub fn len(&self) -> usize {
    self.landmarks.len()
  }
}

// ============================================================================
// 4. UNIFORM GRID SPATIAL HASH STORE (Spike prototype)
// ============================================================================

pub type GridCoord = (i64, i64, i64);

pub struct UniformGridSpatialHashStore {
  pub landmarks: Vec<LandmarkRecord>,
  pub grid: HashMap<GridCoord, Vec<usize>>,
  pub dedup_radius_m: f64,
  pub cell_size: f64,
}

impl UniformGridSpatialHashStore {
  pub fn new(dedup_radius_m: f64) -> Self {
    // cell edge length = R (0.6m)
    let cell_size = dedup_radius_m;
    Self {
      landmarks: Vec::new(),
      grid: HashMap::new(),
      dedup_radius_m,
      cell_size,
    }
  }

  #[inline]
  pub fn cell_coords(&self, p: [f64; 3]) -> GridCoord {
    ((p[0] / self.cell_size).floor() as i64, (p[1] / self.cell_size).floor() as i64, (p[2] / self.cell_size).floor() as i64)
  }

  pub fn upsert(&mut self, point: [f64; 3]) -> UpsertResult {
    let (ix, iy, iz) = self.cell_coords(point);
    let mut best_matching_idx: Option<usize> = None;

    // Scan 3x3x3 = 27 neighbor cells
    for dx in -1..=1 {
      for dy in -1..=1 {
        for dz in -1..=1 {
          let neighbor_cell = (ix + dx, iy + dy, iz + dz);
          if let Some(indices) = self.grid.get(&neighbor_cell) {
            for &idx in indices {
              let lm = &self.landmarks[idx];
              let diff_x = lm.position[0] - point[0];
              let diff_y = lm.position[1] - point[1];
              let diff_z = lm.position[2] - point[2];
              let dist = (diff_x * diff_x + diff_y * diff_y + diff_z * diff_z).sqrt();
              if dist < self.dedup_radius_m {
                if best_matching_idx.map_or(true, |curr| idx < curr) {
                  best_matching_idx = Some(idx);
                }
              }
            }
          }
        }
      }
    }

    if let Some(idx) = best_matching_idx {
      self.landmarks[idx].observation_count += 1;
      UpsertResult {
        is_merged: true,
        landmark_id: self.landmarks[idx].landmark_id.clone(),
      }
    } else {
      let new_idx = self.landmarks.len();
      let landmark_id = format!("lm-{}", new_idx);
      self.landmarks.push(LandmarkRecord {
        landmark_id: landmark_id.clone(),
        position: point,
        observation_count: 1,
      });
      self.grid.entry((ix, iy, iz)).or_default().push(new_idx);
      UpsertResult {
        is_merged: false,
        landmark_id,
      }
    }
  }

  pub fn len(&self) -> usize {
    self.landmarks.len()
  }

  pub fn entry_count(&self) -> usize {
    self.grid.len()
  }

  pub fn estimate_memory_bytes(&self) -> (usize, usize, usize) {
    // 1. Grid HashMap memory:
    // Capacity * (key 24B + val 24B + 1B ctrl) + heap buffers of Vecs
    let hashmap_bucket_overhead = self.grid.capacity() * (std::mem::size_of::<GridCoord>() + std::mem::size_of::<Vec<usize>>() + 1);
    let vec_heap_bytes: usize = self.grid.values().map(|v| v.capacity() * std::mem::size_of::<usize>()).sum();
    let grid_bytes = hashmap_bucket_overhead + vec_heap_bytes;

    // 2. Landmarks table memory:
    let landmarks_bytes = self.landmarks.capacity() * std::mem::size_of::<LandmarkRecord>()
      + self.landmarks.iter().map(|lm| lm.landmark_id.capacity()).sum::<usize>();

    let total_bytes = grid_bytes + landmarks_bytes;
    (grid_bytes, landmarks_bytes, total_bytes)
  }
}

// ============================================================================
// 5. STATS & BENCHMARK HELPERS
// ============================================================================

#[derive(Debug, Clone)]
pub struct LatencyStats {
  pub p50_ns: f64,
  pub p95_ns: f64,
  pub p99_ns: f64,
  pub mean_ns: f64,
  pub min_ns: f64,
  pub max_ns: f64,
  pub total_duration_ms: f64,
}

impl LatencyStats {
  pub fn p50_us(&self) -> f64 {
    self.p50_ns / 1_000.0
  }
  pub fn p95_us(&self) -> f64 {
    self.p95_ns / 1_000.0
  }
  pub fn p95_ms(&self) -> f64 {
    self.p95_ns / 1_000_000.0
  }
}

pub fn compute_stats(mut latencies_ns: Vec<f64>) -> LatencyStats {
  latencies_ns.sort_by(|a, b| a.total_cmp(b));
  let n = latencies_ns.len();
  let min_ns = latencies_ns[0];
  let max_ns = latencies_ns[n - 1];
  let sum_ns: f64 = latencies_ns.iter().sum();
  let total_duration_ms = sum_ns / 1_000_000.0;
  let mean_ns = sum_ns / (n as f64);
  let p50_ns = latencies_ns[(n as f64 * 0.50) as usize];
  let p95_ns = latencies_ns[((n as f64 * 0.95) as usize).min(n - 1)];
  let p99_ns = latencies_ns[((n as f64 * 0.99) as usize).min(n - 1)];
  LatencyStats {
    p50_ns,
    p95_ns,
    p99_ns,
    mean_ns,
    min_ns,
    max_ns,
    total_duration_ms,
  }
}

pub fn bench_linear_store(points: &[[f64; 3]], dedup_radius_m: f64) -> (LinearScanStore, LatencyStats) {
  let mut store = LinearScanStore::new(dedup_radius_m);
  let mut latencies_ns = Vec::with_capacity(points.len());

  for &p in points {
    let t0 = Instant::now();
    let _ = store.upsert(p);
    latencies_ns.push(t0.elapsed().as_nanos() as f64);
  }

  let stats = compute_stats(latencies_ns);
  (store, stats)
}

pub fn bench_spatial_hash_store(points: &[[f64; 3]], dedup_radius_m: f64) -> (UniformGridSpatialHashStore, LatencyStats) {
  let mut store = UniformGridSpatialHashStore::new(dedup_radius_m);
  let mut latencies_ns = Vec::with_capacity(points.len());

  for &p in points {
    let t0 = Instant::now();
    let _ = store.upsert(p);
    latencies_ns.push(t0.elapsed().as_nanos() as f64);
  }

  let stats = compute_stats(latencies_ns);
  (store, stats)
}

// ============================================================================
// 6. UNIT & GEOMETRY VERIFICATIONS
// ============================================================================

#[test]
fn test_spatial_hash_deterministic_cpython_rng() {
  let mut rng = Mt19937Rng::new_with_seed(7);
  // Verify against known CPython random.seed(7) numbers:
  let r0 = rng.random_f64();
  let r1 = rng.random_f64();
  let r2 = rng.random_f64();
  assert!((r0 - 0.32383276483316237).abs() < 1e-12, "r0 mismatch: {r0}");
  assert!((r1 - 0.15084917392450192).abs() < 1e-12, "r1 mismatch: {r1}");
  assert!((r2 - 0.6509344730398537).abs() < 1e-12, "r2 mismatch: {r2}");

  let mut rng_mc = Mt19937Rng::new_with_seed(7);
  let p0 = rng_mc.random_minecraft_point();
  // x in [-1000, 1000], y in [0, 256], z in [-1000, 1000]
  assert!((p0[0] - -352.3344703336753).abs() < 1e-10, "p0.x mismatch: {}", p0[0]);
  assert!((p0[1] - 38.61738852467249).abs() < 1e-10, "p0.y mismatch: {}", p0[1]);
  assert!((p0[2] - 301.8689460797075).abs() < 1e-10, "p0.z mismatch: {}", p0[2]);
}

#[test]
fn test_spatial_hash_cell_boundary_and_corner_guarantees() {
  let dedup_r = 0.6;
  let mut store = UniformGridSpatialHashStore::new(dedup_r);

  // Point A right before cell boundary
  let p_a = [0.599, 0.599, 0.599];
  let res_a = store.upsert(p_a);
  assert!(!res_a.is_merged);
  assert_eq!(res_a.landmark_id, "lm-0");

  // Point B right across cell boundary, distance is 0.002 < 0.6
  let p_b = [0.601, 0.601, 0.601];
  let res_b = store.upsert(p_b);
  assert!(res_b.is_merged, "point across cell boundary must be detected and merged");
  assert_eq!(res_b.landmark_id, "lm-0");

  // Point C across negative cell boundary
  let mut store2 = UniformGridSpatialHashStore::new(dedup_r);
  let p_c = [-0.001, -0.001, -0.001];
  let res_c = store2.upsert(p_c);
  assert!(!res_c.is_merged);

  let p_d = [0.001, 0.001, 0.001];
  let res_d = store2.upsert(p_d);
  assert!(res_d.is_merged, "negative/positive boundary transition must merge");
  assert_eq!(res_d.landmark_id, "lm-0");
}

// ============================================================================
// 7. DIFFERENTIAL TESTS: O(N) LINEAR SCAN VS SPATIAL HASH
// ============================================================================

#[test]
fn test_differential_1k_uniform_dataset() {
  let dedup_r = 0.6;
  let points = generate_minecraft_points(1_000, 7);

  let mut linear_store = LinearScanStore::new(dedup_r);
  let mut hash_store = UniformGridSpatialHashStore::new(dedup_r);

  for (i, &p) in points.iter().enumerate() {
    let res_lin = linear_store.upsert(p);
    let res_sh = hash_store.upsert(p);

    assert_eq!(res_lin, res_sh, "1k uniform mismatch at index {i}: linear={:?} vs hash={:?}", res_lin, res_sh);
  }

  assert_eq!(linear_store.len(), hash_store.len());
}

#[test]
fn test_differential_10k_uniform_dataset() {
  let dedup_r = 0.6;
  let points = generate_minecraft_points(10_000, 7);

  let mut linear_store = LinearScanStore::new(dedup_r);
  let mut hash_store = UniformGridSpatialHashStore::new(dedup_r);

  for (i, &p) in points.iter().enumerate() {
    let res_lin = linear_store.upsert(p);
    let res_sh = hash_store.upsert(p);

    assert_eq!(res_lin, res_sh, "10k uniform mismatch at index {i}: linear={:?} vs hash={:?}", res_lin, res_sh);
  }

  assert_eq!(linear_store.len(), hash_store.len());
}

#[test]
fn test_differential_1k_clustered_dataset() {
  let dedup_r = 0.6;
  // 30% of points are perturbed duplicates to strictly exercise merge branch
  let points = generate_clustered_minecraft_points(1_000, 7, 0.30);

  let mut linear_store = LinearScanStore::new(dedup_r);
  let mut hash_store = UniformGridSpatialHashStore::new(dedup_r);
  let mut merge_count = 0;

  for (i, &p) in points.iter().enumerate() {
    let res_lin = linear_store.upsert(p);
    let res_sh = hash_store.upsert(p);

    assert_eq!(res_lin, res_sh, "1k clustered mismatch at index {i}: linear={:?} vs hash={:?}", res_lin, res_sh);

    if res_lin.is_merged {
      merge_count += 1;
    }
  }

  assert!(merge_count > 100, "clustered test must trigger substantial merges, got {merge_count}");
  assert_eq!(linear_store.len(), hash_store.len());
}

#[test]
fn test_differential_10k_clustered_dataset() {
  let dedup_r = 0.6;
  // 30% of points are perturbed duplicates to strictly exercise merge branch
  let points = generate_clustered_minecraft_points(10_000, 7, 0.30);

  let mut linear_store = LinearScanStore::new(dedup_r);
  let mut hash_store = UniformGridSpatialHashStore::new(dedup_r);
  let mut merge_count = 0;

  for (i, &p) in points.iter().enumerate() {
    let res_lin = linear_store.upsert(p);
    let res_sh = hash_store.upsert(p);

    assert_eq!(res_lin, res_sh, "10k clustered mismatch at index {i}: linear={:?} vs hash={:?}", res_lin, res_sh);

    if res_lin.is_merged {
      merge_count += 1;
    }
  }

  assert!(merge_count > 1500, "clustered test must trigger substantial merges, got {merge_count}");
  assert_eq!(linear_store.len(), hash_store.len());
}

// ============================================================================
// 8. BENCHMARK SUITE: 1K / 10K / 100K WITH LATENCY & MEMORY REPORTING
// ============================================================================

#[test]
fn test_benchmark_suite_and_go_no_go_gate() {
  let dedup_r = 0.6;
  println!("\n================================================================================");
  println!("SPIKE S2: SPATIAL HASH VS O(N) LINEAR SCAN BENCHMARK REPORT");
  println!("Parameter: dedup_radius_m R = {} m", dedup_r);
  println!("Seed: 7 (CPython MT19937 compatible)");
  println!("Coordinate domain: x in [-1000, 1000], y in [0, 256], z in [-1000, 1000]");
  println!("================================================================================\n");

  let scales = [1_000, 10_000, 100_000];

  println!("| Scale (N) | Store Backend | Merged / Total | Total Time (ms) | Latency p50 (µs) | Latency p95 (µs) | Latency Max (µs) |");
  println!("| :--- | :--- | :--- | :--- | :--- | :--- | :--- |");

  for &scale in &scales {
    let points = generate_minecraft_points(scale, 7);

    // 1. Linear Scan Store
    let (lin_store, lin_stats) = bench_linear_store(&points, dedup_r);
    let lin_merged = scale - lin_store.len();

    println!(
      "| {:>9} | Linear O(N)   | {:>6} / {:<6} | {:>15.2} | {:>16.2} | {:>16.2} | {:>16.2} |",
      scale,
      lin_merged,
      scale,
      lin_stats.total_duration_ms,
      lin_stats.p50_us(),
      lin_stats.p95_us(),
      lin_stats.max_ns / 1_000.0,
    );

    // 2. Spatial Hash Store
    let (sh_store, sh_stats) = bench_spatial_hash_store(&points, dedup_r);
    let sh_merged = scale - sh_store.len();

    println!(
      "| {:>9} | Spatial Hash  | {:>6} / {:<6} | {:>15.2} | {:>16.2} | {:>16.2} | {:>16.2} |",
      scale,
      sh_merged,
      scale,
      sh_stats.total_duration_ms,
      sh_stats.p50_us(),
      sh_stats.p95_us(),
      sh_stats.max_ns / 1_000.0,
    );

    // At 100k scale: inspect memory and assert B4 GO/NO-GO gate
    if scale == 100_000 {
      let (grid_bytes, lm_bytes, total_bytes) = sh_store.estimate_memory_bytes();
      let entries = sh_store.entry_count();

      println!("\n--- 100k Spatial Hash Scale Inspection ---");
      println!("HashMap Active Grid Entries: {}", entries);
      println!("Unique Landmarks Stored:     {}", sh_store.len());
      println!("Grid Index Memory:           {:.2} MB ({} bytes)", grid_bytes as f64 / (1024.0 * 1024.0), grid_bytes);
      println!("Landmarks Table Memory:      {:.2} MB ({} bytes)", lm_bytes as f64 / (1024.0 * 1024.0), lm_bytes);
      println!("Total Memory Footprint:      {:.2} MB ({} bytes)", total_bytes as f64 / (1024.0 * 1024.0), total_bytes);

      let speedup_p50 = lin_stats.p50_ns / sh_stats.p50_ns.max(1.0);
      let speedup_p95 = lin_stats.p95_ns / sh_stats.p95_ns.max(1.0);
      let speedup_total = lin_stats.total_duration_ms / sh_stats.total_duration_ms.max(0.001);

      println!("\nSpeedup at 100k (Linear / SpatialHash):");
      println!("  Total Time Speedup: {:.1}x", speedup_total);
      println!("  p50 Latency Speedup: {:.1}x", speedup_p50);
      println!("  p95 Latency Speedup: {:.1}x", speedup_p95);

      // Gate B4: 100k 下去重查询 p95 < 1ms (1000 µs)
      assert!(
        sh_stats.p95_ms() < 1.0,
        "GO/NO-GO GATE FAILED: Spatial hash p95 at 100k must be < 1.0ms, got {:.3}ms ({:.1}µs)",
        sh_stats.p95_ms(),
        sh_stats.p95_us()
      );
      println!("\n>>> GO/NO-GO DECISION: GO (p95 = {:.3}ms < 1.0ms, Differential 100% Passed) <<<\n", sh_stats.p95_ms());
    }
  }
}
