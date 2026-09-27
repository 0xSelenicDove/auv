use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::time::Instant;

use auv_game_minecraft::projection::MinecraftProjector;
use auv_game_minecraft::types::{BlockPosition, MinecraftSpatialFrame, NearbyBlock, Vec3};
use serde::{Deserialize, Serialize};

const WHITELIST_CLASSES: &[(&str, usize)] = &[
  ("grass_block", 0),
  ("chest", 1),
  ("furnace", 2),
  ("crafting_table", 3),
  ("door", 4),
  ("torch", 5),
];

fn match_whitelist_class(block_id: &str) -> Option<usize> {
  let normalized = block_id.strip_prefix("minecraft:").unwrap_or(block_id);
  for &(class_name, class_id) in WHITELIST_CLASSES {
    if normalized == class_name || normalized.contains(class_name) {
      return Some(class_id);
    }
  }
  None
}

#[derive(Debug, Clone)]
struct ProjectedBBox {
  class_id: usize,
  class_name: &'static str,
  #[allow(dead_code)]
  block_pos: BlockPosition,
  #[allow(dead_code)]
  distance: f64,
  yolo: [f64; 4],  // [cx, cy, w, h]
  pixel: [f64; 4], // [min_x, min_y, max_x, max_y]
}

#[derive(Debug, Serialize, Deserialize)]
struct AutolabelReport {
  total_frames_processed: usize,
  total_frames_with_boxes: usize,
  total_boxes_generated: usize,
  elapsed_ms: u128,
  throughput_fps: f64,
  throughput_fpm: f64,
  min_distance_m: f64,
  max_distance_m: f64,
  class_distribution: HashMap<String, usize>,
  overlay_images_generated: Vec<String>,
  notes: Vec<String>,
}

fn ray_intersects_block_aabb(eye: Vec3, dir_x: f64, dir_y: f64, dir_z: f64, block_pos: BlockPosition, max_dist: f64) -> bool {
  let min_x = block_pos.x as f64;
  let max_x = min_x + 1.0;
  let min_y = block_pos.y as f64;
  let max_y = min_y + 1.0;
  let min_z = block_pos.z as f64;
  let max_z = min_z + 1.0;

  let (t1_x, t2_x) = if dir_x.abs() < 1e-9 {
    if eye.x < min_x || eye.x > max_x {
      return false;
    }
    (f64::NEG_INFINITY, f64::INFINITY)
  } else {
    let t1 = (min_x - eye.x) / dir_x;
    let t2 = (max_x - eye.x) / dir_x;
    (t1.min(t2), t1.max(t2))
  };

  let (t1_y, t2_y) = if dir_y.abs() < 1e-9 {
    if eye.y < min_y || eye.y > max_y {
      return false;
    }
    (f64::NEG_INFINITY, f64::INFINITY)
  } else {
    let t1 = (min_y - eye.y) / dir_y;
    let t2 = (max_y - eye.y) / dir_y;
    (t1.min(t2), t1.max(t2))
  };

  let (t1_z, t2_z) = if dir_z.abs() < 1e-9 {
    if eye.z < min_z || eye.z > max_z {
      return false;
    }
    (f64::NEG_INFINITY, f64::INFINITY)
  } else {
    let t1 = (min_z - eye.z) / dir_z;
    let t2 = (max_z - eye.z) / dir_z;
    (t1.min(t2), t1.max(t2))
  };

  let t_enter = t1_x.max(t1_y).max(t1_z);
  let t_exit = t2_x.min(t2_y).min(t2_z);

  t_exit >= t_enter && t_exit > 0.05 && t_enter < (max_dist - 0.2)
}

fn is_occluded_by_closer_block(eye: Vec3, target_center: Vec3, target_pos: BlockPosition, nearby: &[NearbyBlock]) -> bool {
  let dx = target_center.x - eye.x;
  let dy = target_center.y - eye.y;
  let dz = target_center.z - eye.z;
  let dist = (dx * dx + dy * dy + dz * dz).sqrt();
  if dist < 1e-4 {
    return false;
  }
  let dir_x = dx / dist;
  let dir_y = dy / dist;
  let dir_z = dz / dist;

  for other in nearby {
    if other.block_pos == target_pos {
      continue;
    }
    let ob_center = other.block_pos.center();
    let odx = ob_center.x - eye.x;
    let ody = ob_center.y - eye.y;
    let odz = ob_center.z - eye.z;
    let o_dist_sq = odx * odx + ody * ody + odz * odz;
    if o_dist_sq >= dist * dist {
      continue;
    }

    if ray_intersects_block_aabb(eye, dir_x, dir_y, dir_z, other.block_pos, dist) {
      return true;
    }
  }
  false
}

fn draw_box(img: &mut image::RgbImage, min_x: u32, min_y: u32, max_x: u32, max_y: u32, color: image::Rgb<u8>, thickness: u32) {
  let width = img.width();
  let height = img.height();
  for t in 0..thickness {
    let y1 = (min_y + t).min(height - 1);
    let y2 = (max_y.saturating_sub(t)).min(height - 1);
    for x in min_x..=max_x.min(width - 1) {
      img.put_pixel(x, y1, color);
      img.put_pixel(x, y2, color);
    }
    let x1 = (min_x + t).min(width - 1);
    let x2 = (max_x.saturating_sub(t)).min(width - 1);
    for y in min_y..=max_y.min(height - 1) {
      img.put_pixel(x1, y, color);
      img.put_pixel(x2, y, color);
    }
  }
}

fn class_color(class_id: usize) -> image::Rgb<u8> {
  match class_id {
    0 => image::Rgb([0, 255, 0]),     // grass_block: Green
    1 => image::Rgb([0, 200, 255]),   // chest: Cyan
    2 => image::Rgb([255, 60, 60]),   // furnace: Red
    3 => image::Rgb([255, 220, 0]),   // crafting_table: Yellow
    4 => image::Rgb([180, 0, 255]),   // door: Purple
    5 => image::Rgb([255, 140, 0]),   // torch: Orange
    _ => image::Rgb([255, 255, 255]), // White
  }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
  let mut telemetry_path = PathBuf::from(r"F:\pcl\.minecraft\versions\1.21.1-Fabric 0.16.10\auv\telemetry.jsonl");
  let mut session_dir: Option<PathBuf> = Some(PathBuf::from(r"F:\auv\.tmp\m2-session"));
  let mut output_dir = PathBuf::from(r"F:\auv\.tmp\autolabel-dataset");
  let mut max_frames = 0usize;
  let mut export_overlays_count = 10usize;
  let min_distance = 1.0f64;
  let max_distance = 20.0f64;

  let args: Vec<String> = std::env::args().collect();
  let mut i = 1;
  while i < args.len() {
    match args[i].as_str() {
      "--telemetry" => {
        if i + 1 < args.len() {
          telemetry_path = PathBuf::from(&args[i + 1]);
          i += 1;
        }
      }
      "--session-dir" => {
        if i + 1 < args.len() {
          session_dir = Some(PathBuf::from(&args[i + 1]));
          i += 1;
        }
      }
      "--output-dir" => {
        if i + 1 < args.len() {
          output_dir = PathBuf::from(&args[i + 1]);
          i += 1;
        }
      }
      "--max-frames" => {
        if i + 1 < args.len() {
          max_frames = args[i + 1].parse().unwrap_or(0);
          i += 1;
        }
      }
      "--export-overlays" => {
        if i + 1 < args.len() {
          export_overlays_count = args[i + 1].parse().unwrap_or(10);
          i += 1;
        }
      }
      _ => {}
    }
    i += 1;
  }

  println!("=== AUV Minecraft Auto-Labeling Pipeline ===");
  println!("Telemetry source: {}", telemetry_path.display());
  println!("Output directory: {}", output_dir.display());
  if let Some(ref sdir) = session_dir {
    println!("Session directory: {}", sdir.display());
  }

  // Create YOLO dataset directories
  fs::create_dir_all(output_dir.join("images/train"))?;
  fs::create_dir_all(output_dir.join("images/val"))?;
  fs::create_dir_all(output_dir.join("labels/train"))?;
  fs::create_dir_all(output_dir.join("labels/val"))?;
  fs::create_dir_all(output_dir.join("overlays"))?;

  // Load session frames with screenshots if available
  let mut session_screenshots: HashMap<String, (PathBuf, MinecraftSpatialFrame)> = HashMap::new();
  let search_roots = [
    session_dir.clone().unwrap_or_else(|| PathBuf::from(r"F:\auv\.tmp\m2-session")),
    PathBuf::from(r"F:\auv\.tmp\m1-baseline"),
  ];

  for sdir in &search_roots {
    if sdir.is_dir() {
      if let Ok(entries) = fs::read_dir(sdir) {
        for entry in entries.flatten() {
          let view_path = entry.path();
          if view_path.is_dir() {
            let img_path = view_path.join("screenshot.png");
            let telem_path = view_path.join("telemetry.jsonl");
            if img_path.is_file() && telem_path.is_file() {
              if let Ok(content) = fs::read_to_string(&telem_path) {
                if let Some(first_line) = content.lines().next() {
                  if let Ok(frame) = serde_json::from_str::<MinecraftSpatialFrame>(first_line) {
                    session_screenshots.insert(frame.spatial_frame_id.clone(), (img_path.clone(), frame));
                  }
                }
              }
            }
          }
        }
      }
    }
  }
  println!("Loaded {} session views with screenshots.", session_screenshots.len());

  let file = File::open(&telemetry_path)?;
  let reader = BufReader::new(file);

  let start_time = Instant::now();
  let mut total_frames = 0usize;
  let mut total_frames_with_boxes = 0usize;
  let mut total_boxes = 0usize;
  let mut class_distribution: HashMap<String, usize> = HashMap::new();
  for &(name, _) in WHITELIST_CLASSES {
    class_distribution.insert(name.to_string(), 0);
  }

  let mut overlay_images_generated: Vec<String> = Vec::new();

  for (_line_idx, line_res) in reader.lines().enumerate() {
    if max_frames > 0 && total_frames >= max_frames {
      break;
    }
    let line = line_res?;
    if line.trim().is_empty() {
      continue;
    }

    let frame: MinecraftSpatialFrame = match serde_json::from_str(&line) {
      Ok(f) => f,
      Err(_) => continue,
    };

    total_frames += 1;
    let projector = match MinecraftProjector::new(frame.clone()) {
      Ok(p) => p,
      Err(_) => continue,
    };

    let eye = frame.player_pose.eye_position;
    let mut frame_boxes: Vec<ProjectedBBox> = Vec::new();

    for block in &frame.nearby_blocks {
      let Some(class_id) = match_whitelist_class(&block.block_id) else {
        continue;
      };
      let class_name = WHITELIST_CLASSES[class_id].0;

      let center = block.block_pos.center();
      let dx = center.x - eye.x;
      let dy = center.y - eye.y;
      let dz = center.z - eye.z;
      let dist = (dx * dx + dy * dy + dz * dz).sqrt();
      if dist < min_distance || dist > max_distance {
        continue;
      }

      // Raycast occlusion check against closer blocks in nearby_blocks
      if is_occluded_by_closer_block(eye, center, block.block_pos, &frame.nearby_blocks) {
        continue;
      }

      let yolo_bbox = match projector.project_block_yolo_bbox(block.block_pos) {
        Ok(Some(bbox)) => bbox,
        _ => continue,
      };

      let pixel_bbox = match projector.project_block_2d_bbox(block.block_pos) {
        Ok(Some(bbox)) => bbox,
        _ => continue,
      };

      // Filter sub-pixel noise (<0.5% of viewport width/height)
      if yolo_bbox[2] < 0.005 || yolo_bbox[3] < 0.005 {
        continue;
      }

      frame_boxes.push(ProjectedBBox {
        class_id,
        class_name,
        block_pos: block.block_pos,
        distance: dist,
        yolo: yolo_bbox,
        pixel: pixel_bbox,
      });
    }

    if !frame_boxes.is_empty() {
      total_frames_with_boxes += 1;
    }

    for b in &frame_boxes {
      *class_distribution.entry(b.class_name.to_string()).or_default() += 1;
      total_boxes += 1;
    }

    // Determine split: 9:1 train/val split
    let split = if total_frames % 10 == 0 {
      "val"
    } else {
      "train"
    };

    // Format label lines: class_id cx cy w h
    let mut label_content = String::new();
    for b in &frame_boxes {
      label_content.push_str(&format!("{} {:.6} {:.6} {:.6} {:.6}\n", b.class_id, b.yolo[0], b.yolo[1], b.yolo[2], b.yolo[3]));
    }

    let frame_file_name = format!("frame_{:06}", total_frames);
    let label_path = output_dir.join("labels").join(split).join(format!("{}.txt", frame_file_name));
    fs::write(&label_path, label_content)?;

    // Check if this frame has a matching screenshot
    let matching_screenshot = session_screenshots.get(&frame.spatial_frame_id);
    if let Some((src_png, _)) = matching_screenshot {
      let dst_img_path = output_dir.join("images").join(split).join(format!("{}.png", frame_file_name));
      let _ = fs::copy(src_png, &dst_img_path);

      // Generate visual overlay if under count
      if overlay_images_generated.len() < export_overlays_count {
        if let Ok(dynamic_img) = image::open(src_png) {
          let mut rgb_img = dynamic_img.to_rgb8();
          for b in &frame_boxes {
            let min_x = b.pixel[0].max(0.0) as u32;
            let min_y = b.pixel[1].max(0.0) as u32;
            let max_x = b.pixel[2].max(0.0) as u32;
            let max_y = b.pixel[3].max(0.0) as u32;
            let color = class_color(b.class_id);
            draw_box(&mut rgb_img, min_x, min_y, max_x, max_y, color, 2);
          }
          let overlay_path = output_dir.join("overlays").join(format!("{}_overlay.png", frame_file_name));
          if rgb_img.save(&overlay_path).is_ok() {
            overlay_images_generated.push(overlay_path.to_string_lossy().to_string());
          }
        }
      }
    }
  }

  // Also process the session screenshots specifically to ensure we have visual verification overlays
  for (sf_id, (src_png, frame)) in &session_screenshots {
    if overlay_images_generated.len() >= export_overlays_count {
      break;
    }
    let projector = match MinecraftProjector::new(frame.clone()) {
      Ok(p) => p,
      Err(_) => continue,
    };
    let eye = frame.player_pose.eye_position;
    let mut frame_boxes = Vec::new();
    for block in &frame.nearby_blocks {
      let Some(class_id) = match_whitelist_class(&block.block_id) else {
        continue;
      };
      let class_name = WHITELIST_CLASSES[class_id].0;
      let center = block.block_pos.center();
      let dx = center.x - eye.x;
      let dy = center.y - eye.y;
      let dz = center.z - eye.z;
      let dist = (dx * dx + dy * dy + dz * dz).sqrt();
      if dist < min_distance || dist > max_distance {
        continue;
      }
      if is_occluded_by_closer_block(eye, center, block.block_pos, &frame.nearby_blocks) {
        continue;
      }
      let yolo_bbox = match projector.project_block_yolo_bbox(block.block_pos) {
        Ok(Some(bbox)) => bbox,
        _ => continue,
      };
      let pixel_bbox = match projector.project_block_2d_bbox(block.block_pos) {
        Ok(Some(bbox)) => bbox,
        _ => continue,
      };
      if yolo_bbox[2] < 0.005 || yolo_bbox[3] < 0.005 {
        continue;
      }
      frame_boxes.push(ProjectedBBox {
        class_id,
        class_name,
        block_pos: block.block_pos,
        distance: dist,
        yolo: yolo_bbox,
        pixel: pixel_bbox,
      });
    }

    if let Ok(dynamic_img) = image::open(src_png) {
      let mut rgb_img = dynamic_img.to_rgb8();
      for b in &frame_boxes {
        let min_x = b.pixel[0].max(0.0) as u32;
        let min_y = b.pixel[1].max(0.0) as u32;
        let max_x = b.pixel[2].max(0.0) as u32;
        let max_y = b.pixel[3].max(0.0) as u32;
        let color = class_color(b.class_id);
        draw_box(&mut rgb_img, min_x, min_y, max_x, max_y, color, 2);
      }
      let overlay_path = output_dir.join("overlays").join(format!("session_{}_overlay.png", sf_id));
      if rgb_img.save(&overlay_path).is_ok() {
        if !overlay_images_generated.contains(&overlay_path.to_string_lossy().to_string()) {
          overlay_images_generated.push(overlay_path.to_string_lossy().to_string());
        }
      }
    }
  }

  let elapsed = start_time.elapsed();
  let elapsed_ms = elapsed.as_millis();
  let elapsed_secs = elapsed.as_secs_f64();
  let throughput_fps = if elapsed_secs > 0.0 {
    total_frames as f64 / elapsed_secs
  } else {
    0.0
  };
  let throughput_fpm = throughput_fps * 60.0;

  println!("\n=== Auto-Labeling Summary ===");
  println!("Frames processed: {}", total_frames);
  println!("Frames with boxes: {}", total_frames_with_boxes);
  println!("Total boxes generated: {}", total_boxes);
  println!("Elapsed time: {:.3}s ({} ms)", elapsed_secs, elapsed_ms);
  println!("Throughput: {:.2} frames/sec ({:.1} frames/min)", throughput_fps, throughput_fpm);
  println!("\nClass Distribution:");
  for &(name, id) in WHITELIST_CLASSES {
    let count = class_distribution.get(name).copied().unwrap_or(0);
    println!("  [{}] {}: {} boxes", id, name, count);
  }
  println!("Overlay images generated: {}", overlay_images_generated.len());
  for ov in &overlay_images_generated {
    println!("  - {}", ov);
  }

  let mut notes = Vec::new();
  notes.push(format!("Throughput measured at {:.2} fps on {} frames.", throughput_fps, total_frames));
  if class_distribution.get("furnace").copied().unwrap_or(0) == 0 {
    notes.push("furnace count is 0 in wild terrain recording; targeted sampling required for non-natural blocks.".to_string());
  }
  if class_distribution.get("chest").copied().unwrap_or(0) == 0 {
    notes.push("chest count is 0 in wild terrain recording; targeted sampling required for non-natural blocks.".to_string());
  }

  let report = AutolabelReport {
    total_frames_processed: total_frames,
    total_frames_with_boxes,
    total_boxes_generated: total_boxes,
    elapsed_ms,
    throughput_fps,
    throughput_fpm,
    min_distance_m: min_distance,
    max_distance_m: max_distance,
    class_distribution,
    overlay_images_generated,
    notes,
  };

  let report_path = output_dir.join("autolabel_report.json");
  let report_json = serde_json::to_string_pretty(&report)?;
  fs::write(&report_path, report_json)?;
  println!("\nReport written to: {}", report_path.display());

  Ok(())
}
