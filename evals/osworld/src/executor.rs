//! Stateful OSWorld GUI delivery over one routed AUV Runner.

use std::{collections::HashMap, time::Duration};

use auv::client::{PlacementError, RunnerExecution, runner::CapabilityError};
use auv_driver::{
  Click, ClickModifiers, InputActionResult, InputPolicy, InputTarget, MouseButton, MoveMouseRequest, PressKeysOptions, Scroll,
  TypeTextOptions,
};
use thiserror::Error;

use crate::{Action, Button, Key, Point};

const HOLD_LIMIT: Duration = Duration::from_secs(30);
const CLICK_INTERVAL: Duration = Duration::from_millis(100);
const DRAG_DURATION: Duration = Duration::from_millis(500);

/// A benchmark transition; these are never delivered as GUI events.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlSignal {
  Wait,
  Done,
  Fail,
}

/// Driver delivery and benchmark semantics remain distinct. `verified` on an
/// individual driver result is not an OSWorld task success judgement.
#[derive(Clone, Debug, PartialEq)]
pub struct ActionOutcome {
  pub delivery: Vec<InputActionResult>,
  pub control: Option<ControlSignal>,
  /// This adapter never evaluates the task environment or its reward.
  pub semantic_verification: Option<bool>,
}

impl ActionOutcome {
  fn delivered(delivery: Vec<InputActionResult>) -> Self {
    Self {
      delivery,
      control: None,
      semantic_verification: None,
    }
  }

  fn control(control: ControlSignal) -> Self {
    Self {
      delivery: Vec::new(),
      control: Some(control),
      semantic_verification: None,
    }
  }
}

#[derive(Debug, Error)]
pub enum ExecuteError {
  #[error("invalid OSWorld input state: {0}")]
  InvalidState(String),
  #[error("unsupported OSWorld key spelling for AUV X11 input: {0}")]
  UnsupportedKey(String),
  #[error("AUV Runner input delivery failed: {0}")]
  Delivery(#[from] CapabilityError),
  #[error("AUV Run lifecycle failed: {0}")]
  Placement(#[from] PlacementError),
  #[error("{primary}; cleanup also failed: {cleanup}")]
  Cleanup { primary: String, cleanup: String },
}

/// One OSWorld episode owns one AUV Run/Runner and its cross-call holds.
/// Invoke `finish` on every terminal path; `Drop` cannot perform async release.
pub struct ActionExecutor {
  runner: RunnerExecution,
  key_holds: HashMap<String, auv_driver::KeyboardHoldId>,
  mouse_hold: Option<(Button, u64)>,
  failed: bool,
}

impl ActionExecutor {
  pub fn new(runner: RunnerExecution) -> Self {
    Self {
      runner,
      key_holds: HashMap::new(),
      mouse_hold: None,
      failed: false,
    }
  }

  /// Submit one validated action. On a delivery error, actively release all
  /// holds, poison this episode, and preserve both primary and cleanup errors.
  pub async fn execute(&mut self, action: Action) -> Result<ActionOutcome, ExecuteError> {
    if self.failed {
      return Err(ExecuteError::InvalidState("episode failed; finish it before submitting another action".into()));
    }
    match self.deliver(action).await {
      Ok(outcome) => Ok(outcome),
      Err(primary) => {
        self.failed = true;
        match self.cleanup().await {
          Ok(()) => Err(primary),
          Err(cleanup) => Err(ExecuteError::Cleanup {
            primary: primary.to_string(),
            cleanup: cleanup.to_string(),
          }),
        }
      }
    }
  }

  /// Release any held input, then finish only the implicitly owned AUV Run.
  /// An attached Run remains available to its original owner.
  pub async fn finish(mut self, outcome: auv::runs::RunOutcome) -> Result<auv::runs::Run, ExecuteError> {
    let cleanup = self.cleanup().await;
    let finished = self.runner.finish(outcome).await;
    match (cleanup, finished) {
      (Ok(()), Ok(run)) => Ok(run),
      (Err(cleanup), Ok(_)) => Err(cleanup),
      (Ok(()), Err(error)) => Err(error.into()),
      (Err(cleanup), Err(error)) => Err(ExecuteError::Cleanup {
        primary: error.to_string(),
        cleanup: cleanup.to_string(),
      }),
    }
  }

  async fn deliver(&mut self, action: Action) -> Result<ActionOutcome, ExecuteError> {
    let input = self.runner.input();
    match action {
      Action::Wait => Ok(ActionOutcome::control(ControlSignal::Wait)),
      Action::Done => Ok(ActionOutcome::control(ControlSignal::Done)),
      Action::Fail => Ok(ActionOutcome::control(ControlSignal::Fail)),
      Action::MoveTo(point) => {
        let point = pixel(point);
        let mut request = MoveMouseRequest::direct(point);
        if let Some((_, mouse)) = self.mouse_hold {
          request.mouse = mouse;
        }
        let mut stream = input.move_mouse(request).await?;
        while let Some(event) = stream.next().await? {
          if let auv::client::runner::MouseMotionEvent::Completed { action, .. } = event {
            return Ok(ActionOutcome::delivered(vec![action]));
          }
        }
        Err(ExecuteError::InvalidState("MoveMouse stream ended without completion evidence".into()))
      }
      Action::Click {
        button,
        position,
        count,
      } => {
        if self.mouse_hold.is_some() {
          return Err(ExecuteError::InvalidState("CLICK while a mouse button is held is ambiguous".into()));
        }
        let point = match position {
          Some(point) => pixel(point),
          // TODO(osworld-atomic-current-click): this observation and click are
          // separate RPCs; add an atomic current-pointer click only if a
          // receiver test shows external pointer races matter to the harness.
          None => input.current_position().await?,
        };
        let click = match count.get() {
          1 => Click::Single,
          2 => Click::Double {
            interval: CLICK_INTERVAL,
          },
          3 => Click::Repeated {
            count: 3,
            interval: CLICK_INTERVAL,
          },
          _ => return Err(ExecuteError::InvalidState("click count is outside 1..=3".into())),
        };
        let result = input.click_screen_point(point, mouse_button(button), click, ClickModifiers::default()).await?;
        Ok(ActionOutcome::delivered(vec![result.action]))
      }
      Action::MouseDown(button) => {
        if self.mouse_hold.is_some() {
          // TODO(osworld-mouse-chord): the shared MouseCoordinator supports one
          // held button per desktop; add chords only after a driver contract.
          return Err(ExecuteError::InvalidState("simultaneous mouse buttons are not supported".into()));
        }
        let point = input.current_position().await?;
        let mouse = input.create_mouse().await?;
        // Record the ID before delivery: a lost response does not prove the
        // press was absent, and cleanup must attempt to release this mouse.
        self.mouse_hold = Some((button, mouse));
        let result = input.mouse_down(&InputTarget::Foreground, mouse, point, mouse_button(button), HOLD_LIMIT).await?;
        Ok(ActionOutcome::delivered(vec![result]))
      }
      Action::MouseUp(button) => {
        let Some((held_button, mouse)) = self.mouse_hold else {
          return Err(ExecuteError::InvalidState("MOUSE_UP without MOUSE_DOWN".into()));
        };
        if held_button != button {
          return Err(ExecuteError::InvalidState("MOUSE_UP button differs from held button".into()));
        }
        let released = input.mouse_up(mouse).await?;
        let removed = input.remove_mouse(mouse).await?;
        self.mouse_hold = None;
        Ok(ActionOutcome::delivered(vec![released, removed]))
      }
      Action::DragTo(end) => {
        if self.mouse_hold.is_some() {
          return Err(ExecuteError::InvalidState("DRAG_TO while a mouse button is held is ambiguous".into()));
        }
        let start = input.current_position().await?;
        let end = pixel(end);
        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let mut request = MoveMouseRequest::direct(start);
        request.target = Some(InputTarget::Foreground);
        request.curve.segments.push(auv_driver::MouseCubicBezierSegment {
          control_1: auv_driver::Point::new(dx / 3.0, dy / 3.0),
          control_2: auv_driver::Point::new(dx * 2.0 / 3.0, dy * 2.0 / 3.0),
          end: auv_driver::Point::new(dx, dy),
        });
        request.options = auv_driver::MouseMotionOptions {
          duration: DRAG_DURATION,
          sample_rate_hz: 60,
          curve_tolerance: 0.5,
        };
        let (_, action) = input.drag_mouse(request, MouseButton::Left).await?;
        Ok(ActionOutcome::delivered(vec![action]))
      }
      Action::Scroll { dx, dy } => {
        if self.mouse_hold.is_some() {
          return Err(ExecuteError::InvalidState("SCROLL while a mouse button is held is ambiguous".into()));
        }
        let Some(scroll) = scroll_delta(dx, dy)? else {
          return Ok(ActionOutcome::delivered(Vec::new()));
        };
        let point = input.current_position().await?;
        let result = input.scroll_screen_point(point, scroll, Duration::ZERO).await?;
        Ok(ActionOutcome::delivered(vec![result.action]))
      }
      Action::Typing(text) => {
        let result = input
          .type_text(
            text,
            TypeTextOptions {
              policy: InputPolicy::ForegroundPreferred,
              ..Default::default()
            },
          )
          .await?;
        Ok(ActionOutcome::delivered(vec![result]))
      }
      Action::Press(key) => {
        let key = delivery_key(&key)?;
        if self.key_holds.contains_key(&key) {
          return Err(ExecuteError::InvalidState(format!("PRESS overlaps held key {key:?}")));
        }
        let result = input.press_keys(&InputTarget::Foreground, press_options(vec![key]), InputPolicy::ForegroundPreferred, false).await?;
        Ok(ActionOutcome::delivered(vec![result.expect("non-dry-run PressKeys returns evidence")]))
      }
      Action::KeyDown(key) => {
        let key = delivery_key(&key)?;
        if self.key_holds.contains_key(&key) {
          return Err(ExecuteError::InvalidState(format!("KEY_DOWN duplicates held key {key:?}")));
        }
        // NOTICE(osworld-lost-hold-id): a lost KeyDown response may have
        // delivered a press without returning its ID. The Runner's 30-second
        // deadline is the recovery bound; only a protocol lease would allow
        // immediate release after that transport failure.
        let (hold, result) =
          input.key_down(&InputTarget::Foreground, vec![key.clone()], InputPolicy::ForegroundPreferred, HOLD_LIMIT).await?;
        self.key_holds.insert(key, hold);
        Ok(ActionOutcome::delivered(vec![result]))
      }
      Action::KeyUp(key) => {
        let key = delivery_key(&key)?;
        let hold =
          *self.key_holds.get(&key).ok_or_else(|| ExecuteError::InvalidState(format!("KEY_UP without matching KEY_DOWN for {key:?}")))?;
        let result = input.key_up(hold).await?;
        self.key_holds.remove(&key);
        Ok(ActionOutcome::delivered(vec![result]))
      }
      Action::Hotkey(keys) => {
        let keys = keys.iter().map(delivery_key).collect::<Result<Vec<_>, _>>()?;
        if keys.iter().any(|key| self.key_holds.contains_key(key)) {
          return Err(ExecuteError::InvalidState("HOTKEY overlaps a held key".into()));
        }
        let mut distinct = keys.clone();
        distinct.sort();
        distinct.dedup();
        if distinct.len() != keys.len() {
          return Err(ExecuteError::InvalidState("HOTKEY contains duplicate native keys".into()));
        }
        let result = input.press_keys(&InputTarget::Foreground, press_options(keys), InputPolicy::ForegroundPreferred, false).await?;
        Ok(ActionOutcome::delivered(vec![result.expect("non-dry-run PressKeys returns evidence")]))
      }
    }
  }

  async fn cleanup(&mut self) -> Result<(), ExecuteError> {
    let input = self.runner.input();
    let mut failures = Vec::new();
    if let Some((_, mouse)) = self.mouse_hold.take() {
      if let Err(error) = input.mouse_up(mouse).await {
        failures.push(format!("MouseUp({mouse}): {error}"));
      }
      if let Err(error) = input.remove_mouse(mouse).await {
        failures.push(format!("RemoveMouse({mouse}): {error}"));
      }
    }
    for (key, hold) in self.key_holds.drain() {
      if let Err(error) = input.key_up(hold).await {
        failures.push(format!("KeyUp({key:?}, {hold}): {error}"));
      }
    }
    if failures.is_empty() {
      Ok(())
    } else {
      Err(ExecuteError::InvalidState(failures.join("; ")))
    }
  }
}

// NOTICE(osworld-pixel-rounding): finite positive coordinates are already
// parser-validated. Round half-pixels away from zero before X11's integer
// coordinate boundary; never silently truncate a fraction.
fn pixel(point: Point) -> auv_driver::Point {
  auv_driver::Point::new(point.x().round(), point.y().round())
}

fn mouse_button(button: Button) -> MouseButton {
  match button {
    Button::Left => MouseButton::Left,
    Button::Right => MouseButton::Right,
    Button::Middle => MouseButton::Middle,
  }
}

fn press_options(keys: Vec<String>) -> PressKeysOptions {
  PressKeysOptions {
    keys,
    ..Default::default()
  }
}

// NOTICE(osworld-wheel): OSWorld uses PyAutoGUI wheel detents (+Y up), while
// AUV uses logical pixels (+Y down). The pinned X11 receiver calibration is
// 120 AUV px per detent; X11 sends horizontal before vertical within one RPC.
fn scroll_delta(dx: i64, dy: i64) -> Result<Option<Scroll>, ExecuteError> {
  if dx == 0 && dy == 0 {
    return Ok(None);
  }
  if dx.unsigned_abs() > 1024 || dy.unsigned_abs() > 1024 {
    return Err(ExecuteError::InvalidState("SCROLL exceeds the X11 per-axis 1024-detent limit".into()));
  }
  Ok(Some(Scroll::new((dx as f64) * 120.0, -(dy as f64) * 120.0)))
}

/// Translate only spellings known to the current X11 key parser. Do not pass
/// every syntactically valid upstream name through and discover failure after
/// another action in the episode has already changed desktop state.
fn delivery_key(key: &Key) -> Result<String, ExecuteError> {
  let raw = key.as_str();
  let translated = match raw {
    " " => "space",
    "\t" => "tab",
    "\n" | "\r" => "return",
    "ctrl" | "alt" | "shift" | "win" | "return" | "enter" | "esc" | "escape" | "tab" | "backspace" | "delete" | "del" | "left" | "right"
    | "up" | "down" | "home" | "end" | "pageup" | "pgup" | "pagedown" | "pgdn" | "print" | "printscreen" | "prtsc" | "prtscr"
    | "capslock" | "numlock" | "scrolllock" | "insert" | "pause" => raw,
    "prntscrn" => "printscreen",
    "option" => "alt",
    "command" => "win",
    name if name.len() == 1 && name.as_bytes()[0].is_ascii_graphic() => name,
    name if name.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()).is_some_and(|n| (1..=24).contains(&n)) => name,
    _ => return Err(ExecuteError::UnsupportedKey(raw.into())),
  };
  Ok(translated.into())
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;

  #[test]
  fn rounds_half_pixels_and_calibrates_key_aliases() {
    let Action::MoveTo(point) = crate::parse_action(&json!({"action_type":"MOVE_TO","x":1.5,"y":2.49})).unwrap() else {
      panic!()
    };
    assert_eq!(pixel(point), auv_driver::Point::new(2.0, 2.0));
    let Action::Press(key) = crate::parse_action(&json!({"action_type":"PRESS","key":"OPTION"})).unwrap() else {
      panic!()
    };
    assert_eq!(delivery_key(&key).unwrap(), "alt");
  }

  #[test]
  fn rejects_upstream_keys_not_supported_by_x11() {
    for spelling in ["browserback", "ctrlleft", "shiftright"] {
      let Action::Press(key) = crate::parse_action(&json!({"action_type":"PRESS","key":spelling})).unwrap() else {
        panic!()
      };
      assert!(matches!(delivery_key(&key), Err(ExecuteError::UnsupportedKey(value)) if value == spelling));
    }
  }

  #[test]
  fn converts_osworld_wheel_steps_to_auv_x11_pixels() {
    assert_eq!(scroll_delta(2, -3).unwrap(), Some(Scroll::new(240.0, 360.0)));
    assert_eq!(scroll_delta(-1, 4).unwrap(), Some(Scroll::new(-120.0, -480.0)));
    assert_eq!(scroll_delta(0, 0).unwrap(), None);
    assert!(matches!(scroll_delta(1025, 0), Err(ExecuteError::InvalidState(_))));
  }

  /// Run with `AUV_OSWORLD_TEST_DAEMON=unix:///...` against an isolated Xorg
  /// fixture. This exercises the public Client/Runner route, not a planner.
  #[tokio::test]
  #[ignore = "requires an isolated Xorg AUV daemon"]
  async fn public_runner_delivers_a_stateful_episode() {
    let endpoint = std::env::var("AUV_OSWORLD_TEST_DAEMON").expect("set AUV_OSWORLD_TEST_DAEMON");
    let client = auv::Client::from_context(auv::AuvContext {
      daemon_endpoint: Some(endpoint),
      ..Default::default()
    })
    .await
    .unwrap();
    let runner = client.runner(Default::default()).await.unwrap();
    let input = runner.input();
    let mut executor = ActionExecutor::new(runner);
    let move_to = crate::parse_action(&json!({"action_type":"MOVE_TO","x":321.5,"y":234.25})).unwrap();
    let moved = executor.execute(move_to).await.unwrap();
    assert_eq!(moved.delivery.len(), 1);
    assert_eq!(moved.semantic_verification, None);
    assert_eq!(input.current_position().await.unwrap(), auv_driver::Point::new(322.0, 234.0));
    for action in [
      json!({"action_type":"MOUSE_DOWN"}),
      json!({"action_type":"MOVE_TO","x":331,"y":245}),
      json!({"action_type":"MOUSE_UP"}),
      json!({"action_type":"KEY_DOWN","key":"shift"}),
      json!({"action_type":"PRESS","key":"a"}),
      json!({"action_type":"KEY_UP","key":"shift"}),
      json!({"action_type":"SCROLL","dx":1,"dy":-1}),
      json!({"action_type":"RIGHT_CLICK"}),
    ] {
      executor.execute(crate::parse_action(&action).unwrap()).await.unwrap();
    }
    assert_eq!(input.current_position().await.unwrap(), auv_driver::Point::new(331.0, 245.0));
    let done = executor.execute(Action::Done).await.unwrap();
    assert_eq!(done.control, Some(ControlSignal::Done));
    executor.execute(Action::MouseDown(Button::Left)).await.unwrap();
    let failure = executor.execute(crate::parse_action(&json!({"action_type":"RIGHT_CLICK"})).unwrap()).await;
    assert!(matches!(failure, Err(ExecuteError::InvalidState(_))));
    // A fresh logical mouse can take admission only if the executor released
    // the previous cross-call hold on its error path.
    let mouse = input.create_mouse().await.unwrap();
    let point = input.current_position().await.unwrap();
    input.mouse_down(&InputTarget::Foreground, mouse, point, MouseButton::Left, HOLD_LIMIT).await.unwrap();
    input.mouse_up(mouse).await.unwrap();
    input.remove_mouse(mouse).await.unwrap();
    executor.finish(auv::runs::RunOutcome::Failed).await.unwrap();

    let runner = client.runner(Default::default()).await.unwrap();
    let input = runner.input();
    let mut executor = ActionExecutor::new(runner);
    executor.execute(crate::parse_action(&json!({"action_type":"KEY_DOWN","key":"ctrl"})).unwrap()).await.unwrap();
    let failure = executor.execute(crate::parse_action(&json!({"action_type":"PRESS","key":"browserback"})).unwrap()).await;
    assert!(matches!(failure, Err(ExecuteError::UnsupportedKey(_))));
    let (hold, _) =
      input.key_down(&InputTarget::Foreground, vec!["ctrl".into()], InputPolicy::ForegroundPreferred, HOLD_LIMIT).await.unwrap();
    input.key_up(hold).await.unwrap();
    executor.finish(auv::runs::RunOutcome::Failed).await.unwrap();
  }
}
