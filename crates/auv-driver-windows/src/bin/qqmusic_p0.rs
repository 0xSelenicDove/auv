//! P0 primitive CLI tool for QQ Music background control.
//!
//! Provides CLI subcommands for:
//! - query: inspect current SMTC metadata and playback status
//! - play: trigger playback
//! - pause: pause playback
//! - volume <level>: set process volume (0.0 .. 1.0) via CoreAudio
//! - next: skip to next track and return new metadata
//! - capture: WGC capture of QQ Music window and verify non-black pixels
#![cfg(target_os = "windows")]

use auv_driver_common::error::{DriverError, DriverResult};
use auv_driver_windows::desktop::ensure_input_desktop;
use auv_driver_windows::media::{AudioVolumeController, SmtcMediaManager};
use auv_driver_windows::wgc::capture_window_wgc;
use auv_driver_windows::window::list_windows;
use serde::{Deserialize, Serialize};
use std::env;
use std::time::Duration;

#[derive(Serialize, Deserialize, Debug)]
struct QueryResult {
  app_id: String,
  title: String,
  artist: String,
  status: String,
  volume: Option<f32>,
}

#[derive(Serialize, Deserialize, Debug)]
struct ActionResult {
  success: bool,
  op: String,
  details: serde_json::Value,
}

#[derive(Serialize, Deserialize, Debug)]
struct CaptureResult {
  width: u32,
  height: u32,
  total_pixels: usize,
  non_black_pixels: usize,
  non_black_ratio: f64,
  alive: bool,
}

fn get_qq_session() -> DriverResult<(auv_driver_windows::media::SmtcSession, u32)> {
  let manager = SmtcMediaManager::new()?;
  let session = manager.find_session("qqmusic")?.ok_or_else(|| DriverError::NotFound {
    target: "QQ Music SMTC session".to_string(),
  })?;

  // Get PID
  let mut pid = 0;
  if let Ok(windows) = list_windows() {
    for w in windows {
      if w.app_name.as_deref() == Some("QQMusic.exe") {
        if let Some(p) = w.process_id {
          pid = p;
          break;
        }
      }
    }
  }

  Ok((session, pid))
}

fn main() {
  ensure_input_desktop();
  let args: Vec<String> = env::args().collect();
  if args.len() < 2 {
    eprintln!("Usage: qqmusic_p0 <query|play|pause|volume <0.0-1.0>|next|capture>");
    std::process::exit(1);
  }

  let cmd = args[1].as_str();
  match cmd {
    "query" => {
      let (session, pid) = get_qq_session().expect("Failed to get QQ Music session");
      let meta = session.track_metadata().expect("Failed to query track metadata");
      let status = session.playback_status().expect("Failed to query playback status");
      let volume = if pid > 0 {
        AudioVolumeController::get_process_volume(pid).ok()
      } else {
        None
      };

      let res = QueryResult {
        app_id: session.app_id().to_string(),
        title: meta.title,
        artist: meta.artist,
        status: format!("{:?}", status),
        volume,
      };
      println!("{}", serde_json::to_string_pretty(&res).unwrap());
    }
    "play" => {
      let (session, _) = get_qq_session().expect("Failed to get QQ Music session");
      let ok = session.play().expect("Play failed");
      std::thread::sleep(Duration::from_millis(200));
      let status = session.playback_status().expect("Query status failed");
      let res = ActionResult {
        success: ok,
        op: "play".to_string(),
        details: serde_json::json!({ "status": format!("{:?}", status) }),
      };
      println!("{}", serde_json::to_string_pretty(&res).unwrap());
    }
    "pause" => {
      let (session, _) = get_qq_session().expect("Failed to get QQ Music session");
      let ok = session.pause().expect("Pause failed");
      std::thread::sleep(Duration::from_millis(200));
      let status = session.playback_status().expect("Query status failed");
      let res = ActionResult {
        success: ok,
        op: "pause".to_string(),
        details: serde_json::json!({ "status": format!("{:?}", status) }),
      };
      println!("{}", serde_json::to_string_pretty(&res).unwrap());
    }
    "volume" => {
      let level: f32 = args.get(2).expect("Volume level required").parse().expect("Invalid float");
      let (_, pid) = get_qq_session().expect("Failed to get QQ Music session");
      assert!(pid > 0, "QQMusic.exe process PID not found");
      AudioVolumeController::set_process_volume(pid, level).expect("Failed to set process volume");
      let readback = AudioVolumeController::get_process_volume(pid).expect("Failed to readback volume");
      let res = ActionResult {
        success: (readback - level).abs() < 0.05,
        op: "volume".to_string(),
        details: serde_json::json!({ "target": level, "readback": readback, "pid": pid }),
      };
      println!("{}", serde_json::to_string_pretty(&res).unwrap());
    }
    "next" => {
      let (session, _) = get_qq_session().expect("Failed to get QQ Music session");
      let before_meta = session.track_metadata().expect("Failed to query before metadata");
      let ok = session.skip_next().expect("SkipNext failed");

      // Wait for title update up to 1.5s
      let mut after_meta = before_meta.clone();
      for _ in 0..15 {
        std::thread::sleep(Duration::from_millis(100));
        if let Ok(meta) = session.track_metadata() {
          if meta.title != before_meta.title {
            after_meta = meta;
            break;
          }
        }
      }

      let res = ActionResult {
        success: ok,
        op: "next".to_string(),
        details: serde_json::json!({
          "previous_title": before_meta.title,
          "new_title": after_meta.title,
          "title_changed": after_meta.title != before_meta.title
        }),
      };
      println!("{}", serde_json::to_string_pretty(&res).unwrap());
    }
    "capture" => {
      let windows = list_windows().expect("Failed to list windows");
      let qq_win =
        windows.into_iter().find(|w| w.app_name.as_deref() == Some("QQMusic.exe")).expect("QQMusic window not found in list_windows");

      let cap = capture_window_wgc(&qq_win).expect("WGC capture failed");
      let total = (cap.image.width() * cap.image.height()) as usize;
      let raw = cap.image.as_raw();
      let mut non_black = 0usize;
      for chunk in raw.chunks_exact(4) {
        if chunk[0] > 10 || chunk[1] > 10 || chunk[2] > 10 {
          non_black += 1;
        }
      }
      let ratio = if total > 0 {
        non_black as f64 / total as f64 * 100.0
      } else {
        0.0
      };
      let alive = ratio > 50.0;

      let res = CaptureResult {
        width: cap.image.width(),
        height: cap.image.height(),
        total_pixels: total,
        non_black_pixels: non_black,
        non_black_ratio: ratio,
        alive,
      };
      println!("{}", serde_json::to_string_pretty(&res).unwrap());
    }
    other => {
      eprintln!("Unknown command: {other}");
      std::process::exit(1);
    }
  }
}
