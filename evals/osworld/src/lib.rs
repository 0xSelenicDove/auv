//! Validated OSWorld `computer_13` action data, before any AUV GUI delivery.
//!
//! This is a benchmark harness boundary, not an AUV driver or agent policy.
//! TODO(osworld-delivery): GUI execution is intentionally absent from this
//! parser-only slice; add it after the receiver-calibrated scroll mapping and
//! persistent Runner ownership contract are validated.

mod action;

pub use action::{Action, ActionError, Button, ClickCount, Key, Point, parse_action};
