//! GNOME Wayland host facts for an existing logged-in user session.
//!
//! The driver is scoped to the daemon's effective UID. A multi-account host
//! needs an authorized per-user worker before the Device policy can expose it.

use auv::devices::{DeviceEntryErrorReason, UserSession, UserSessionConnectionKind, UserSessionLockState};
use auv_driver_linux::device_unlock::{self, GnomeSession, LockState, UnlockError};

pub(super) struct LinuxSession {
  pub session: UserSession,
  pub native: GnomeSession,
}

/// Takes one inventory snapshot under the installed host's OS identity.
pub(super) fn current_user_sessions() -> Result<Vec<LinuxSession>, DeviceEntryErrorReason> {
  let sessions = device_unlock::list_user_sessions()
    .map_err(map_error)?
    .into_iter()
    .map(|native| {
      let session = UserSession {
        selector: native.selector(),
        user: native.user.clone(),
        lock_state: match native.lock_state {
          LockState::Locked => UserSessionLockState::Locked,
          LockState::Usable => UserSessionLockState::Usable,
          LockState::Unknown => UserSessionLockState::Unknown,
        },
        connection_kind: UserSessionConnectionKind::Physical,
        seat: Some(native.seat.clone()),
      };
      LinuxSession { session, native }
    })
    .collect();
  Ok(sessions)
}

pub(super) fn map_error(error: UnlockError) -> DeviceEntryErrorReason {
  match error {
    UnlockError::ServiceUnavailable => DeviceEntryErrorReason::ServiceUnavailable,
    UnlockError::StaleSession => DeviceEntryErrorReason::StaleSession,
    UnlockError::UnsupportedOsState => DeviceEntryErrorReason::UnsupportedOsState,
    UnlockError::OutcomeUnverified => DeviceEntryErrorReason::OutcomeUnverified,
  }
}
