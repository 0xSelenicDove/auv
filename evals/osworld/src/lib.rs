//! Validated OSWorld `computer_13` actions and AUV Runner delivery.
//!
//! This is a benchmark harness boundary, not an AUV driver or agent policy.

mod action;
mod executor;

pub use action::{Action, ActionError, Button, ClickCount, Key, Point, parse_action};
pub use executor::{ActionExecutor, ActionOutcome, ControlSignal, ExecuteError};
