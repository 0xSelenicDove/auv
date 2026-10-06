use std::path::PathBuf;
use std::str::FromStr;

use auv_tracing::{ArtifactId, ArtifactMetadata, ArtifactPurpose, ArtifactUri, Attributes, ByteLength, ContentType, RunId, Sha256Digest};

use super::*;
use crate::{InvokeCommandOutput, default_registry};

#[test]
fn primary_artifact_is_directly_openable_in_human_and_json_output() {
  let registry = default_registry();
  let command = registry.resolve("display.capture").expect("capture command");
  let run_id = RunId::from_str("019f8b1e-4b2d-7a00-8f00-0000000000ab").expect("run id");
  let uri = ArtifactUri::from_ids(run_id, ArtifactId::new());
  let metadata = ArtifactMetadata::new(
    uri.clone(),
    ArtifactPurpose::new("auv.driver.display_capture"),
    ContentType::new("image/png"),
    Some("png".to_string()),
    ByteLength::new(3).expect("length"),
    Sha256Digest::new([7; 32]),
    Attributes::empty(),
  );
  let file_path = PathBuf::from("/tmp/auv-artifacts/capture.png");
  let output = InvokeCommandOutput::from_result(&serde_json::json!({ "captured": true })).expect("result").with_artifacts([metadata]);
  let result = InvokeResult::from_command_result(run_id, command, Ok(output)).with_artifact_paths([(uri, file_path.clone())]);

  let human = result.render_to_string(InvokeOutputOptions::default()).expect("human output");
  assert!(human.contains("Artifacts"));
  assert!(human.contains("auv.driver.display_capture"));
  assert!(human.contains(file_path.to_str().expect("UTF-8 fixture path")));
  assert!(human.contains("auv://runs/"));

  let json = result
    .render_to_string(InvokeOutputOptions {
      json: true,
      ..InvokeOutputOptions::default()
    })
    .expect("JSON output");
  let value: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
  assert_eq!(value["artifacts"][0]["purpose"], "auv.driver.display_capture");
  assert_eq!(value["artifacts"][0]["content_type"], "image/png");
  assert_eq!(value["artifacts"][0]["file_extension"], "png");
  assert_eq!(value["artifacts"][0]["file_path"], file_path.to_str().expect("UTF-8 fixture path"));
}

#[test]
fn typed_failure_keeps_legacy_message_and_machine_readable_reason() {
  let registry = default_registry();
  let command = registry.resolve("input.key").unwrap();
  let failure = crate::InvokeFailure::from(auv_driver::DriverError::NotFound {
    target: "window:missing".into(),
  });
  let output = InvokeResult::from_command_result(RunId::new(), command, Err(failure));
  let json: serde_json::Value = serde_json::from_str(
    &output
      .render_to_string(InvokeOutputOptions {
        json: true,
        ..Default::default()
      })
      .unwrap(),
  )
  .unwrap();
  assert_eq!(json["status"], "failed");
  assert_eq!(json["command_id"], "input.key");
  assert_eq!(json["failure"], "window:missing was not found");
  assert_eq!(json["failure_details"]["code"], "not_found");
  assert_eq!(json["failure_details"]["message"], json["failure"]);
}

#[test]
fn compact_json_preserves_complete_results_failures_and_artifact_paths() {
  let run_id = RunId::new();
  let uri = ArtifactUri::from_ids(run_id, ArtifactId::new());
  let metadata = ArtifactMetadata::new(
    uri.clone(),
    ArtifactPurpose::new("auv.scan.scroll_until_final_capture"),
    ContentType::new("image/png"),
    Some("png".into()),
    ByteLength::new(3).unwrap(),
    Sha256Digest::new([7; 32]),
    Attributes::empty(),
  );
  let registry = default_registry();
  let command = registry.resolve("input.scrollUntil").unwrap();
  let value = serde_json::json!({
    "result": {"reason": "text_visible", "text_match": {"text": "  Target\n東京  "},
      "action": {"verified": false, "attempts": [{"succeeded": true, "fallback_reason": "test"}]}},
    "window": {"reference": {"id": "window-1"}}, "request": {"max_steps": 50},
    "diagnostics": [null, "retain me"]
  });
  let success =
    InvokeResult::from_command_result(run_id, command, Ok(InvokeCommandOutput::from_result(&value).unwrap().with_artifacts([metadata])))
      .with_artifact_paths([(uri, PathBuf::from("/tmp/final capture.png"))]);
  let failure = InvokeResult::from_command_result(
    run_id,
    command,
    Err(crate::InvokeFailure::from(auv_driver::DriverError::NotFound {
      target: "window:missing".into(),
    })),
  );
  for result in [success, failure] {
    let pretty = result
      .render_to_string(InvokeOutputOptions {
        json: true,
        ..Default::default()
      })
      .unwrap();
    let compact = result
      .render_to_string(InvokeOutputOptions {
        compact_json: true,
        ..Default::default()
      })
      .unwrap();
    assert_eq!(compact.lines().count(), 1);
    assert!(compact.ends_with('\n'));
    assert!(compact.len() < pretty.len());
    assert_eq!(serde_json::from_str::<serde_json::Value>(&compact).unwrap(), serde_json::from_str::<serde_json::Value>(&pretty).unwrap());
  }
}
