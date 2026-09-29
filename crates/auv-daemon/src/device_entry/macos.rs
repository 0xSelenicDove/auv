//! Device policy mapping for the physical macOS console observation.

use std::thread;
use std::time::{Duration, Instant};

use auv::devices::{DeviceEntryErrorReason, UserSession, UserSessionConnectionKind, UserSessionLockState};
use auv_driver_macos::device_session::{ConsoleSession as NativeConsoleSession, ObserveError, observe_console};

/// One logged-in console identity and the public facts derived from it.
/// The native observation retains UID and the exact session identity for
/// target-local enrollment and same-session readback.
#[derive(Debug)]
// TODO(device-entry-legacy-observer): The native identity is still used by the
// older delivery proof below; remove that proof after its replacement review.
#[allow(dead_code)]
pub(super) struct ConsoleSession {
  pub session: UserSession,
  pub uid: u32,
  native: NativeConsoleSession,
}

/// Observe one physical, already logged-in user through the shared driver
/// parser. This remains a fresh IORegistry read on each call.
pub(super) fn current_console_session() -> Result<Option<ConsoleSession>, DeviceEntryErrorReason> {
  let observed = observe_console().map_err(map_observation)?;
  Ok(observed.map(|native| {
    let locked = native.is_locked();
    let uid = native.uid();
    let session = UserSession {
      selector: native.selector().to_owned(),
      user: native.user().to_owned(),
      lock_state: if locked {
        UserSessionLockState::Locked
      } else {
        UserSessionLockState::Usable
      },
      connection_kind: UserSessionConnectionKind::Physical,
      seat: Some("console".to_owned()),
    };
    ConsoleSession {
      session,
      uid,
      native,
    }
  }))
}

fn map_observation(error: ObserveError) -> DeviceEntryErrorReason {
  match error {
    ObserveError::Unavailable => DeviceEntryErrorReason::ServiceUnavailable,
    ObserveError::UnknownState => DeviceEntryErrorReason::UnsupportedOsState,
    ObserveError::Ambiguous => DeviceEntryErrorReason::AmbiguousUser,
  }
}

/// Attempt one delivery to a selected existing locked console session.
///
/// The installed graphical host owns `deliver` and its target-local credential.
/// A successful native posting call is only an attempt; the effect requires a
/// fresh IORegistry read of this exact session and account. No secret is passed
/// to the observer or included in any result.
// TODO(device-entry-legacy-observer): Production unlock uses MacosUnlockHost;
// remove this delivery proof after its replacement review.
#[allow(dead_code)]
pub(super) fn unlock_user_session(
  selected: &ConsoleSession,
  deliver: impl FnOnce(u32) -> Result<(), DeviceEntryErrorReason>,
) -> Result<(), DeviceEntryErrorReason> {
  let before = selected_console_session(selected)?;
  if before.session.lock_state != UserSessionLockState::Locked {
    return Err(DeviceEntryErrorReason::UnsupportedOsState);
  }
  deliver(selected.uid)?;

  let deadline = Instant::now() + Duration::from_secs(10);
  loop {
    let observed = selected_console_session(selected)?;
    if observed.session.lock_state == UserSessionLockState::Usable {
      return Ok(());
    }
    if Instant::now() >= deadline {
      return Err(DeviceEntryErrorReason::OutcomeUnverified);
    }
    thread::sleep(Duration::from_millis(100));
  }
}

fn selected_console_session(selected: &ConsoleSession) -> Result<ConsoleSession, DeviceEntryErrorReason> {
  let current = current_console_session()?.ok_or(DeviceEntryErrorReason::StaleSession)?;
  if !selected.native.same_identity(&current.native) {
    return Err(DeviceEntryErrorReason::StaleSession);
  }
  Ok(current)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn maps_observer_ambiguity_and_unknown_state_to_device_errors() {
    assert_eq!(map_observation(ObserveError::Ambiguous), DeviceEntryErrorReason::AmbiguousUser);
    assert_eq!(map_observation(ObserveError::UnknownState), DeviceEntryErrorReason::UnsupportedOsState);
    assert_eq!(map_observation(ObserveError::Unavailable), DeviceEntryErrorReason::ServiceUnavailable);
  }
}
