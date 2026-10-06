//! Foreground, operator-audited action sequence over one AUV Run/Runner.

use std::{
  fs,
  future::Future,
  io::{Read as _, Write as _},
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
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt};

use crate::{Action, ActionExecutor, ActionOutcome, ControlSignal, parse_action};

const MAX_FINAL_SETTLE_MS: u64 = 5_000;
const MAX_INTERACTIVE_LINE_BYTES: usize = 64 * 1024;
const MAX_INTERACTIVE_REQUEST_BYTES: usize = 1024 * 1024;
const MAX_INTERACTIVE_ACTIONS: usize = 1_000;
const MAX_INTERACTIVE_CAPTURES: usize = 32;
const INTERACTIVE_BUDGET: Duration = Duration::from_secs(570);
const INTERACTIVE_IDLE: Duration = Duration::from_secs(60);

#[derive(Debug, Deserialize)]
#[serde(tag = "op", deny_unknown_fields)]
enum RawInteractiveRequest {
  #[serde(rename = "action")]
  Action { seq: u64, action: Value },
  #[serde(rename = "capture")]
  Capture { seq: u64 },
  #[serde(rename = "finish")]
  Finish { seq: u64 },
  #[serde(rename = "abort")]
  Abort { seq: u64 },
}

enum InteractiveRequest {
  Action { seq: u64, action: Action },
  Capture { seq: u64 },
  Finish,
  Abort,
}

fn parse_interactive_request(line: &[u8], expected_seq: u64) -> Result<InteractiveRequest, String> {
  if line.len() > MAX_INTERACTIVE_LINE_BYTES {
    return Err("interactive request line exceeds 64 KiB".into());
  }
  let raw: RawInteractiveRequest = serde_json::from_slice(line).map_err(|error| format!("invalid interactive request: {error}"))?;
  let seq = match &raw {
    RawInteractiveRequest::Action { seq, .. }
    | RawInteractiveRequest::Capture { seq }
    | RawInteractiveRequest::Finish { seq }
    | RawInteractiveRequest::Abort { seq } => *seq,
  };
  if seq != expected_seq {
    return Err(format!("interactive sequence must be {expected_seq}"));
  }
  match raw {
    RawInteractiveRequest::Action { seq, action } => {
      let action = parse_action(&action).map_err(|error| error.to_string())?;
      if matches!(action, Action::Wait | Action::Done | Action::Fail) {
        return Err("interactive action must deliver typed GUI input; use finish or abort for control".into());
      }
      Ok(InteractiveRequest::Action { seq, action })
    }
    RawInteractiveRequest::Capture { seq } => Ok(InteractiveRequest::Capture { seq }),
    RawInteractiveRequest::Finish { .. } => Ok(InteractiveRequest::Finish),
    RawInteractiveRequest::Abort { .. } => Ok(InteractiveRequest::Abort),
  }
}

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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct InteractiveContext {
  version: u8,
  context: Context,
}

impl InteractiveContext {
  fn validate(self) -> Result<Context, String> {
    if self.version != 1 {
      return Err("interactive context version must be 1".into());
    }
    self.context.validate()?;
    Ok(self.context)
  }
}

impl Context {
  fn validate(&self) -> Result<(), String> {
    match self {
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
    Ok(())
  }
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
    self.context.validate()?;
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

fn read_interactive_context(path: &Path) -> Result<Context, String> {
  if !path.is_absolute() {
    return Err("interactive context path must be absolute".into());
  }
  let bytes = fs::read(path).map_err(|error| format!("cannot read interactive context: {error}"))?;
  let value: InteractiveContext = serde_json::from_slice(&bytes).map_err(|error| format!("invalid interactive context schema: {error}"))?;
  value.validate()
}

async fn read_interactive_line<R: AsyncBufRead + Unpin>(reader: &mut R) -> Result<Option<Vec<u8>>, String> {
  let mut bytes = Vec::new();
  let read = (&mut *reader)
    .take((MAX_INTERACTIVE_LINE_BYTES + 1) as u64)
    .read_until(b'\n', &mut bytes)
    .await
    .map_err(|error| format!("interactive input failed: {error}"))?;
  if read == 0 {
    return Ok(None);
  }
  if read > MAX_INTERACTIVE_LINE_BYTES {
    return Err("interactive request line exceeds 64 KiB".into());
  }
  if bytes.last() != Some(&b'\n') {
    return Err("interactive request must end with newline".into());
  }
  Ok(Some(bytes))
}

fn emit_interactive(value: &Value) -> Result<(), String> {
  let mut output = std::io::stdout().lock();
  serde_json::to_writer(&mut output, value).map_err(|error| error.to_string())?;
  output.write_all(b"\n").map_err(|error| error.to_string())?;
  output.flush().map_err(|error| error.to_string())
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
  save_named_capture(dir, "final-screenshot.png", capture)
}

fn save_named_capture(dir: &Path, name: &str, capture: auv_driver::DisplayCapture) -> Result<Value, String> {
  let mut temp = NamedTempFile::new_in(dir).map_err(|error| error.to_string())?;
  DynamicImage::ImageRgba8(capture.capture.image).write_to(&mut temp, ImageFormat::Png).map_err(|error| error.to_string())?;
  temp.as_file().sync_all().map_err(|error| error.to_string())?;
  let mut bytes = Vec::new();
  temp.reopen().map_err(|error| error.to_string())?.read_to_end(&mut bytes).map_err(|error| error.to_string())?;
  let digest = format!("{:x}", Sha256::digest(&bytes));
  temp.persist(dir.join(name)).map_err(|error| error.to_string())?;
  Ok(json!({"path": name, "sha256": digest}))
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

struct Session {
  dir: PathBuf,
  sidecar: PathBuf,
  run_id: String,
  executor: ActionExecutor,
  deliveries: Vec<Vec<InputActionResult>>,
  final_artifact: Option<Value>,
  checkpoints: Vec<Value>,
}

impl Session {
  async fn start(context: &Context) -> Result<Self, String> {
    let (dir, sidecar) = episode_paths()?;
    let (client, device_id) = connect(context).await?;
    let mut run_options = RunOptions {
      selection: RunSelection::New,
      ..Default::default()
    };
    run_options.device = auv::resource::DeviceSelector::by_id(&device_id);
    let runner = client.runner_with(run_options, RunnerOptions::default()).await.map_err(|error| error.to_string())?;
    let run_id = runner.run().id.to_string();
    let executor = ActionExecutor::new(runner);
    // Persist the observed Run before any GUI delivery or protocol response.
    if let Err(error) = atomic_json(&sidecar, &evidence(&run_id, None)) {
      let cleanup = executor.finish(RunOutcome::Canceled).await;
      return Err(match cleanup {
        Ok(_) => error,
        Err(cleanup) => format!("{error}; cleanup: {cleanup}"),
      });
    }
    Ok(Self {
      dir,
      sidecar,
      run_id,
      executor,
      deliveries: Vec::new(),
      final_artifact: None,
      checkpoints: Vec::new(),
    })
  }

  fn persist_delivery(&mut self, step: &ActionOutcome) -> Result<(), String> {
    self.deliveries.push(step.delivery.clone());
    atomic_json(&self.dir.join("input-action-results.json"), &json!(self.deliveries))
  }

  fn persist_final_capture(&mut self, capture: auv_driver::DisplayCapture) -> Result<(), String> {
    let artifact = save_capture(&self.dir, capture)?;
    self.final_artifact = Some(artifact);
    atomic_json(&self.sidecar, &evidence(&self.run_id, self.final_artifact.clone()))
  }

  fn persist_checkpoint(&mut self, capture: auv_driver::DisplayCapture) -> Result<Value, String> {
    let name = format!("checkpoint-{:04}.png", self.checkpoints.len() + 1);
    let artifact = save_named_capture(&self.dir, &name, capture)?;
    self.checkpoints.push(artifact.clone());
    atomic_json(&self.dir.join("checkpoints.json"), &json!(self.checkpoints))?;
    Ok(artifact)
  }

  async fn finish(self, outcome: RunOutcome, error: Option<String>) -> Result<(), String> {
    let mut error = error;
    if let Err(failure) = self.executor.finish(outcome).await {
      error = Some(match error {
        Some(primary) => format!("{primary}; finish: {failure}"),
        None => format!("finish: {failure}"),
      });
    }
    let terminal = evidence(&self.run_id, self.final_artifact);
    // Re-persist after finish so terminal stdout mirrors durable sidecar.
    emit_terminal(std::io::stdout().lock(), &self.sidecar, &terminal)?;
    match error {
      Some(error) => Err(error),
      None => Ok(()),
    }
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
  let mut session = Session::start(&plan.context).await?;
  let mut outcome = RunOutcome::Succeeded;
  let mut error = None;
  let cancel = interrupted();
  tokio::pin!(cancel);
  for action in plan.actions {
    let result = tokio::select! {
      result = session.executor.execute(action) => Some(result),
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
        if let Err(failure) = session.persist_delivery(&step) {
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

  if outcome == RunOutcome::Succeeded {
    let captured = tokio::select! {
      result = session.executor.capture_final() => Some(result),
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
      Some(Ok(capture)) => match session.persist_final_capture(capture) {
        Ok(()) => {}
        Err(failure) => {
          outcome = RunOutcome::Failed;
          error = Some(format!("final capture persistence failed: {failure}"));
        }
      },
    }
  }
  session.finish(outcome, error).await
}

/// Foreground JSONL control over one persistent AUV Run/Runner.
/// Intermediate responses are JSONL; the final line remains the exact action
/// sidecar object used by the batch runner. This mode is not wired to the
/// capture-only Kubernetes adapter or its stdin=DEVNULL batch phase.
// TODO(osworld-interactive-batch): Batch integration needs an owner-audited
// controller transport; do not silently add arbitrary action argv or relax
// batch_runner's stdin policy to expose this protocol.
pub async fn run_interactive(context_path: &Path) -> Result<(), String> {
  let context = read_interactive_context(context_path)?;
  let mut session = Session::start(&context).await?;
  // The bounded foreground protocol begins once the Run ID is durable.
  let deadline = tokio::time::Instant::now() + INTERACTIVE_BUDGET;
  let mut input = tokio::io::BufReader::new(tokio::io::stdin());
  let cancel = interrupted();
  tokio::pin!(cancel);
  let mut expected_seq = 1;
  let mut actions = 0;
  let mut captures = 0;
  let mut requests = Vec::<Value>::new();
  let mut request_bytes = 0;

  if let Err(error) = emit_interactive(&json!({
    "op": "ready", "version": 1, "run_id": session.run_id,
    "limits": {"actions": MAX_INTERACTIVE_ACTIONS, "captures": MAX_INTERACTIVE_CAPTURES,
               "line_bytes": MAX_INTERACTIVE_LINE_BYTES, "request_bytes": MAX_INTERACTIVE_REQUEST_BYTES,
               "budget_seconds": INTERACTIVE_BUDGET.as_secs(),
               "idle_seconds": INTERACTIVE_IDLE.as_secs()},
  })) {
    return session.finish(RunOutcome::Failed, Some(format!("ready response failed: {error}"))).await;
  }

  let (outcome, error) = loop {
    let now = tokio::time::Instant::now();
    if now >= deadline {
      break (RunOutcome::Canceled, Some("interactive session deadline reached".into()));
    }
    let read_deadline = (now + INTERACTIVE_IDLE).min(deadline);
    let line = tokio::select! {
      biased;
      _ = &mut cancel => break (RunOutcome::Canceled, Some("interactive session interrupted".into())),
      result = tokio::time::timeout_at(read_deadline, read_interactive_line(&mut input)) => match result {
        Ok(Ok(Some(line))) => line,
        Ok(Ok(None)) => break (RunOutcome::Canceled, Some("interactive input closed before finish".into())),
        Ok(Err(error)) => break (RunOutcome::Failed, Some(error)),
        Err(_) => break (RunOutcome::Canceled, Some("interactive input deadline reached".into())),
      },
    };
    let request = match parse_interactive_request(&line, expected_seq) {
      Ok(request) => request,
      Err(error) => break (RunOutcome::Failed, Some(error)),
    };
    request_bytes += line.len();
    if request_bytes > MAX_INTERACTIVE_REQUEST_BYTES {
      break (RunOutcome::Failed, Some("interactive request byte limit reached".into()));
    }
    let raw: Value = serde_json::from_slice(&line).expect("validated interactive JSON");
    requests.push(raw);
    if let Err(error) = atomic_json(&session.dir.join("action-requests.json"), &json!(requests)) {
      break (RunOutcome::Failed, Some(format!("interactive request persistence failed: {error}")));
    }
    match request {
      InteractiveRequest::Action { seq, action } => {
        if actions >= MAX_INTERACTIVE_ACTIONS {
          break (RunOutcome::Failed, Some("interactive action limit reached".into()));
        }
        actions += 1;
        let step = tokio::select! {
          biased;
          _ = &mut cancel => break (RunOutcome::Canceled, Some("interactive action interrupted".into())),
          _ = tokio::time::sleep_until(deadline) => break (RunOutcome::Canceled, Some("interactive session deadline reached".into())),
          result = session.executor.execute(action) => match result {
            Ok(step) => step,
            Err(error) => break (RunOutcome::Failed, Some(error.to_string())),
          },
        };
        if let Err(error) = session.persist_delivery(&step) {
          break (RunOutcome::Failed, Some(format!("interactive input evidence persistence failed: {error}")));
        }
        if let Err(error) = emit_interactive(&json!({"seq": seq, "op": "action", "delivery": step.delivery})) {
          break (RunOutcome::Failed, Some(format!("interactive action response failed: {error}")));
        }
      }
      InteractiveRequest::Capture { seq } => {
        if captures >= MAX_INTERACTIVE_CAPTURES {
          break (RunOutcome::Failed, Some("interactive capture limit reached".into()));
        }
        captures += 1;
        let frame = tokio::select! {
          biased;
          _ = &mut cancel => break (RunOutcome::Canceled, Some("interactive capture interrupted".into())),
          _ = tokio::time::sleep_until(deadline) => break (RunOutcome::Canceled, Some("interactive session deadline reached".into())),
          result = session.executor.capture_final() => match result {
            Ok(frame) => frame,
            Err(error) => break (RunOutcome::Failed, Some(error.to_string())),
          },
        };
        let artifact = match session.persist_checkpoint(frame) {
          Ok(artifact) => artifact,
          Err(error) => break (RunOutcome::Failed, Some(format!("interactive checkpoint persistence failed: {error}"))),
        };
        if let Err(error) = emit_interactive(&json!({"seq": seq, "op": "capture", "artifact": artifact})) {
          break (RunOutcome::Failed, Some(format!("interactive capture response failed: {error}")));
        }
      }
      InteractiveRequest::Finish => {
        let frame = tokio::select! {
          biased;
          _ = &mut cancel => break (RunOutcome::Canceled, Some("interactive final capture interrupted".into())),
          _ = tokio::time::sleep_until(deadline) => break (RunOutcome::Canceled, Some("interactive session deadline reached".into())),
          result = session.executor.capture_final() => match result {
            Ok(frame) => frame,
            Err(error) => break (RunOutcome::Failed, Some(error.to_string())),
          },
        };
        if let Err(error) = session.persist_final_capture(frame) {
          break (RunOutcome::Failed, Some(format!("interactive final capture persistence failed: {error}")));
        }
        break (RunOutcome::Succeeded, None);
      }
      InteractiveRequest::Abort => break (RunOutcome::Canceled, Some("interactive session aborted".into())),
    }
    expected_seq += 1;
  };
  session.finish(outcome, error).await
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

  #[test]
  fn interactive_jsonl_accepts_only_ordered_typed_gui_requests() {
    let click = parse_interactive_request(br#"{"seq":1,"op":"action","action":{"action_type":"CLICK","x":1269,"y":638}}"#, 1).unwrap();
    assert!(matches!(
      click,
      InteractiveRequest::Action {
        seq: 1,
        action: Action::Click { .. }
      }
    ));
    assert!(matches!(parse_interactive_request(br#"{"seq":2,"op":"capture"}"#, 2).unwrap(), InteractiveRequest::Capture { seq: 2 }));
    for invalid in [
      br#"{"seq":1,"op":"action","action":{"action_type":"EXECUTE","command":"xdotool click 1"}}"#.as_slice(),
      br#"{"seq":1,"op":"action","action":"WAIT"}"#.as_slice(),
      br#"{"seq":1,"op":"action","action":"DONE"}"#.as_slice(),
      br#"{"seq":1,"op":"capture","command":"python x.py"}"#.as_slice(),
      br#"{"seq":1,"op":"shell","command":"echo hi"}"#.as_slice(),
      br#"{"seq":2,"op":"capture"}"#.as_slice(),
      br#"{"seq":1,"op":"action","action":{"action_type":"CLICK","x":0,"y":0},"extra":true}"#.as_slice(),
    ] {
      assert!(parse_interactive_request(invalid, 1).is_err());
    }
    assert!(matches!(parse_interactive_request(br#"{"seq":3,"op":"finish"}"#, 3).unwrap(), InteractiveRequest::Finish));
    assert!(matches!(parse_interactive_request(br#"{"seq":4,"op":"abort"}"#, 4).unwrap(), InteractiveRequest::Abort));
  }

  #[test]
  fn interactive_context_is_strict_and_requires_explicit_endpoint_or_profile() {
    let dir = tempfile::tempdir().unwrap();
    let context_path = dir.path().join("context.json");
    fs::write(
      &context_path,
      br#"{"version":1,"context":{"kind":"guest-local","device_id":"device","daemon_endpoint":"unix:///tmp/auv.sock"}}"#,
    )
    .unwrap();
    assert!(matches!(read_interactive_context(&context_path).unwrap(), Context::GuestLocal { .. }));
    assert!(read_interactive_context(Path::new("context.json")).is_err());
    for invalid in [
      json!({"version":2,"context":{"kind":"guest-local","device_id":"device","daemon_endpoint":"unix:///tmp/auv.sock"}}),
      json!({"version":1,"context":{"kind":"guest-local","device_id":"device","daemon_endpoint":"http://relay"}}),
      json!({"version":1,"context":{"kind":"guest-local","device_id":"device","daemon_endpoint":"unix:///tmp/auv.sock","command":"xdotool click 1"}}),
      json!({"version":1,"context":{"kind":"paired","device_id":"device","config_profile":"episode","profiles_file":"relative.json"}}),
    ] {
      fs::write(&context_path, serde_json::to_vec(&invalid).unwrap()).unwrap();
      assert!(read_interactive_context(&context_path).is_err());
    }
  }

  #[tokio::test]
  async fn interactive_reader_rejects_truncated_and_oversized_jsonl() {
    use tokio::io::AsyncWriteExt;
    let (mut writer, reader) = tokio::io::duplex(MAX_INTERACTIVE_LINE_BYTES + 16);
    writer.write_all(b"{\"seq\":1,\"op\":\"capture\"}\n").await.unwrap();
    writer.write_all(b"{\"seq\":2").await.unwrap();
    drop(writer);
    let mut reader = tokio::io::BufReader::new(reader);
    assert!(read_interactive_line(&mut reader).await.unwrap().is_some());
    assert!(read_interactive_line(&mut reader).await.is_err());
    let (mut writer, reader) = tokio::io::duplex(MAX_INTERACTIVE_LINE_BYTES + 16);
    writer.write_all(&vec![b' '; MAX_INTERACTIVE_LINE_BYTES + 1]).await.unwrap();
    drop(writer);
    assert!(read_interactive_line(&mut tokio::io::BufReader::new(reader)).await.is_err());
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
    let checkpoint = save_named_capture(dir.path(), "checkpoint-0001.png", capture.clone()).unwrap();
    atomic_json(&dir.path().join("checkpoints.json"), &json!([checkpoint.clone()])).unwrap();
    let checkpoint_bytes = fs::read(dir.path().join("checkpoint-0001.png")).unwrap();
    assert_eq!(checkpoint["sha256"], format!("{:x}", Sha256::digest(&checkpoint_bytes)));
    assert_eq!(serde_json::from_slice::<Value>(&fs::read(dir.path().join("checkpoints.json")).unwrap()).unwrap()[0], checkpoint);
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
