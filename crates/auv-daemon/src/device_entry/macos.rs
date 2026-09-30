//! Device policy mapping for the physical macOS console observation.

use auv::devices::{DeviceEntryErrorReason, UserSession, UserSessionConnectionKind, UserSessionLockState};
use auv_driver_macos::device_session::{ObserveError, observe_console};

/// One logged-in console identity and the public facts derived from it.
/// The native observation retains UID and the exact session identity for
/// target-local enrollment and same-session readback.
#[derive(Debug)]
pub(super) struct ConsoleSession {
  pub session: UserSession,
  pub uid: u32,
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

    ConsoleSession { session, uid }
  }))
}

fn map_observation(error: ObserveError) -> DeviceEntryErrorReason {
  match error {
    ObserveError::Unavailable => DeviceEntryErrorReason::ServiceUnavailable,
    ObserveError::UnknownState => DeviceEntryErrorReason::UnsupportedOsState,
    ObserveError::Ambiguous => DeviceEntryErrorReason::AmbiguousUser,
  }
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
