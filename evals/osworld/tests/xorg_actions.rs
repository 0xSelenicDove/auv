//! Ignored live gate: public OSWorld ActionExecutor -> AUV Runner -> X11 ->
//! independent Tk receiver. Run only against a disposable Xorg desktop.

use std::{
  fs::{self, File},
  process::{Child, Command, Stdio},
  thread,
  time::{Duration, Instant},
};

use auv_osworld_evals::{Action, ActionExecutor, ControlSignal, ExecuteError, parse_action};
use serde_json::{Value, json};

struct Receiver {
  child: Child,
  log: std::path::PathBuf,
  _directory: tempfile::TempDir,
}

impl Receiver {
  fn start() -> Self {
    let directory = tempfile::tempdir().unwrap();
    let log = directory.path().join("events.jsonl");
    let child = Command::new("python3")
      .arg("-u")
      .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/x11_receiver.py"))
      .env("AUV_OSWORLD_RECEIVER_LOG", &log)
      .stdout(Stdio::null())
      .stderr(Stdio::inherit())
      .spawn()
      .expect("start independent Tk receiver");
    let receiver = Self {
      child,
      log,
      _directory: directory,
    };
    receiver.expect_since(0, |events| events.iter().any(|event| event["kind"] == "ready"), "receiver ready");
    receiver
  }

  fn events(&self) -> Vec<Value> {
    let Ok(contents) = fs::read_to_string(&self.log) else {
      return Vec::new();
    };
    contents.lines().map(|line| serde_json::from_str(line).expect("receiver JSONL")).collect()
  }

  fn mark(&self) -> usize {
    self.events().len()
  }

  fn expect_since(&self, mark: usize, predicate: impl Fn(&[Value]) -> bool, label: &str) -> Vec<Value> {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
      let events = self.events();
      let recent = &events[mark..];
      if predicate(recent) {
        return recent.to_vec();
      }
      assert!(Instant::now() < deadline, "{label}: receiver saw {recent:?}");
      thread::sleep(Duration::from_millis(25));
    }
  }
}

impl Drop for Receiver {
  fn drop(&mut self) {
    let _ = self.child.kill();
    let _ = self.child.wait();
  }
}

/// Tk maps horizontal wheel buttons into its vertical binding on some X11
/// builds. xev observes the raw X11 ButtonPress stream instead.
struct RawWheelReceiver {
  child: Child,
  log: std::path::PathBuf,
  _directory: tempfile::TempDir,
}

impl RawWheelReceiver {
  fn start() -> Self {
    let directory = tempfile::tempdir().unwrap();
    let log = directory.path().join("xev.log");
    let child = Command::new("stdbuf")
      .arg("-oL")
      .arg("xev")
      .args(["-geometry", "300x300+1050+50", "-event", "mouse"])
      .stdout(File::create(&log).unwrap())
      .stderr(Stdio::inherit())
      .spawn()
      .expect("start raw X11 event receiver");
    Self {
      child,
      log,
      _directory: directory,
    }
  }

  fn button_presses(&self) -> Vec<i64> {
    let Ok(contents) = fs::read_to_string(&self.log) else {
      return Vec::new();
    };
    let mut presses = Vec::new();
    let mut pressed = false;
    for line in contents.lines() {
      if line.starts_with("ButtonPress event") {
        pressed = true;
      } else if line.starts_with("ButtonRelease event") {
        pressed = false;
      } else if pressed
        && let Some(button) = line.split("button ").nth(1).and_then(|value| value.split(',').next()).and_then(|value| value.parse().ok())
      {
        presses.push(button);
        pressed = false;
      }
    }
    presses
  }

  fn expect_since(&self, mark: usize, expected: &[i64]) {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
      let presses = self.button_presses();
      let recent = &presses[mark..];
      if recent.len() >= expected.len() {
        assert_eq!(recent, expected, "raw X11 wheel button order");
        return;
      }
      assert!(Instant::now() < deadline, "raw X11 wheel receiver saw {recent:?}, expected {expected:?}");
      thread::sleep(Duration::from_millis(25));
    }
  }
}

impl Drop for RawWheelReceiver {
  fn drop(&mut self) {
    let _ = self.child.kill();
    let _ = self.child.wait();
  }
}

fn count(events: &[Value], kind: &str, button: i64) -> usize {
  events.iter().filter(|event| event["kind"] == kind && event["button"] == button).count()
}

fn key_count(events: &[Value], kind: &str, key: &str) -> usize {
  events.iter().filter(|event| event["kind"] == kind && event["keysym"] == key).count()
}

fn action(value: Value) -> Action {
  parse_action(&value).unwrap()
}

/// Requires a disposable Xorg display with the installed AUV daemon and the
/// test process on the same DISPLAY. The receiver is independent of AUV's
/// delivery result; no OSWorld evaluator or task reward is involved.
#[tokio::test]
#[ignore = "set AUV_OSWORLD_TEST_DAEMON and DISPLAY on an isolated Xorg desktop"]
async fn every_supported_action_reaches_independent_x11_receiver() {
  let endpoint = std::env::var("AUV_OSWORLD_TEST_DAEMON").expect("set AUV_OSWORLD_TEST_DAEMON");
  let receiver = Receiver::start();
  let client = auv::Client::from_context(auv::AuvContext {
    daemon_endpoint: Some(endpoint),
    ..Default::default()
  })
  .await
  .unwrap();
  let runner = client.runner(Default::default()).await.unwrap();
  let input = runner.input();
  let mut executor = ActionExecutor::new(runner);

  let mark = receiver.mark();
  let moved = executor.execute(action(json!({"action_type":"MOVE_TO","x":321.5,"y":234.25}))).await.unwrap();
  assert_eq!(moved.delivery.len(), 1);
  assert_eq!(moved.semantic_verification, None);
  assert_eq!(input.current_position().await.unwrap(), auv_driver::Point::new(322.0, 234.0));
  receiver.expect_since(
    mark,
    |events| events.iter().any(|event| event["kind"] == "motion" && event["x"] == 322 && event["y"] == 234),
    "MOVE_TO",
  );

  for (value, button, expected, point) in [
    (json!({"action_type":"CLICK","x":420,"y":320,"num_clicks":1}), 1, 1, (420, 320)),
    (json!({"action_type":"CLICK","x":440,"y":320,"num_clicks":2}), 1, 2, (440, 320)),
    (json!({"action_type":"CLICK","x":460,"y":320,"num_clicks":3}), 1, 3, (460, 320)),
    (json!({"action_type":"RIGHT_CLICK","x":480,"y":320}), 3, 1, (480, 320)),
    (json!({"action_type":"CLICK","button":"middle","x":500,"y":320}), 2, 1, (500, 320)),
    (json!({"action_type":"DOUBLE_CLICK"}), 1, 2, (500, 320)),
  ] {
    let mark = receiver.mark();
    let outcome = executor.execute(action(value)).await.unwrap();
    assert_eq!(outcome.delivery.len(), 1);
    let events = receiver.expect_since(
      mark,
      |events| count(events, "button_down", button) >= expected && count(events, "button_up", button) >= expected,
      "CLICK button/count",
    );
    assert!(
      events
        .iter()
        .filter(|event| event["kind"] == "button_down" && event["button"] == button)
        .all(|event| event["x"] == point.0 && event["y"] == point.1)
    );
  }

  let mark = receiver.mark();
  executor.execute(action(json!({"action_type":"DRAG_TO","x":620,"y":420}))).await.unwrap();
  assert_eq!(input.current_position().await.unwrap(), auv_driver::Point::new(620.0, 420.0));
  receiver.expect_since(
    mark,
    |events| {
      count(events, "button_down", 1) == 1
        && count(events, "button_up", 1) == 1
        && events.iter().any(|event| {
          event["kind"] == "motion"
            && event["x"] == 620
            && event["y"] == 420
            && event["state"].as_i64().is_some_and(|state| state & 0x100 != 0)
        })
    },
    "DRAG_TO held motion",
  );

  let mark = receiver.mark();
  executor.execute(action(json!({"action_type":"MOUSE_DOWN","button":"left"}))).await.unwrap();
  executor.execute(action(json!({"action_type":"MOVE_TO","x":640,"y":440}))).await.unwrap();
  executor.execute(action(json!({"action_type":"MOUSE_UP","button":"left"}))).await.unwrap();
  receiver.expect_since(
    mark,
    |events| {
      count(events, "button_down", 1) == 1
        && count(events, "button_up", 1) == 1
        && events.iter().any(|event| {
          event["kind"] == "motion"
            && event["x"] == 640
            && event["y"] == 440
            && event["state"].as_i64().is_some_and(|state| state & 0x100 != 0)
        })
    },
    "MOUSE_DOWN / MOVE_TO / MOUSE_UP",
  );

  {
    let raw = RawWheelReceiver::start();
    thread::sleep(Duration::from_millis(200));
    executor.execute(action(json!({"action_type":"MOVE_TO","x":1100,"y":150}))).await.unwrap();
    for (value, expected) in [
      (json!({"action_type":"SCROLL","dx":2,"dy":-3}), vec![7, 7, 5, 5, 5]),
      (json!({"action_type":"SCROLL","dx":-1,"dy":1}), vec![6, 4]),
    ] {
      let mark = raw.button_presses().len();
      let outcome = executor.execute(action(value)).await.unwrap();
      assert_eq!(outcome.delivery.len(), 1);
      raw.expect_since(mark, &expected);
    }
  }

  executor.execute(action(json!({"action_type":"MOVE_TO","x":640,"y":440}))).await.unwrap();
  let mark = receiver.mark();
  executor.execute(action(json!({"action_type":"CLICK"}))).await.unwrap();
  receiver.expect_since(
    mark,
    |events| {
      events.iter().any(|event| event["kind"] == "button_down" && event["button"] == 1 && event["x"] == 640 && event["y"] == 440)
        && events.iter().any(|event| event["kind"] == "button_up" && event["button"] == 1 && event["x"] == 640 && event["y"] == 440)
    },
    "CLICK at current pointer",
  );

  let mark = receiver.mark();
  executor.execute(action(json!({"action_type":"TYPING","text":"abcΩ中"}))).await.unwrap();
  receiver.expect_since(
    mark,
    |events| events.iter().any(|event| event["kind"] == "text" && event["value"].as_str().is_some_and(|value| value.contains("abcΩ中"))),
    "TYPING ASCII and Unicode text",
  );

  let mark = receiver.mark();
  executor.execute(action(json!({"action_type":"PRESS","key":"F13"}))).await.unwrap();
  receiver.expect_since(mark, |events| key_count(events, "key_down", "F13") == 1 && key_count(events, "key_up", "F13") == 1, "PRESS F13");

  let mark = receiver.mark();
  executor.execute(action(json!({"action_type":"KEY_DOWN","key":"shift"}))).await.unwrap();
  executor.execute(action(json!({"action_type":"PRESS","key":"a"}))).await.unwrap();
  executor.execute(action(json!({"action_type":"KEY_UP","key":"shift"}))).await.unwrap();
  receiver.expect_since(
    mark,
    |events| {
      key_count(events, "key_down", "Shift_L") == 1 && key_count(events, "key_down", "A") == 1 && key_count(events, "key_up", "Shift_L") == 1
    },
    "held modifier plus PRESS",
  );

  let mark = receiver.mark();
  executor.execute(action(json!({"action_type":"KEY_DOWN","key":"ctrl"}))).await.unwrap();
  executor.execute(action(json!({"action_type":"KEY_DOWN","key":"shift"}))).await.unwrap();
  executor.execute(action(json!({"action_type":"PRESS","key":"a"}))).await.unwrap();
  executor.execute(action(json!({"action_type":"KEY_UP","key":"shift"}))).await.unwrap();
  executor.execute(action(json!({"action_type":"KEY_UP","key":"ctrl"}))).await.unwrap();
  receiver.expect_since(
    mark,
    |events| {
      key_count(events, "key_down", "Control_L") == 1
        && key_count(events, "key_down", "Shift_L") == 1
        && events.iter().any(|event| {
          event["kind"] == "key_down" && event["keysym"] == "A" && event["state"].as_i64().is_some_and(|state| state & 0x5 == 0x5)
        })
        && key_count(events, "key_up", "Shift_L") == 1
        && key_count(events, "key_up", "Control_L") == 1
    },
    "overlapping Control+Shift holds",
  );

  let mark = receiver.mark();
  executor.execute(action(json!({"action_type":"HOTKEY","keys":["ctrl","a"]}))).await.unwrap();
  receiver.expect_since(
    mark,
    |events| {
      key_count(events, "key_down", "Control_L") == 1
        && key_count(events, "key_down", "a") == 1
        && key_count(events, "key_up", "Control_L") == 1
    },
    "HOTKEY",
  );

  let mark = receiver.mark();
  for (kind, expected) in [
    ("WAIT", ControlSignal::Wait),
    ("DONE", ControlSignal::Done),
    ("FAIL", ControlSignal::Fail),
  ] {
    let outcome = executor.execute(action(json!(kind))).await.unwrap();
    assert_eq!(outcome.control, Some(expected));
    assert!(outcome.delivery.is_empty());
    assert_eq!(outcome.semantic_verification, None);
  }
  thread::sleep(Duration::from_millis(100));
  assert_eq!(receiver.mark(), mark, "benchmark controls emitted GUI input");

  let mark = receiver.mark();
  executor.execute(action(json!({"action_type":"MOUSE_DOWN"}))).await.unwrap();
  let failed = executor.execute(action(json!({"action_type":"RIGHT_CLICK"}))).await;
  assert!(matches!(failed, Err(ExecuteError::InvalidState(_))));
  receiver.expect_since(
    mark,
    |events| count(events, "button_down", 1) == 1 && count(events, "button_up", 1) == 1,
    "error cleanup releases mouse",
  );
  assert!(matches!(executor.execute(Action::Wait).await, Err(ExecuteError::InvalidState(_))), "failed episode must be poisoned");
  executor.finish(auv::runs::RunOutcome::Failed).await.unwrap();

  let runner = client.runner(Default::default()).await.unwrap();
  let mut executor = ActionExecutor::new(runner);
  let mark = receiver.mark();
  executor.execute(action(json!({"action_type":"KEY_DOWN","key":"ctrl"}))).await.unwrap();
  let failed = executor.execute(action(json!({"action_type":"PRESS","key":"browserback"}))).await;
  assert!(matches!(failed, Err(ExecuteError::UnsupportedKey(_))));
  receiver.expect_since(
    mark,
    |events| key_count(events, "key_down", "Control_L") == 1 && key_count(events, "key_up", "Control_L") == 1,
    "error cleanup releases key",
  );
  executor.finish(auv::runs::RunOutcome::Failed).await.unwrap();

  let runner = client.runner(Default::default()).await.unwrap();
  let mut executor = ActionExecutor::new(runner);
  let mark = receiver.mark();
  executor.execute(action(json!({"action_type":"KEY_DOWN","key":"shift"}))).await.unwrap();
  executor.execute(action(json!({"action_type":"MOUSE_DOWN","button":"middle"}))).await.unwrap();
  executor.finish(auv::runs::RunOutcome::Failed).await.unwrap();
  receiver.expect_since(
    mark,
    |events| key_count(events, "key_up", "Shift_L") == 1 && count(events, "button_up", 2) == 1,
    "finish releases both held inputs",
  );
}
