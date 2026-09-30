//! Windows physical-console adapter for the shared Device policy.
//!
//! The installed LocalSystem service delegates exact-session input to its
//! selected-session worker and reads back the console state.

use auv::devices::{DeviceEntryErrorReason, UserSession, UserSessionConnectionKind, UserSessionLockState};
use auv_driver_windows::device_session::{ConsoleLockState, ConsoleSession, ConsoleSessionError, observe_console};
use auv_driver_windows::device_unlock_host::{HostError, lock_with_worker, unlock_enrolled_with_worker};
use auv_driver_windows::device_unlock_vault::{VaultError, verify_while_locked};

use super::policy::{ObservedSession, SessionHost};

pub(super) struct WindowsSessionHost;

impl SessionHost for WindowsSessionHost {
  fn sessions(&self) -> Result<Vec<ObservedSession>, DeviceEntryErrorReason> {
    Ok(observe_console().map_err(session_error)?.map(|session| vec![observed(&session)]).unwrap_or_default())
  }

  async fn verify_pending_credential(
    &self,
    selected: &ObservedSession,
    authorize_effect: &(dyn Fn() -> Result<(), DeviceEntryErrorReason> + Send + Sync),
  ) -> Result<(), DeviceEntryErrorReason> {
    let session = selected_locked_console(selected)?;
    authorize_effect()?;
    verify_while_locked(&session).map_err(vault_error)
  }

  async fn verify_ready_credential(
    &self,
    _selected: &ObservedSession,
    _authorize_effect: &(dyn Fn() -> Result<(), DeviceEntryErrorReason> + Send + Sync),
  ) -> Result<(), DeviceEntryErrorReason> {
    // Ready means the LocalSystem host retrieved the PIN in a prior locked
    // attempt. The worker retrieves and submits it again during delivery.
    // TODO(device-entry-windows-rotation): A confirmed OS rejection signal is
    // needed before suspending a rotated PIN; the worker currently reports an
    // unverified result instead. Reopen after the installed Winlogon gate.
    Ok(())
  }

  fn unlock_locked(&self, selected: &ObservedSession) -> Result<(), DeviceEntryErrorReason> {
    let session = selected_locked_console(selected)?;
    let after = unlock_enrolled_with_worker(&session).map_err(host_error)?;

    if !session.same_login(&after) || after.lock_state != ConsoleLockState::Usable {
      return Err(DeviceEntryErrorReason::OutcomeUnverified);
    }

    Ok(())
  }

  fn lock_usable(&self, selected: &ObservedSession) -> Result<(), DeviceEntryErrorReason> {
    let current = observe_console().map_err(session_error)?.ok_or(DeviceEntryErrorReason::StaleSession)?;

    if selected.public.selector != current.selector()
      || selected.public.user != account_name(&current)
      || selected.os_account_id != current.account_sid
      || selected.public.lock_state != UserSessionLockState::Usable
      || current.lock_state != ConsoleLockState::Usable
    {
      return Err(DeviceEntryErrorReason::StaleSession);
    }

    let after = lock_with_worker(&current).map_err(host_error)?;

    if !current.same_login(&after) || after.lock_state != ConsoleLockState::Locked {
      return Err(DeviceEntryErrorReason::OutcomeUnverified);
    }

    Ok(())
  }
}

fn selected_locked_console(selected: &ObservedSession) -> Result<ConsoleSession, DeviceEntryErrorReason> {
  let current = observe_console().map_err(session_error)?.ok_or(DeviceEntryErrorReason::StaleSession)?;

  if selected_matches_locked_console(selected, &current) {
    Ok(current)
  } else {
    Err(DeviceEntryErrorReason::StaleSession)
  }
}

fn selected_matches_locked_console(selected: &ObservedSession, current: &ConsoleSession) -> bool {
  selected.public.selector == current.selector()
    && selected.public.user == account_name(current)
    && selected.os_account_id == current.account_sid
    && selected.public.lock_state == UserSessionLockState::Locked
    && current.lock_state == ConsoleLockState::Locked
}

pub(super) fn account_name(session: &ConsoleSession) -> String {
  if session.domain.is_empty() {
    session.user.clone()
  } else {
    format!(r"{}\{}", session.domain, session.user)
  }
}

fn observed(session: &ConsoleSession) -> ObservedSession {
  ObservedSession {
    public: UserSession {
      selector: session.selector(),
      user: account_name(session),
      lock_state: match session.lock_state {
        ConsoleLockState::Locked => UserSessionLockState::Locked,
        ConsoleLockState::Usable => UserSessionLockState::Usable,
        ConsoleLockState::Unknown => UserSessionLockState::Unknown,
      },
      connection_kind: UserSessionConnectionKind::Physical,
      seat: Some("console".into()),
    },
    os_account_id: session.account_sid.clone(),
  }
}

fn session_error(error: ConsoleSessionError) -> DeviceEntryErrorReason {
  match error {
    ConsoleSessionError::ConsoleTransition => DeviceEntryErrorReason::StaleSession,
    ConsoleSessionError::InconsistentRecord | ConsoleSessionError::UnsupportedPlatform => DeviceEntryErrorReason::UnsupportedOsState,
    ConsoleSessionError::QueryFailed(_) | ConsoleSessionError::IdentityUnverified => DeviceEntryErrorReason::ServiceUnavailable,
  }
}

fn vault_error(error: VaultError) -> DeviceEntryErrorReason {
  match error {
    VaultError::NotEnrolled => DeviceEntryErrorReason::Unenrolled,
    VaultError::NotLocked | VaultError::InvalidAccount => DeviceEntryErrorReason::StaleSession,
    VaultError::Unavailable | VaultError::Permissions | VaultError::RetrievalFailed => DeviceEntryErrorReason::ServiceUnavailable,
  }
}

fn host_error(error: HostError) -> DeviceEntryErrorReason {
  match error {
    HostError::StaleSession | HostError::NotLocked => DeviceEntryErrorReason::StaleSession,
    HostError::Unverified => DeviceEntryErrorReason::OutcomeUnverified,
    HostError::Unavailable | HostError::WorkerUnavailable | HostError::WorkerIdentity | HostError::TransferFailed => {
      DeviceEntryErrorReason::ServiceUnavailable
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn session(state: ConsoleLockState) -> ConsoleSession {
    ConsoleSession {
      session_id: 2,
      logon_time: 123,
      account_sid: "S-1-5-21-123-456-789-1001".into(),
      domain: "DESKTOP".into(),
      user: "neko".into(),
      lock_state: state,
    }
  }

  #[test]
  fn selected_login_requires_exact_session_sid_name_and_lock() {
    let current = session(ConsoleLockState::Locked);
    let selected = observed(&current);

    assert_eq!(selected.public.user, r"DESKTOP\neko");
    assert!(selected_matches_locked_console(&selected, &current));

    let mut different = current.clone();
    different.logon_time += 1;

    assert!(!selected_matches_locked_console(&selected, &different));

    different = current.clone();
    different.account_sid = "S-1-5-21-123-456-789-1002".into();

    assert!(!selected_matches_locked_console(&selected, &different));
    assert!(!selected_matches_locked_console(&selected, &session(ConsoleLockState::Usable)));
  }
}
