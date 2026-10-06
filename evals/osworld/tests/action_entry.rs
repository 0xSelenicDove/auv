//! Ignored live gate for the foreground binary, not a local capability claim.

use std::{
  fs,
  process::{Command, Stdio},
  thread,
  time::{Duration, Instant},
};

use serde_json::{Value, json};

#[test]
fn binary_rejects_unreviewed_gui_relay_without_creating_evidence() {
  let root = tempfile::tempdir().unwrap();
  let plan_path = root.path().join("plan.json");
  fs::write(
    &plan_path,
    serde_json::to_vec(&json!({
      "version": 1,
      "context": {"kind": "guest-local", "device_id": "device", "daemon_endpoint": "unix:///tmp/auv.sock"},
      "actions": [{"action_type": "EXECUTE", "command": "xdotool click 1"}],
    }))
    .unwrap(),
  )
  .unwrap();
  let sidecar = root.path().join("action_evidence.json");
  let result = Command::new(env!("CARGO_BIN_EXE_auv-osworld-action"))
    .args(["--plan", plan_path.to_str().unwrap()])
    .env("AUV_OSWORLD_EPISODE_DIR", root.path())
    .env("AUV_OSWORLD_ACTION_EVIDENCE", &sidecar)
    .output()
    .unwrap();
  assert!(!result.status.success());
  assert!(result.stdout.is_empty());
  assert!(!sidecar.exists());
}

fn plan(path: &std::path::Path, endpoint: &str, actions: Vec<Value>) {
  fs::write(path, serde_json::to_vec(&json!({
    "version": 1,
    "context": {"kind": "guest-local", "device_id": std::env::var("AUV_OSWORLD_TEST_DEVICE_ID").expect("set canonical Device ID"), "daemon_endpoint": endpoint},
    "actions": actions,
  })).unwrap()).unwrap();
}

/// Requires an isolated Xorg guest with a running local AUV daemon. The
/// second Run proves that cancellation released the first Run's mouse hold.
#[test]
#[ignore = "requires isolated Xorg guest and AUV_OSWORLD_TEST_DAEMON/DEVICE_ID"]
fn foreground_runner_cancellation_preserves_run_and_releases_input() {
  let endpoint = std::env::var("AUV_OSWORLD_TEST_DAEMON").expect("set Unix AUV endpoint");
  assert!(endpoint.starts_with("unix:///"));
  let root = tempfile::tempdir().unwrap();
  let first = root.path().join("first");
  fs::create_dir(&first).unwrap();
  let first_plan = root.path().join("first-plan.json");
  let mut actions = vec![json!({"action_type":"MOUSE_DOWN"})];
  actions.extend((0..1000).map(|_| json!({"action_type":"MOVE_TO","x":300,"y":300})));
  plan(&first_plan, &endpoint, actions);
  let sidecar = first.join("action_evidence.json");
  let child = Command::new(env!("CARGO_BIN_EXE_auv-osworld-action"))
    .args(["--plan", first_plan.to_str().unwrap()])
    .env("AUV_OSWORLD_EPISODE_DIR", &first)
    .env("AUV_OSWORLD_ACTION_EVIDENCE", &sidecar)
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .unwrap();
  let deadline = Instant::now() + Duration::from_secs(10);
  while !first.join("input-action-results.json").exists() {
    assert!(Instant::now() < deadline, "first typed delivery not observed");
    thread::sleep(Duration::from_millis(20));
  }
  let status = Command::new("kill").args(["-TERM", &child.id().to_string()]).status().unwrap();
  assert!(status.success());
  let output = child.wait_with_output().unwrap();
  assert!(!output.status.success());
  let terminal: Value = serde_json::from_slice(output.stdout.split(|byte| *byte == b'\n').rfind(|line| !line.is_empty()).unwrap()).unwrap();
  assert_eq!(terminal, serde_json::from_slice::<Value>(&fs::read(&sidecar).unwrap()).unwrap());
  assert!(terminal["run_ids"][0].as_str().is_some());
  assert!(terminal["final_artifact"].is_null());

  let second = root.path().join("second");
  fs::create_dir(&second).unwrap();
  let second_plan = root.path().join("second-plan.json");
  plan(
    &second_plan,
    &endpoint,
    vec![
      json!({"action_type":"MOUSE_DOWN"}),
      json!({"action_type":"MOUSE_UP"}),
    ],
  );
  let result = Command::new(env!("CARGO_BIN_EXE_auv-osworld-action"))
    .args(["--plan", second_plan.to_str().unwrap()])
    .env("AUV_OSWORLD_EPISODE_DIR", &second)
    .env("AUV_OSWORLD_ACTION_EVIDENCE", second.join("action_evidence.json"))
    .output()
    .unwrap();
  assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
  let evidence: Value = serde_json::from_slice(&fs::read(second.join("action_evidence.json")).unwrap()).unwrap();
  assert!(evidence["final_artifact"].is_object());
}

/// Requires the same isolated Xorg/AUV fixture as the hold-release gate.
/// The completed DONE step gives a deterministic point inside final settle.
#[test]
#[ignore = "requires isolated Xorg guest and AUV_OSWORLD_TEST_DAEMON/DEVICE_ID"]
fn interrupt_during_final_settle_keeps_run_evidence_without_success_png() {
  let endpoint = std::env::var("AUV_OSWORLD_TEST_DAEMON").expect("set Unix AUV endpoint");
  let root = tempfile::tempdir().unwrap();
  let episode = root.path().join("episode");
  fs::create_dir(&episode).unwrap();
  let plan_path = root.path().join("settle-plan.json");
  fs::write(
    &plan_path,
    serde_json::to_vec(&json!({
      "version": 1,
      "context": {"kind": "guest-local", "device_id": std::env::var("AUV_OSWORLD_TEST_DEVICE_ID").expect("set canonical Device ID"), "daemon_endpoint": endpoint},
      "actions": ["DONE"],
      "final_settle_ms": 5000,
    }))
    .unwrap(),
  )
  .unwrap();
  let sidecar = episode.join("action_evidence.json");
  let child = Command::new(env!("CARGO_BIN_EXE_auv-osworld-action"))
    .args(["--plan", plan_path.to_str().unwrap()])
    .env("AUV_OSWORLD_EPISODE_DIR", &episode)
    .env("AUV_OSWORLD_ACTION_EVIDENCE", &sidecar)
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .unwrap();
  let deadline = Instant::now() + Duration::from_secs(10);
  while !episode.join("input-action-results.json").exists() {
    assert!(Instant::now() < deadline, "DONE evidence not observed");
    thread::sleep(Duration::from_millis(20));
  }
  assert!(!episode.join("final-screenshot.png").exists());
  assert!(Command::new("kill").args(["-TERM", &child.id().to_string()]).status().unwrap().success());
  let output = child.wait_with_output().unwrap();
  assert!(!output.status.success());
  assert!(String::from_utf8_lossy(&output.stderr).contains("final settle interrupted"));
  let terminal: Value = serde_json::from_slice(output.stdout.split(|byte| *byte == b'\n').rfind(|line| !line.is_empty()).unwrap()).unwrap();
  assert_eq!(terminal, serde_json::from_slice::<Value>(&fs::read(&sidecar).unwrap()).unwrap());
  assert!(terminal["run_ids"][0].as_str().is_some());
  assert!(terminal["final_artifact"].is_null());
  assert!(!episode.join("final-screenshot.png").exists());
}
