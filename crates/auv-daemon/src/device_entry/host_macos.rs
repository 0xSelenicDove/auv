//! Remote policy adapter for the physical macOS console and signed Aqua host.
//!
//! Only an OS account ID and exact login-session selector cross the daemon's
//! helper IPC. Credential retrieval and delivery remain inside the helper.

use auv::devices::{DeviceEntryErrorReason, UserSessionLockState};
use auv_device_helper_macos::HostError;

use super::enrollment_macos::{Account, resolve_uid};
use super::macos::current_console_session;
use super::policy::{ObservedSession, UnlockHost};

pub(super) struct MacosUnlockHost;

impl MacosUnlockHost {
  pub(super) fn new() -> Self {
    Self
  }
}

impl UnlockHost for MacosUnlockHost {
  fn sessions(&self) -> Result<Vec<ObservedSession>, DeviceEntryErrorReason> {
    let Some(console) = current_console_session()? else {
      return Ok(Vec::new());
    };
    let account = resolve_uid(console.uid).map_err(|_| DeviceEntryErrorReason::ServiceUnavailable)?;
    if account.uid == 0 || account.name != console.session.user {
      return Err(DeviceEntryErrorReason::UnsupportedOsState);
    }
    Ok(vec![ObservedSession {
      public: console.session,
      os_account_id: account.id,
    }])
  }

  async fn verify_pending_credential(
    &self,
    selected: &ObservedSession,
    authorize_effect: &(dyn Fn() -> Result<(), DeviceEntryErrorReason> + Send + Sync),
  ) -> Result<(), DeviceEntryErrorReason> {
    let account = self.selected_locked_account(selected)?;
    authorize_effect()?;
    auv_device_helper_macos::probe_locked(&account.home, account.uid, &selected.public.selector).map_err(map_host_error)
  }

  async fn verify_ready_credential(
    &self,
    _selected: &ObservedSession,
    authorize_effect: &(dyn Fn() -> Result<(), DeviceEntryErrorReason> + Send + Sync),
  ) -> Result<(), DeviceEntryErrorReason> {
    // A Ready macOS enrollment already passed the locked Keychain probe. The
    // installed helper retrieves and submits it during the unlock attempt.
    authorize_effect()
  }

  fn unlock_locked(&self, selected: &ObservedSession) -> Result<(), DeviceEntryErrorReason> {
    let account = self.selected_locked_account(selected)?;
    // The helper makes its own fresh locked-session observation before and
    // after Keychain retrieval, then reads back the same usable session.
    // Policy performs another independent observation after this returns.
    auv_device_helper_macos::unlock(&account.home, account.uid, &selected.public.selector).map_err(map_host_error)
  }
}

impl MacosUnlockHost {
  fn selected_locked_account(&self, selected: &ObservedSession) -> Result<Account, DeviceEntryErrorReason> {
    let current = self.sessions()?.into_iter().next().ok_or(DeviceEntryErrorReason::StaleSession)?;
    validate_selected_locked(selected, &current)?;
    let uid =
      selected.os_account_id.strip_prefix("uid:").and_then(|value| value.parse::<u32>().ok()).ok_or(DeviceEntryErrorReason::StaleSession)?;
    let account = resolve_uid(uid).map_err(|_| DeviceEntryErrorReason::ServiceUnavailable)?;
    if account.uid == 0 || account.id != selected.os_account_id || account.name != selected.public.user {
      return Err(DeviceEntryErrorReason::StaleSession);
    }
    Ok(account)
  }
}

fn validate_selected_locked(selected: &ObservedSession, current: &ObservedSession) -> Result<(), DeviceEntryErrorReason> {
  if selected.public.selector != current.public.selector
    || selected.public.user != current.public.user
    || selected.os_account_id != current.os_account_id
  {
    return Err(DeviceEntryErrorReason::StaleSession);
  }
  if current.public.lock_state != UserSessionLockState::Locked {
    return Err(DeviceEntryErrorReason::StaleSession);
  }
  Ok(())
}

fn map_host_error(error: HostError) -> DeviceEntryErrorReason {
  match error {
    HostError::StaleSession | HostError::NotLocked => DeviceEntryErrorReason::StaleSession,
    HostError::OutcomeUnverified | HostError::InputUnavailable => DeviceEntryErrorReason::OutcomeUnverified,
    HostError::VaultUnavailable => DeviceEntryErrorReason::Unenrolled,
    HostError::Unavailable | HostError::Unauthorized | HostError::InvalidRequest => DeviceEntryErrorReason::ServiceUnavailable,
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use auv::devices::{UserSession, UserSessionConnectionKind};

  fn observed(selector: &str, user: &str, id: &str, lock_state: UserSessionLockState) -> ObservedSession {
    ObservedSession {
      public: UserSession {
        selector: selector.to_owned(),
        user: user.to_owned(),
        lock_state,
        connection_kind: UserSessionConnectionKind::Physical,
        seat: Some("console".to_owned()),
      },
      os_account_id: id.to_owned(),
    }
  }

  #[test]
  fn locked_selection_requires_same_login_instance_and_uid() {
    let selected = observed("macos:uuid-a", "neko", "uid:501", UserSessionLockState::Locked);
    assert_eq!(validate_selected_locked(&selected, &observed("macos:uuid-a", "neko", "uid:501", UserSessionLockState::Locked)), Ok(()));
    assert_eq!(
      validate_selected_locked(&selected, &observed("macos:uuid-b", "neko", "uid:501", UserSessionLockState::Locked)),
      Err(DeviceEntryErrorReason::StaleSession)
    );
    assert_eq!(
      validate_selected_locked(&selected, &observed("macos:uuid-a", "neko", "uid:502", UserSessionLockState::Locked)),
      Err(DeviceEntryErrorReason::StaleSession)
    );
    assert_eq!(
      validate_selected_locked(&selected, &observed("macos:uuid-a", "neko", "uid:501", UserSessionLockState::Usable)),
      Err(DeviceEntryErrorReason::StaleSession)
    );
  }

  #[test]
  fn helper_outcomes_do_not_infer_credential_rejection() {
    assert_eq!(map_host_error(HostError::VaultUnavailable), DeviceEntryErrorReason::Unenrolled);
    assert_eq!(map_host_error(HostError::InputUnavailable), DeviceEntryErrorReason::OutcomeUnverified);
    assert_eq!(map_host_error(HostError::StaleSession), DeviceEntryErrorReason::StaleSession);
  }
}
