//! Foreground, operator-audited action sequence over one AUV Run/Runner.

use std::{
  fs,
  future::Future,
  io::Read as _,
  path::{Path, PathBuf},
  time::Duration,
};

use auv::{
  AuvContext, Client,
  client::{RunOptions, RunSelection, RunnerOptions},
  profile::ProfileStore,
  runs::RunOutcome,
};
use auv_driver::InputActionResult;
use image::{DynamicImage, ImageFormat};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;

use crate::{Action, ActionExecutor, ControlSignal, parse_action};

const MAX_FINAL_SETTLE_MS: u64 = 5_000;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
  version: u8,
  context: Context,
  actions: Vec<Value>,
  #[serde(default)]
  final_settle_ms: u64,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum Context {
  Paired {
    device_id: String,
    config_profile: String,
    profiles_file: PathBuf,
  },
  GuestLocal {
    device_id: String,
    daemon_endpoint: String,
  },
}

struct ValidatedPlan {
  context: Context,
  actions: Vec<Action>,
  final_settle_ms: u64,
}

impl Plan {
  fn validate(self) -> Result<ValidatedPlan, String> {
    if self.version != 1 {
      return Err("action plan version must be 1".into());
    }
    if self.actions.is_empty() || self.actions.len() > 1000 {
      return Err("action plan needs 1..=1000 predeclared actions".into());
    }
    if self.final_settle_ms > MAX_FINAL_SETTLE_MS {
      return Err(format!("final_settle_ms must be 0..={MAX_FINAL_SETTLE_MS}"));
    }
    match &self.context {
      Context::Paired {
        device_id,
        config_profile,
        profiles_file,
      } => {
        if device_id.is_empty() || config_profile.is_empty() || !profiles_file.is_absolute() || !profiles_file.is_file() {
          return Err("paired context needs a Device ID, profile, and existing absolute profile file".into());
        }
      }
      Context::GuestLocal {
        device_id,
        daemon_endpoint,
      } => {
        if device_id.is_empty() || !daemon_endpoint.starts_with("unix:///") {
          return Err("guest-local context needs a Device ID and explicit Unix daemon endpoint".into());
        }
      }
    }
    let actions = self.actions.iter().map(parse_action).collect::<Result<Vec<_>, _>>().map_err(|error| error.to_string())?;
    Ok(ValidatedPlan {
      context: self.context,
      actions,
      final_settle_ms: self.final_settle_ms,
    })
  }
}

fn read_plan(path: &Path) -> Result<ValidatedPlan, String> {
  if !path.is_absolute() {
    return Err("action plan path must be absolute".into());
  }
  let bytes = fs::read(path).map_err(|error| format!("cannot read action plan: {error}"))?;
  let plan: Plan = serde_json::from_slice(&bytes).map_err(|error| format!("invalid action plan schema: {error}"))?;
  plan.validate()
}

fn episode_paths() -> Result<(PathBuf, PathBuf), String> {
  let dir = PathBuf::from(std::env::var_os("AUV_OSWORLD_EPISODE_DIR").ok_or("AUV_OSWORLD_EPISODE_DIR is required")?);
  let sidecar = PathBuf::from(std::env::var_os("AUV_OSWORLD_ACTION_EVIDENCE").ok_or("AUV_OSWORLD_ACTION_EVIDENCE is required")?);
  validate_episode_paths(dir, sidecar)
}

fn validate_episode_paths(dir: PathBuf, sidecar: PathBuf) -> Result<(PathBuf, PathBuf), String> {
  if !dir.is_absolute() {
    return Err("episode directory must be absolute".into());
  }
  let dir = dir.canonicalize().map_err(|error| format!("episode directory is unavailable: {error}"))?;
  if !dir.is_dir() {
    return Err("episode path is not a directory".into());
  }
  if sidecar != dir.join("action_evidence.json") {
    return Err("action evidence must be the episode's action_evidence.json".into());
  }
  Ok((dir, sidecar))
}

fn atomic_json(path: &Path, value: &Value) -> Result<(), String> {
  let mut temp = NamedTempFile::new_in(path.parent().ok_or("output has no parent directory")?).map_err(|error| error.to_string())?;
  serde_json::to_writer(&mut temp, value).map_err(|error| error.to_string())?;
  temp.as_file().sync_all().map_err(|error| error.to_string())?;
  temp.persist(path).map_err(|error| error.to_string())?;
  Ok(())
}

fn evidence(run_id: &str, artifact: Option<Value>) -> Value {
  json!({"run_ids": [run_id], "final_artifact": artifact})
}

fn emit_terminal(mut output: impl std::io::Write, sidecar: &Path, value: &Value) -> Result<(), String> {
  atomic_json(sidecar, value)?;
  serde_json::to_writer(&mut output, value).map_err(|error| error.to_string())?;
  output.write_all(b"\n").map_err(|error| error.to_string())?;
  output.flush().map_err(|error| error.to_string())
}

fn save_capture(dir: &Path, capture: auv_driver::DisplayCapture) -> Result<Value, String> {
  let mut temp = NamedTempFile::new_in(dir).map_err(|error| error.to_string())?;
  DynamicImage::ImageRgba8(capture.capture.image).write_to(&mut temp, ImageFormat::Png).map_err(|error| error.to_string())?;
  temp.as_file().sync_all().map_err(|error| error.to_string())?;
  let mut bytes = Vec::new();
  temp.reopen().map_err(|error| error.to_string())?.read_to_end(&mut bytes).map_err(|error| error.to_string())?;
  let digest = format!("{:x}", Sha256::digest(&bytes));
  temp.persist(dir.join("final-screenshot.png")).map_err(|error| error.to_string())?;
  Ok(json!({"path": "final-screenshot.png", "sha256": digest}))
}

async fn connect(context: &Context) -> Result<(Client, String), String> {
  match context {
    Context::Paired {
      device_id,
      config_profile,
      profiles_file,
    } => {
      let context = AuvContext {
        device_id: Some(device_id.clone()),
        config_profile: Some(config_profile.clone()),
        ..Default::default()
      };
      let profiles = ProfileStore::from_path(profiles_file);
      let client = Client::from_context_with_profiles(context, &profiles).await.map_err(|error| error.to_string())?;
      Ok((client, device_id.clone()))
    }
    Context::GuestLocal {
      device_id,
      daemon_endpoint,
    } => {
      let context = AuvContext {
        device_id: Some(device_id.clone()),
        daemon_endpoint: Some(daemon_endpoint.clone()),
        ..Default::default()
      };
      let client = Client::from_context(context).await.map_err(|error| error.to_string())?.local().map_err(|error| error.to_string())?;
      Ok((client, device_id.clone()))
    }
  }
}

async fn interrupted() {
  #[cfg(unix)]
  {
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("install SIGTERM handler");
    tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = terminate.recv() => {} }
  }
  #[cfg(not(unix))]
  {
    let _ = tokio::signal::ctrl_c().await;
  }
}

async fn settle_before_capture(final_settle_ms: u64, cancel: impl Future<Output = ()>) -> bool {
  if final_settle_ms == 0 {
    return true;
  }
  tokio::select! {
    biased;
    _ = cancel => false,
    _ = tokio::time::sleep(Duration::from_millis(final_settle_ms)) => true,
  }
}

/// Execute a predeclared sequence, then print the final sidecar JSON on the
/// final stdout line. Errors remain process failures, not benchmark scores.
// TODO(osworld-action-entry-live): Runner finish and cancellation release are
// covered only by the ignored isolated-Xorg gate; run it before claiming live
// GUI-action baseline capability or wiring this entry into the K8s adapter.
// TODO(osworld-adaptive-observation): Intermediate screenshot/branching is
// deferred: this fixed plan only adds a bounded final settle. Revisit after
// an owner-approved observation contract and fresh UI-state evidence.
pub async fn run(plan_path: &Path) -> Result<(), String> {
  let plan = read_plan(plan_path)?;
  let (dir, sidecar) = episode_paths()?;
  let (client, device_id) = connect(&plan.context).await?;
  let mut run_options = RunOptions {
    selection: RunSelection::New,
    ..Default::default()
  };
  run_options.device = auv::resource::DeviceSelector::by_id(&device_id);
  let runner = client.runner_with(run_options, RunnerOptions::default()).await.map_err(|error| error.to_string())?;
  let run_id = runner.run().id.to_string();
  let mut executor = ActionExecutor::new(runner);
  // Persist the observed Run before any GUI delivery. A later timeout may
  // prevent terminal stdout, but the runner can still retain this ID.
  if let Err(error) = atomic_json(&sidecar, &evidence(&run_id, None)) {
    let cleanup = executor.finish(RunOutcome::Canceled).await;
    return Err(match cleanup {
      Ok(_) => error,
      Err(cleanup) => format!("{error}; cleanup: {cleanup}"),
    });
  }

  let mut deliveries: Vec<Vec<InputActionResult>> = Vec::new();
  let mut outcome = RunOutcome::Succeeded;
  let mut error = None;
  let cancel = interrupted();
  tokio::pin!(cancel);
  for action in plan.actions {
    let result = tokio::select! {
      result = executor.execute(action) => Some(result),
      _ = &mut cancel => None,
    };
    match result {
      None => {
        outcome = RunOutcome::Canceled;
        error = Some("action sequence interrupted".to_string());
        break;
      }
      Some(Err(failure)) => {
        outcome = RunOutcome::Failed;
        error = Some(failure.to_string());
        break;
      }
      Some(Ok(step)) => {
        deliveries.push(step.delivery);
        if let Err(failure) = atomic_json(&dir.join("input-action-results.json"), &json!(deliveries)) {
          outcome = RunOutcome::Failed;
          error = Some(format!("input evidence persistence failed: {failure}"));
          break;
        }
        match step.control {
          Some(ControlSignal::Fail) => {
            outcome = RunOutcome::Failed;
            error = Some("plan declared FAIL".into());
            break;
          }
          Some(ControlSignal::Done) => break,
          _ => {}
        }
      }
    }
  }

  if outcome == RunOutcome::Succeeded && !settle_before_capture(plan.final_settle_ms, &mut cancel).await {
    outcome = RunOutcome::Canceled;
    error = Some("final settle interrupted".to_string());
  }

  let mut final_artifact = None;
  if outcome == RunOutcome::Succeeded {
    let captured = tokio::select! {
      result = executor.capture_final() => Some(result),
      _ = &mut cancel => None,
    };
    match captured {
      None => {
        outcome = RunOutcome::Canceled;
        error = Some("final capture interrupted".into());
      }
      Some(Err(failure)) => {
        outcome = RunOutcome::Failed;
        error = Some(failure.to_string());
      }
      Some(Ok(capture)) => match save_capture(&dir, capture) {
        Ok(artifact) => {
          final_artifact = Some(artifact);
          if let Err(failure) = atomic_json(&sidecar, &evidence(&run_id, final_artifact.clone())) {
            outcome = RunOutcome::Failed;
            error = Some(format!("final sidecar persistence failed: {failure}"));
          }
        }
        Err(failure) => {
          outcome = RunOutcome::Failed;
          error = Some(format!("final PNG persistence failed: {failure}"));
        }
      },
    }
  }
  if let Err(failure) = executor.finish(outcome).await {
    error = Some(match error {
      Some(primary) => format!("{primary}; finish: {failure}"),
      None => format!("finish: {failure}"),
    });
  }
  let terminal = evidence(&run_id, final_artifact);
  // Re-persist after finish so terminal stdout always mirrors durable sidecar.
  emit_terminal(std::io::stdout().lock(), &sidecar, &terminal)?;
  match error {
    Some(error) => Err(error),
    None => Ok(()),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn rejects_shell_and_extra_schema_fields_before_connection() {
    let extra = serde_json::from_value::<Plan>(
      json!({"version":1,"context":{"kind":"guest-local","device_id":"device","daemon_endpoint":"unix:///tmp/auv.sock"},"actions":["WAIT"],"command":"xdotool click 1"}),
    );
    assert!(extra.is_err());
    let execute: Plan = serde_json::from_value(json!({"version":1,"context":{"kind":"guest-local","device_id":"device","daemon_endpoint":"unix:///tmp/auv.sock"},"actions":[{"action_type":"EXECUTE","command":"xdotool click 1"}]})).unwrap();
    assert!(execute.validate().is_err());
    let python: Plan = serde_json::from_value(json!({"version":1,"context":{"kind":"guest-local","device_id":"device","daemon_endpoint":"unix:///tmp/auv.sock"},"actions":["pyautogui.click(1, 2)"]})).unwrap();
    assert!(python.validate().is_err());
    let context_extra = serde_json::from_value::<Plan>(
      json!({"version":1,"context":{"kind":"guest-local","device_id":"device","daemon_endpoint":"unix:///tmp/auv.sock","script":"x.py"},"actions":["WAIT"]}),
    );
    assert!(context_extra.is_err());
    let wrong_version: Plan = serde_json::from_value(
      json!({"version":2,"context":{"kind":"guest-local","device_id":"device","daemon_endpoint":"unix:///tmp/auv.sock"},"actions":["WAIT"]}),
    )
    .unwrap();
    assert!(wrong_version.validate().is_err());
  }

  #[test]
  fn sidecar_is_atomically_replaced_with_existing_run_id() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("action_evidence.json");
    atomic_json(&path, &evidence("run-1", None)).unwrap();
    atomic_json(&path, &evidence("run-1", Some(json!({"path":"final-screenshot.png","sha256":"hash"})))).unwrap();
    assert_eq!(
      serde_json::from_slice::<Value>(&fs::read(&path).unwrap()).unwrap(),
      evidence("run-1", Some(json!({"path":"final-screenshot.png","sha256":"hash"})))
    );
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
  }

  #[test]
  fn rejects_sidecar_outside_canonical_episode_and_relative_plan() {
    let dir = tempfile::tempdir().unwrap();
    assert!(validate_episode_paths(dir.path().to_path_buf(), dir.path().join("../other.json")).is_err());
    assert!(read_plan(Path::new("relative-plan.json")).is_err());
  }

  #[test]
  fn paired_plan_requires_explicit_profile_file_and_parses_typed_actions() {
    let dir = tempfile::tempdir().unwrap();
    let profiles = dir.path().join("profiles.json");
    fs::write(&profiles, b"{}").unwrap();
    let plan: Plan = serde_json::from_value(json!({
      "version": 1,
      "context": {"kind": "paired", "device_id": "device", "config_profile": "episode", "profiles_file": profiles},
      "actions": [{"action_type": "CLICK", "x": 100, "y": 200}, "DONE"],
    }))
    .unwrap();
    assert_eq!(plan.validate().unwrap().actions.len(), 2);
  }

  #[test]
  fn final_settle_is_optional_bounded_integer_in_strict_plan() {
    let base =
      json!({"version":1,"context":{"kind":"guest-local","device_id":"device","daemon_endpoint":"unix:///tmp/auv.sock"},"actions":["DONE"]});
    let default: Plan = serde_json::from_value(base.clone()).unwrap();
    assert_eq!(default.validate().unwrap().final_settle_ms, 0);
    let mut at_limit = base.clone();
    at_limit["final_settle_ms"] = json!(5000);
    let at_limit: Plan = serde_json::from_value(at_limit).unwrap();
    assert_eq!(at_limit.validate().unwrap().final_settle_ms, 5000);
    let mut over_limit = base.clone();
    over_limit["final_settle_ms"] = json!(5001);
    let over_limit: Plan = serde_json::from_value(over_limit).unwrap();
    assert!(over_limit.validate().is_err());
    for invalid in [json!(-1), json!(1.5), json!(true), json!("100")] {
      let mut wrong_type = base.clone();
      wrong_type["final_settle_ms"] = invalid;
      assert!(serde_json::from_value::<Plan>(wrong_type).is_err());
    }
  }

  #[tokio::test]
  async fn interrupt_during_final_settle_prevents_capture() {
    let began = std::time::Instant::now();
    let settled = settle_before_capture(5_000, tokio::time::sleep(std::time::Duration::from_millis(10))).await;
    assert!(!settled);
    assert!(began.elapsed() < std::time::Duration::from_secs(1));
  }

  #[test]
  fn final_png_is_inside_episode_and_digest_matches_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let image = image::RgbaImage::from_pixel(2, 2, image::Rgba([1, 2, 3, 255]));
    let capture = auv_driver::DisplayCapture {
      display: auv_driver::Display {
        id: "display".into(),
        name: None,
        frame: auv_driver::Rect::new(0.0, 0.0, 2.0, 2.0),
        coordinate_space: auv_driver::CoordinateSpace::Screen,
        scale_factor: 1.0,
        is_primary: true,
        is_builtin: None,
      },
      capture: auv_driver::Capture {
        origin: None,
        image,
        bounds: auv_driver::Rect::new(0.0, 0.0, 2.0, 2.0),
        scale_factor: 1.0,
        backend: "test".into(),
        fallback_reason: None,
      },
    };
    let artifact = save_capture(dir.path(), capture).unwrap();
    assert_eq!(artifact["path"], "final-screenshot.png");
    let bytes = fs::read(dir.path().join("final-screenshot.png")).unwrap();
    assert_eq!(artifact["sha256"], format!("{:x}", Sha256::digest(&bytes)));
    assert_eq!(image::load_from_memory_with_format(&bytes, ImageFormat::Png).unwrap().width(), 2);
  }

  #[test]
  fn final_stdout_line_matches_durable_sidecar_on_failed_capture() {
    let dir = tempfile::tempdir().unwrap();
    let sidecar = dir.path().join("action_evidence.json");
    let mut stdout = Vec::new();
    let failed_capture = evidence("run-1", None);
    emit_terminal(&mut stdout, &sidecar, &failed_capture).unwrap();
    let line = stdout.split(|byte| *byte == b'\n').rfind(|line| !line.is_empty()).unwrap();
    assert_eq!(serde_json::from_slice::<Value>(line).unwrap(), serde_json::from_slice::<Value>(&fs::read(sidecar).unwrap()).unwrap());
    assert!(failed_capture["final_artifact"].is_null());
    assert!(failed_capture.get("score").is_none());
  }
}
