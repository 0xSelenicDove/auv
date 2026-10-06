use serde_json::{Map, Value};
use thiserror::Error;

// Pinned upstream schemas: OSWorld b138d348 and OSWorld-V2 acdd3493,
// desktop_env/actions.py. Coordinates are the upstream 1920x1080 bounds.
const X_MAX: f64 = 1920.0;
const Y_MAX: f64 = 1080.0;

/// A screen point in the pinned OSWorld action space. Conversion to AUV's
/// integer pixels belongs to the delivery adapter, not this parser.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
  x: f64,
  y: f64,
}

impl Point {
  pub fn x(self) -> f64 {
    self.x
  }
  pub fn y(self) -> f64 {
    self.y
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
  Left,
  Right,
  Middle,
}

/// Upstream permits exactly one, two, or three clicks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClickCount(u8);

impl ClickCount {
  pub fn get(self) -> u8 {
    self.0
  }
}

/// An upstream keyboard key spelling, normalized to lowercase for named keys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Key(String);

impl Key {
  pub fn as_str(&self) -> &str {
    &self.0
  }
}

/// Normalized structured `computer_13` action. `Wait`, `Done`, and `Fail`
/// are benchmark control signals, not GUI input.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
  MoveTo(Point),
  Click {
    button: Button,
    position: Option<Point>,
    count: ClickCount,
  },
  MouseDown(Button),
  MouseUp(Button),
  DragTo(Point),
  /// Raw PyAutoGUI wheel steps; the X11 executor maps one step to 120 AUV pixels.
  Scroll {
    dx: i64,
    dy: i64,
  },
  Typing(String),
  Press(Key),
  KeyDown(Key),
  KeyUp(Key),
  Hotkey(Vec<Key>),
  Wait,
  Done,
  Fail,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ActionError {
  #[error("action must be a string or object")]
  InvalidShape,
  #[error("missing or invalid action_type")]
  MissingType,
  #[error("unsupported action type: {0}")]
  UnsupportedType(String),
  #[error("EXECUTE is a benchmark control-plane operation, not GUI input")]
  ExecuteNotGui,
  #[error("invalid action parameters: {0}")]
  InvalidParameters(String),
}

/// Parse only enumerated structured actions; Python/PyAutoGUI source is never
/// interpreted. Accept both upstream flat and nested `parameters` shapes.
pub fn parse_action(value: &Value) -> Result<Action, ActionError> {
  let (kind, mut params) = match value {
    Value::String(kind) => (kind.as_str(), Map::new()),
    Value::Object(map) => {
      let kind = map.get("action_type").and_then(Value::as_str).ok_or(ActionError::MissingType)?;
      let params = if let Some(nested) = map.get("parameters") {
        if map.len() != 2 {
          return invalid("mixed flat and nested parameters");
        }
        nested.as_object().ok_or_else(|| ActionError::InvalidParameters("parameters must be an object".into()))?.clone()
      } else {
        map.iter().filter(|(key, _)| *key != "action_type").map(|(key, value)| (key.clone(), value.clone())).collect()
      };
      (kind, params)
    }
    _ => return Err(ActionError::InvalidShape),
  };

  let action = match kind {
    "WAIT" => Action::Wait,
    "DONE" => Action::Done,
    "FAIL" => Action::Fail,
    "EXECUTE" => return Err(ActionError::ExecuteNotGui),
    "MOVE_TO" => Action::MoveTo(required_point(&mut params)?),
    "DRAG_TO" => Action::DragTo(required_point(&mut params)?),
    "CLICK" => Action::Click {
      button: button(&mut params)?.unwrap_or(Button::Left),
      position: optional_point(&mut params)?,
      count: match params.remove("num_clicks") {
        None => ClickCount(1),
        Some(Value::Number(n)) => match n.as_u64() {
          Some(count @ 1..=3) => ClickCount(count as u8),
          _ => return invalid("num_clicks must be an integer from 1 to 3"),
        },
        Some(_) => return invalid("num_clicks must be an integer from 1 to 3"),
      },
    },
    "RIGHT_CLICK" => Action::Click {
      button: Button::Right,
      position: optional_point(&mut params)?,
      count: ClickCount(1),
    },
    "DOUBLE_CLICK" => Action::Click {
      button: Button::Left,
      position: optional_point(&mut params)?,
      count: ClickCount(2),
    },
    "MOUSE_DOWN" => Action::MouseDown(button(&mut params)?.unwrap_or(Button::Left)),
    "MOUSE_UP" => Action::MouseUp(button(&mut params)?.unwrap_or(Button::Left)),
    "SCROLL" => {
      let dx = integer(&mut params, "dx")?;
      let dy = integer(&mut params, "dy")?;
      if dx.is_none() && dy.is_none() {
        return invalid("SCROLL requires dx or dy");
      }
      Action::Scroll {
        dx: dx.unwrap_or(0),
        dy: dy.unwrap_or(0),
      }
    }
    "TYPING" => Action::Typing(string(&mut params, "text")?),
    "PRESS" => Action::Press(key(&mut params, "key")?),
    "KEY_DOWN" => Action::KeyDown(key(&mut params, "key")?),
    "KEY_UP" => Action::KeyUp(key(&mut params, "key")?),
    "HOTKEY" => {
      let Some(Value::Array(keys)) = params.remove("keys") else {
        return invalid("keys must be a nonempty array");
      };
      if keys.is_empty() {
        return invalid("keys must be a nonempty array");
      }
      Action::Hotkey(keys.iter().map(parse_key).collect::<Result<_, _>>()?)
    }
    _ => return Err(ActionError::UnsupportedType(kind.to_owned())),
  };
  if !params.is_empty() {
    return invalid(&format!("unknown fields: {}", params.keys().cloned().collect::<Vec<_>>().join(", ")));
  }
  Ok(action)
}

fn invalid<T>(message: &str) -> Result<T, ActionError> {
  Err(ActionError::InvalidParameters(message.to_owned()))
}

fn number(params: &mut Map<String, Value>, field: &str, max: f64) -> Result<Option<f64>, ActionError> {
  let Some(value) = params.remove(field) else {
    return Ok(None);
  };
  let Some(n) = value.as_f64() else {
    return invalid(&format!("{field} must be a finite number in 0..={max}"));
  };
  if !n.is_finite() || !(0.0..=max).contains(&n) {
    return invalid(&format!("{field} must be a finite number in 0..={max}"));
  }
  Ok(Some(n))
}

fn optional_point(params: &mut Map<String, Value>) -> Result<Option<Point>, ActionError> {
  match (number(params, "x", X_MAX)?, number(params, "y", Y_MAX)?) {
    (Some(x), Some(y)) => Ok(Some(Point { x, y })),
    (None, None) => Ok(None),
    _ => invalid("x and y must be supplied together"),
  }
}

fn required_point(params: &mut Map<String, Value>) -> Result<Point, ActionError> {
  optional_point(params)?.ok_or_else(|| ActionError::InvalidParameters("x and y are required".into()))
}

fn button(params: &mut Map<String, Value>) -> Result<Option<Button>, ActionError> {
  match params.remove("button") {
    None => Ok(None),
    Some(Value::String(value)) => match value.as_str() {
      "left" => Ok(Some(Button::Left)),
      "right" => Ok(Some(Button::Right)),
      "middle" => Ok(Some(Button::Middle)),
      _ => invalid("button must be left, right, or middle"),
    },
    Some(_) => invalid("button must be left, right, or middle"),
  }
}

fn integer(params: &mut Map<String, Value>, field: &str) -> Result<Option<i64>, ActionError> {
  match params.remove(field) {
    None => Ok(None),
    Some(Value::Number(number)) => {
      number.as_i64().map(Some).ok_or_else(|| ActionError::InvalidParameters(format!("{field} must be a signed integer")))
    }
    Some(_) => invalid(&format!("{field} must be a signed integer")),
  }
}

fn string(params: &mut Map<String, Value>, field: &str) -> Result<String, ActionError> {
  params
    .remove(field)
    .and_then(|value| value.as_str().map(str::to_owned))
    .ok_or_else(|| ActionError::InvalidParameters(format!("{field} must be a string")))
}

fn key(params: &mut Map<String, Value>, field: &str) -> Result<Key, ActionError> {
  let value = params.remove(field).ok_or_else(|| ActionError::InvalidParameters(format!("{field} is required")))?;
  parse_key(&value)
}

// The named-key list is copied from both pinned upstream ACTION_SPACE schemas.
// Some accepted OSWorld names may still lack AUV/X11 delivery support; the
// executor must report that separately instead of weakening schema parsing.
const NAMED_KEYS: &str = "accept add alt altleft altright apps backspace browserback browserfavorites browserforward browserhome browserrefresh browsersearch browserstop capslock clear convert ctrl ctrlleft ctrlright decimal del delete divide down end enter esc escape execute final fn hanguel hangul hanja help home insert junja kana kanji launchapp1 launchapp2 launchmail launchmediaselect left modechange multiply nexttrack nonconvert num0 num1 num2 num3 num4 num5 num6 num7 num8 num9 numlock pagedown pageup pause pgdn pgup playpause prevtrack print printscreen prntscrn prtsc prtscr return right scrolllock select separator shift shiftleft shiftright sleep stop subtract tab up volumedown volumemute volumeup win winleft winright yen command option optionleft optionright";

fn parse_key(value: &Value) -> Result<Key, ActionError> {
  let Some(raw) = value.as_str() else {
    return invalid("key must be an OSWorld keyboard key string");
  };
  let normalized = raw.to_ascii_lowercase();
  let single = normalized.chars().count() == 1 && (normalized.as_bytes()[0].is_ascii_graphic() || matches!(raw, "\t" | "\n" | "\r" | " "));
  let function = normalized
    .strip_prefix('f')
    .and_then(|digits| digits.parse::<u8>().ok())
    .is_some_and(|n| (1..=24).contains(&n) && normalized == format!("f{n}"));
  if single || function || NAMED_KEYS.split_ascii_whitespace().any(|key| key == normalized) {
    Ok(Key(normalized))
  } else {
    invalid("key is not in the pinned OSWorld keyboard key list")
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;

  #[test]
  fn normalizes_flat_and_nested_clicks_with_defaults() {
    let flat = parse_action(&json!({"action_type":"CLICK","x":10.5,"y":20,"num_clicks":3})).unwrap();
    let nested = parse_action(&json!({"action_type":"CLICK","parameters":{"x":10.5,"y":20,"num_clicks":3}})).unwrap();
    assert_eq!(flat, nested);
    assert_eq!(
      flat,
      Action::Click {
        button: Button::Left,
        position: Some(Point { x: 10.5, y: 20.0 }),
        count: ClickCount(3)
      }
    );
    assert_eq!(
      parse_action(&json!({"action_type":"RIGHT_CLICK"})).unwrap(),
      Action::Click {
        button: Button::Right,
        position: None,
        count: ClickCount(1)
      }
    );
    assert_eq!(
      parse_action(&json!({"action_type":"DOUBLE_CLICK","parameters":{}})).unwrap(),
      Action::Click {
        button: Button::Left,
        position: None,
        count: ClickCount(2)
      }
    );
    assert_eq!(parse_action(&json!({"action_type":"MOUSE_DOWN"})).unwrap(), Action::MouseDown(Button::Left));
    assert_eq!(parse_action(&json!({"action_type":"MOUSE_UP","button":"middle"})).unwrap(), Action::MouseUp(Button::Middle));
  }

  #[test]
  fn preserves_points_and_raw_scroll_steps() {
    let move_to = parse_action(&json!({"action_type":"MOVE_TO","parameters":{"x":1920,"y":1080}})).unwrap();
    assert_eq!(
      move_to,
      Action::MoveTo(Point {
        x: 1920.0,
        y: 1080.0
      })
    );
    assert_eq!(parse_action(&json!({"action_type":"DRAG_TO","x":3.25,"y":4.5})).unwrap(), Action::DragTo(Point { x: 3.25, y: 4.5 }));
    assert_eq!(parse_action(&json!({"action_type":"SCROLL","dy":-7})).unwrap(), Action::Scroll { dx: 0, dy: -7 });
    assert_eq!(parse_action(&json!({"action_type":"SCROLL","parameters":{"dx":2,"dy":5}})).unwrap(), Action::Scroll { dx: 2, dy: 5 });
  }

  #[test]
  fn validates_key_actions_and_control_signals() {
    assert_eq!(parse_action(&json!({"action_type":"TYPING","text":"中\n"})).unwrap(), Action::Typing("中\n".into()));
    assert_eq!(parse_action(&json!({"action_type":"PRESS","key":"F13"})).unwrap(), Action::Press(Key("f13".into())));
    assert_eq!(parse_action(&json!({"action_type":"KEY_DOWN","key":"CTRL"})).unwrap(), Action::KeyDown(Key("ctrl".into())));
    assert_eq!(parse_action(&json!({"action_type":"KEY_UP","key":"ctrl"})).unwrap(), Action::KeyUp(Key("ctrl".into())));
    assert_eq!(
      parse_action(&json!({"action_type":"HOTKEY","keys":["Ctrl","a"]})).unwrap(),
      Action::Hotkey(vec![Key("ctrl".into()), Key("a".into())])
    );
    for (value, expected) in [
      (json!("WAIT"), Action::Wait),
      (json!({"action_type":"DONE"}), Action::Done),
      (json!({"action_type":"FAIL","parameters":{}}), Action::Fail),
    ] {
      assert_eq!(parse_action(&value).unwrap(), expected);
    }
  }

  #[test]
  fn rejects_non_gui_source_and_execute() {
    assert_eq!(parse_action(&json!({"action_type":"EXECUTE","command":"touch /tmp/x"})), Err(ActionError::ExecuteNotGui));
    assert_eq!(parse_action(&json!("pyautogui.click()")), Err(ActionError::UnsupportedType("pyautogui.click()".into())));
    assert_eq!(
      parse_action(&json!({"action_type":"CLICK","parameters":{},"x":1})),
      Err(ActionError::InvalidParameters("mixed flat and nested parameters".into()))
    );
  }

  #[test]
  fn rejects_invalid_coordinates_buttons_counts_scroll_and_keys() {
    let bad = [
      json!({"action_type":"CLICK","x":1}),
      json!({"action_type":"MOVE_TO","x":1,"y":-1}),
      json!({"action_type":"DRAG_TO","x":1921,"y":0}),
      json!({"action_type":"CLICK","num_clicks":0}),
      json!({"action_type":"CLICK","num_clicks":4}),
      json!({"action_type":"CLICK","num_clicks":1.5}),
      json!({"action_type":"CLICK","button":"primary"}),
      json!({"action_type":"SCROLL","dx":1.2}),
      json!({"action_type":"SCROLL"}),
      json!({"action_type":"PRESS","key":"f25"}),
      json!({"action_type":"HOTKEY","keys":[]}),
      json!({"action_type":"HOTKEY","keys":["ctrl",3]}),
      json!({"action_type":"DONE","parameters":{"text":"ignored"}}),
      json!({"action_type":"TYPING","text":"hello","extra":1}),
    ];
    for value in bad {
      assert!(parse_action(&value).is_err(), "accepted {value}");
    }
  }
}
