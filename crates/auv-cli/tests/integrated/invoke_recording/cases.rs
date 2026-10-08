use std::process::Command;

#[test]
fn invoke_dry_run_writes_append_only_trace_records() {
  let store = tempfile::tempdir().expect("temporary tracing store");
  let output = Command::new(env!("CARGO_BIN_EXE_auv"))
    .args([
      "invoke",
      "scan.coverage",
      "--fixture-dir",
      "unused",
      "--dry-run",
      "--json",
      "--store-root",
      store.path().to_str().expect("UTF-8 store path"),
    ])
    .output()
    .expect("run auv invoke");
  assert!(output.status.success(), "invoke failed: {}", String::from_utf8_lossy(&output.stderr));

  let direct: serde_json::Value = serde_json::from_slice(&output.stdout).expect("invoke JSON");
  let run_id = direct["run_id"].as_str().expect("run id");
  let records = std::fs::read_to_string(store.path().join("records.jsonl")).expect("trace records");
  let records =
    records.lines().map(|line| serde_json::from_str::<serde_json::Value>(line).expect("trace record envelope")).collect::<Vec<_>>();

  assert!(!records.is_empty());
  assert!(records.iter().all(|envelope| envelope["version"] == 1));
  assert!(records.iter().all(|envelope| envelope["record"]["run_id"] == run_id));
  assert!(records.iter().any(|envelope| envelope["record"]["type"] == "event"));
}

// ROOT CAUSE:
// Runner scrollUntil discarded its final streamed CaptureRef, so the invoke
// result lacked the screenshot and clients had to capture a different frame.
// This opt-in regression checks the real public CLI with separate store roots.
#[cfg(target_os = "macos")]
#[test]
#[ignore = "requires the synthetic repeated-search fixture at the top and a private local daemon; set AUV_EVIDENCE_TEST_ENDPOINT and AUV_EVIDENCE_TEST_DEVICE_ID"]
fn runner_scroll_until_returns_its_recorded_final_frame() {
  let endpoint = std::env::var("AUV_EVIDENCE_TEST_ENDPOINT").expect("private test daemon endpoint");
  let device_id = std::env::var("AUV_EVIDENCE_TEST_DEVICE_ID").expect("observed local test Device");
  let store = tempfile::tempdir().unwrap();
  let output = Command::new(env!("CARGO_BIN_EXE_auv"))
    .env("AUV_ENDPOINT", endpoint)
    .args([
      "--device-id",
      &device_id,
      "invoke",
      "input.scrollUntil",
      "0.5",
      "0.6",
      "--normalized",
      "--dy",
      "350",
      "--until",
      "text:Birch transfer",
      "--max-steps",
      "20",
      "--settle-ms",
      "50",
      "--target",
      "app:local.auv.RepeatedSearchFixture",
      "--title",
      "AUV Repeated Search - Synthetic Benchmark",
      "--input-policy",
      "background-only",
      "--no-overlay",
      "--compact-json",
      "--store-root",
      store.path().to_str().unwrap(),
    ])
    .output()
    .unwrap();
  assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
  let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
  assert_eq!(result["result"]["result"]["reason"], "text_visible");
  assert!(result["result"]["result"]["text_match"]["text"].as_str().unwrap().contains("Birch transfer"));
  let artifacts = result["artifacts"].as_array().unwrap();
  assert_eq!(artifacts.len(), 1, "Runner must return its exact final observation receipt");
  assert_eq!(artifacts[0]["purpose"], "auv.scan.scroll_until_final_capture");
  let path = std::path::Path::new(artifacts[0]["file_path"].as_str().unwrap());
  let image = image::load_from_memory(&std::fs::read(path).unwrap()).unwrap();
  assert_eq!((image.width(), image.height()), (900, 682));
  let client_path = store.path().join("artifacts").join(result["run_id"].as_str().unwrap()).join(path.file_name().unwrap());
  assert!(!client_path.exists(), "pixels must be persisted in the Runner store, not downloaded into the frontend");
}
