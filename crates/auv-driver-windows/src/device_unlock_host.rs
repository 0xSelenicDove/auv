//! LocalSystem console worker placement and one-shot, local-only secret transfer.
//!
//! This is an internal host primitive, not a service registration. The caller
//! must run under the installed LocalSystem service identity. No remote request
//! may carry a secret or choose the worker executable.

use crate::device_session::ConsoleSession;

#[derive(Debug, thiserror::Error)]
pub enum HostError {
  #[error("the Windows unlock host is unavailable")]
  Unavailable,
  #[error("the selected console login changed")]
  StaleSession,
  #[error("the selected console is not locked")]
  NotLocked,
  #[error("the worker could not be started in the selected console session")]
  WorkerUnavailable,
  #[error("the local worker identity could not be verified")]
  WorkerIdentity,
  #[error("the local credential transfer failed")]
  TransferFailed,
  #[error("the console unlock outcome could not be verified")]
  Unverified,
}

/// Fixed, non-secret status from a console-session worker preflight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum PreflightCode {
  ReadyDefault = 0,
  ReadyWinlogon = 1,
  TransitionReady = 3,
  StaleSession = 20,
  NotLocked = 21,
  WorkerIdentity = 22,
  DesktopUnavailable = 23,
  UnsupportedOsState = 24,
  HostIdentity = 25,
  WorkerUnavailable = 26,
  WorkerUnverified = 27,
  ConsoleUnavailable = 28,
  PipeConnectFailed = 29,
  PipeTransferFailed = 30,
  PipePayloadMismatch = 31,
  PipeUnavailable = 32,
  TransitionInitialDesktop = 40,
  TransitionReturnRejected = 41,
  TransitionWinlogonUnavailable = 42,
  TransitionWinlogonBindUnavailable = 43,
}

impl PreflightCode {
  pub fn exit_code(self) -> u32 {
    self as u32
  }

  fn from_exit_code(code: u32) -> Option<Self> {
    Some(match code {
      0 => Self::ReadyDefault,
      1 => Self::ReadyWinlogon,
      3 => Self::TransitionReady,
      20 => Self::StaleSession,
      21 => Self::NotLocked,
      22 => Self::WorkerIdentity,
      23 => Self::DesktopUnavailable,
      24 => Self::UnsupportedOsState,
      25 => Self::HostIdentity,
      26 => Self::WorkerUnavailable,
      27 => Self::WorkerUnverified,
      28 => Self::ConsoleUnavailable,
      29 => Self::PipeConnectFailed,
      30 => Self::PipeTransferFailed,
      31 => Self::PipePayloadMismatch,
      32 => Self::PipeUnavailable,
      40 => Self::TransitionInitialDesktop,
      41 => Self::TransitionReturnRejected,
      42 => Self::TransitionWinlogonUnavailable,
      43 => Self::TransitionWinlogonBindUnavailable,
      _ => return None,
    })
  }

  pub fn as_str(self) -> &'static str {
    match self {
      Self::ReadyDefault => "ready_default",
      Self::ReadyWinlogon => "ready_winlogon",
      Self::TransitionReady => "transition_ready",
      Self::StaleSession => "stale_session",
      Self::NotLocked => "not_locked",
      Self::WorkerIdentity => "worker_identity",
      Self::DesktopUnavailable => "desktop_unavailable",
      Self::UnsupportedOsState => "unsupported_os_state",
      Self::HostIdentity => "host_identity",
      Self::WorkerUnavailable => "worker_unavailable",
      Self::WorkerUnverified => "worker_unverified",
      Self::ConsoleUnavailable => "console_unavailable",
      Self::PipeConnectFailed => "pipe_connect_failed",
      Self::PipeTransferFailed => "pipe_transfer_failed",
      Self::PipePayloadMismatch => "pipe_payload_mismatch",
      Self::PipeUnavailable => "pipe_unavailable",
      Self::TransitionInitialDesktop => "transition_initial_desktop",
      Self::TransitionReturnRejected => "transition_return_rejected",
      Self::TransitionWinlogonUnavailable => "transition_winlogon_unavailable",
      Self::TransitionWinlogonBindUnavailable => "transition_winlogon_bind_unavailable",
    }
  }
}

/// Local-only Return diagnostic detail. The paired Device API never exposes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransitionDiagnostic {
  pub code: PreflightCode,
  pub inserted: Option<u8>,
  pub win32_error: Option<u32>,
}

impl TransitionDiagnostic {
  fn code(code: PreflightCode) -> Self {
    Self {
      code,
      inserted: None,
      win32_error: None,
    }
  }
}

const RETURN_DETAIL_TAG: u32 = 0xA000_0000;
const RETURN_DETAIL_MASK: u32 = 0xFE00_0000;
const RETURN_DETAIL_ERROR_MAX: u32 = 0x00FF_FFFF;

fn encode_return_rejection(inserted: u8, win32_error: u32) -> u32 {
  if inserted > 1 || win32_error > RETURN_DETAIL_ERROR_MAX {
    return PreflightCode::TransitionReturnRejected.exit_code();
  }
  RETURN_DETAIL_TAG | ((inserted as u32) << 24) | win32_error
}

fn full_transition_report(exit: u32) -> TransitionDiagnostic {
  if exit & RETURN_DETAIL_MASK == RETURN_DETAIL_TAG {
    return TransitionDiagnostic {
      code: PreflightCode::TransitionReturnRejected,
      inserted: Some(((exit >> 24) & 1) as u8),
      win32_error: Some(exit & RETURN_DETAIL_ERROR_MAX),
    };
  }
  let code = match PreflightCode::from_exit_code(exit) {
    Some(
      code @ (PreflightCode::TransitionReady
      | PreflightCode::StaleSession
      | PreflightCode::NotLocked
      | PreflightCode::WorkerIdentity
      | PreflightCode::ConsoleUnavailable
      | PreflightCode::UnsupportedOsState
      | PreflightCode::TransitionInitialDesktop
      | PreflightCode::TransitionReturnRejected
      | PreflightCode::TransitionWinlogonUnavailable
      | PreflightCode::TransitionWinlogonBindUnavailable),
    ) => code,
    _ => PreflightCode::WorkerUnverified,
  };
  TransitionDiagnostic::code(code)
}

/// Read the selected account's target-local vault entry under the installed
/// LocalSystem identity, then start one console worker. The worker executable
/// is resolved beside this installed service process, never from a request.
// TODO(device-unlock-windows-service): Register this only after the installed
// LocalSystem service, peer-authenticated enrollment, and live locked gate pass.
pub fn unlock_enrolled_with_worker(target: &ConsoleSession) -> Result<ConsoleSession, HostError> {
  native::checked_target(target)?;
  let credential = crate::device_unlock_vault::retrieve(&target.account_sid).map_err(|_| HostError::Unavailable)?;
  native::unlock_with_worker(target, &credential)
}

/// Launch the same console-session LocalSystem worker without opening the
/// credential vault, creating a secret pipe, or posting any input.
pub fn preflight_locked_with_worker(target: &ConsoleSession) -> Result<PreflightCode, HostError> {
  native::checked_target(target)?;
  native::preflight_with_worker(target)
}

/// Entrypoint used only by the separately installed Windows worker executable.
/// Worker arguments contain a session identity and random pipe name, no secret.
pub fn run_worker(pipe_name: &str, session_id: u32, logon_time: i64, account_sid: &str) -> Result<(), HostError> {
  native::run_worker(pipe_name, session_id, logon_time, account_sid)
}

/// Entrypoint for the installed worker's read-only `--preflight` mode.
pub fn run_preflight_worker(session_id: u32, logon_time: i64, account_sid: &str) -> PreflightCode {
  native::run_preflight_worker(session_id, logon_time, account_sid)
}

/// Entry for a one-shot diagnostic host launched as LocalSystem in Session 0.
/// It reports a fixed code and never reads the enrollment vault.
pub fn run_preflight_host() -> PreflightCode {
  native::run_preflight_host()
}

/// One-shot pipe transport diagnostic with a fixed public payload. It never
/// opens the enrollment vault and cannot submit credential input.
pub fn run_pipe_preflight_host() -> PreflightCode {
  native::run_pipe_preflight_host()
}

/// Entrypoint for the installed worker's public-payload pipe diagnostic.
pub fn run_pipe_preflight_worker(pipe_name: &str, session_id: u32, logon_time: i64, account_sid: &str) -> PreflightCode {
  native::run_pipe_preflight_worker(pipe_name, session_id, logon_time, account_sid)
}

/// One-shot, non-secret Default-to-Winlogon transition diagnostic host.
pub fn run_transition_preflight_host() -> PreflightCode {
  native::run_transition_preflight_host()
}

/// Entrypoint for the diagnostic console worker's one-Return transition.
pub fn run_transition_preflight_worker(session_id: u32, logon_time: i64, account_sid: &str) -> PreflightCode {
  native::run_transition_preflight_worker(session_id, logon_time, account_sid)
}

/// Diagnostic A/B mode using the prior prototype's full desktop access mask.
/// Only one harmless Return pair is possible; the result stays target-local.
pub fn run_full_transition_preflight_host() -> TransitionDiagnostic {
  native::run_full_transition_preflight_host()
}

/// Child entry for the full-access Return diagnostic; returns a packed process
/// exit status with bounded, non-secret SendInput telemetry on rejection.
pub fn run_full_transition_preflight_worker(session_id: u32, logon_time: i64, account_sid: &str) -> u32 {
  native::run_full_transition_preflight_worker(session_id, logon_time, account_sid)
}

#[cfg(not(target_os = "windows"))]
mod native {
  use super::{HostError, PreflightCode, TransitionDiagnostic};
  use crate::device_session::ConsoleSession;

  pub(super) fn unlock_with_worker(_: &ConsoleSession, _: &str) -> Result<ConsoleSession, HostError> {
    Err(HostError::Unavailable)
  }

  pub(super) fn checked_target(_: &ConsoleSession) -> Result<(), HostError> {
    Err(HostError::Unavailable)
  }

  pub(super) fn run_worker(_: &str, _: u32, _: i64, _: &str) -> Result<(), HostError> {
    Err(HostError::Unavailable)
  }

  pub(super) fn preflight_with_worker(_: &ConsoleSession) -> Result<PreflightCode, HostError> {
    Err(HostError::Unavailable)
  }

  pub(super) fn run_preflight_worker(_: u32, _: i64, _: &str) -> PreflightCode {
    PreflightCode::UnsupportedOsState
  }

  pub(super) fn run_preflight_host() -> PreflightCode {
    PreflightCode::UnsupportedOsState
  }

  pub(super) fn run_pipe_preflight_host() -> PreflightCode {
    PreflightCode::UnsupportedOsState
  }

  pub(super) fn run_pipe_preflight_worker(_: &str, _: u32, _: i64, _: &str) -> PreflightCode {
    PreflightCode::UnsupportedOsState
  }

  pub(super) fn run_transition_preflight_host() -> PreflightCode {
    PreflightCode::UnsupportedOsState
  }

  pub(super) fn run_transition_preflight_worker(_: u32, _: i64, _: &str) -> PreflightCode {
    PreflightCode::UnsupportedOsState
  }

  pub(super) fn run_full_transition_preflight_host() -> TransitionDiagnostic {
    TransitionDiagnostic::code(PreflightCode::UnsupportedOsState)
  }

  pub(super) fn run_full_transition_preflight_worker(_: u32, _: i64, _: &str) -> u32 {
    PreflightCode::UnsupportedOsState.exit_code()
  }
}

#[cfg(test)]
mod tests {
  use super::{PreflightCode, RETURN_DETAIL_ERROR_MAX, TransitionDiagnostic, encode_return_rejection, full_transition_report};

  #[test]
  fn preflight_worker_exit_codes_have_one_fixed_meaning() {
    let cases = [
      (0, PreflightCode::ReadyDefault),
      (1, PreflightCode::ReadyWinlogon),
      (3, PreflightCode::TransitionReady),
      (20, PreflightCode::StaleSession),
      (21, PreflightCode::NotLocked),
      (22, PreflightCode::WorkerIdentity),
      (23, PreflightCode::DesktopUnavailable),
      (24, PreflightCode::UnsupportedOsState),
      (25, PreflightCode::HostIdentity),
      (26, PreflightCode::WorkerUnavailable),
      (27, PreflightCode::WorkerUnverified),
      (28, PreflightCode::ConsoleUnavailable),
      (29, PreflightCode::PipeConnectFailed),
      (30, PreflightCode::PipeTransferFailed),
      (31, PreflightCode::PipePayloadMismatch),
      (32, PreflightCode::PipeUnavailable),
      (40, PreflightCode::TransitionInitialDesktop),
      (41, PreflightCode::TransitionReturnRejected),
      (42, PreflightCode::TransitionWinlogonUnavailable),
      (43, PreflightCode::TransitionWinlogonBindUnavailable),
    ];
    for (exit, expected) in cases {
      assert_eq!(PreflightCode::from_exit_code(exit), Some(expected));
      assert_eq!(expected.exit_code(), exit);
    }
    assert_eq!(PreflightCode::from_exit_code(2), None);
    assert_eq!(PreflightCode::from_exit_code(u32::MAX), None);
  }

  #[test]
  fn full_transition_exit_report_rejects_unexpected_success_and_bounds_telemetry() {
    assert_eq!(
      full_transition_report(encode_return_rejection(0, 5)),
      TransitionDiagnostic {
        code: PreflightCode::TransitionReturnRejected,
        inserted: Some(0),
        win32_error: Some(5),
      }
    );
    assert_eq!(full_transition_report(encode_return_rejection(1, 0)).inserted, Some(1));
    assert_eq!(full_transition_report(encode_return_rejection(2, 5)).inserted, None);
    assert_eq!(full_transition_report(encode_return_rejection(0, RETURN_DETAIL_ERROR_MAX + 1)).win32_error, None);
    for unexpected in [0, 1, 2, u32::MAX] {
      assert_eq!(full_transition_report(unexpected).code, PreflightCode::WorkerUnverified);
    }
    assert_eq!(full_transition_report(PreflightCode::TransitionReady.exit_code()).code, PreflightCode::TransitionReady);
  }
}

#[cfg(target_os = "windows")]
mod native {
  use std::ffi::c_void;
  use std::mem::size_of;
  use std::os::windows::ffi::OsStrExt;
  use std::path::Path;
  use std::time::Duration;

  use windows::Win32::Foundation::{
    CloseHandle, ERROR_IO_PENDING, ERROR_PIPE_CONNECTED, GENERIC_READ, HANDLE, HLOCAL, LocalFree, WAIT_OBJECT_0,
  };
  use windows::Win32::Security::Authorization::{ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1};
  use windows::Win32::Security::Cryptography::{BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom};
  use windows::Win32::Security::{
    DuplicateTokenEx, SECURITY_ATTRIBUTES, SecurityImpersonation, SetTokenInformation, TOKEN_ALL_ACCESS, TOKEN_DUPLICATE, TOKEN_QUERY,
    TokenPrimary, TokenSessionId,
  };
  use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, FILE_SHARE_MODE, OPEN_EXISTING, PIPE_ACCESS_OUTBOUND, ReadFile,
    WriteFile,
  };
  use windows::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
  use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, GetNamedPipeClientProcessId, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE,
    PIPE_WAIT,
  };
  use windows::Win32::System::Threading::{
    CreateEventW, CreateProcessAsUserW, GetCurrentProcess, GetExitCodeProcess, OpenProcessToken, PROCESS_INFORMATION, STARTUPINFOW,
    TerminateProcess, WaitForSingleObject,
  };
  use windows::core::{PCWSTR, PWSTR};
  use zeroize::Zeroizing;

  use super::{HostError, PreflightCode, TransitionDiagnostic, encode_return_rejection, full_transition_report};
  use crate::device_session::{
    ConsoleLockState, ConsoleSession, ConsoleUnlockError, InputDesktop, TransitionDesktopAccess, TransitionPreflightResult, observe_console,
    preflight_default_to_winlogon, preflight_locked_worker, unlock_existing_session, verify_local_system_process_in_session,
  };

  struct OwnedHandle(HANDLE);
  impl Drop for OwnedHandle {
    fn drop(&mut self) {
      // SAFETY: This guard exclusively owns one successful Win32 handle.
      let _ = unsafe { CloseHandle(self.0) };
    }
  }

  struct SecurityDescriptor(windows::Win32::Security::PSECURITY_DESCRIPTOR);
  impl Drop for SecurityDescriptor {
    fn drop(&mut self) {
      // SAFETY: ConvertStringSecurityDescriptorToSecurityDescriptorW allocated
      // this descriptor with LocalAlloc; it is freed after pipe creation.
      unsafe { LocalFree(HLOCAL(self.0.0)) };
    }
  }

  struct Worker {
    process: OwnedHandle,
    _thread: OwnedHandle,
    pid: u32,
    completed: bool,
  }

  enum WorkerMode<'a> {
    Unlock(&'a str),
    Preflight,
    PipePreflight(&'a str),
    TransitionPreflight,
    FullTransitionPreflight,
  }

  const PIPE_PAYLOAD_BYTES: usize = 258;
  const PUBLIC_PIPE_MARKER: &[u8] = b"AUV_DEVICE_PIPE_PREFLIGHT_V1";
  impl Drop for Worker {
    fn drop(&mut self) {
      if !self.completed {
        // SAFETY: This is the exact process created for the one-shot request.
        // Failing closed prevents a stranded worker reading a later pipe.
        let _ = unsafe { TerminateProcess(self.process.0, 1) };
      }
    }
  }

  fn wide(value: &std::ffi::OsStr) -> Vec<u16> {
    value.encode_wide().chain(Some(0)).collect()
  }

  pub(super) fn checked_target(target: &ConsoleSession) -> Result<(), HostError> {
    let current = observe_console().map_err(|_| HostError::Unverified)?.ok_or(HostError::StaleSession)?;
    if !target.same_login(&current) {
      return Err(HostError::StaleSession);
    }
    if current.lock_state != ConsoleLockState::Locked {
      return Err(HostError::NotLocked);
    }
    Ok(())
  }

  pub(super) fn unlock_with_worker(target: &ConsoleSession, credential: &str) -> Result<ConsoleSession, HostError> {
    checked_target(target)?;
    let worker_executable = std::env::current_exe().map_err(|_| HostError::Unavailable)?.with_file_name("auv-device-unlock-worker.exe");
    if !worker_executable.is_absolute() || credential.is_empty() || credential.encode_utf16().count() > 128 {
      return Err(HostError::Unavailable);
    }
    let mut nonce = [0u8; 16];
    // SAFETY: The system RNG writes only to this initialized, live nonce.
    if unsafe { BCryptGenRandom(None, &mut nonce, BCRYPT_USE_SYSTEM_PREFERRED_RNG) }.is_err() {
      return Err(HostError::Unavailable);
    }
    let pipe_name = format!("\\\\.\\pipe\\auv-device-unlock-{}-{}", std::process::id(), hex(&nonce));
    let pipe = restricted_pipe(&pipe_name)?;
    let mut worker = launch_worker(&worker_executable, WorkerMode::Unlock(&pipe_name), target)?;
    connect_worker(&pipe, &worker)?;
    checked_target(target)?;

    let mut payload = Zeroizing::new(vec![0u8; PIPE_PAYLOAD_BYTES]);
    let units = Zeroizing::new(credential.encode_utf16().collect::<Vec<_>>());
    payload[..2].copy_from_slice(&(units.len() as u16).to_le_bytes());
    for (index, unit) in units.iter().enumerate() {
      payload[2 + index * 2..4 + index * 2].copy_from_slice(&unit.to_le_bytes());
    }
    write_payload(&pipe, &payload)?;
    // The worker performs its own WTS readback. The host independently checks
    // the same login after the worker exits; neither an input count nor an exit
    // code alone establishes an unlock.
    // SAFETY: The process handle is owned and remains live through this wait.
    if unsafe { WaitForSingleObject(worker.process.0, Duration::from_secs(15).as_millis() as u32) } != WAIT_OBJECT_0 {
      return Err(HostError::WorkerUnavailable);
    }
    worker.completed = true;
    let mut exit = 1u32;
    // SAFETY: The exit-code pointer is live and the process handle is valid.
    unsafe { GetExitCodeProcess(worker.process.0, &mut exit) }.map_err(|_| HostError::Unverified)?;
    if exit != 0 {
      return Err(HostError::Unverified);
    }
    let current = observe_console().map_err(|_| HostError::Unverified)?.ok_or(HostError::StaleSession)?;
    if !target.same_login(&current) {
      return Err(HostError::StaleSession);
    }
    if current.lock_state != ConsoleLockState::Usable {
      return Err(HostError::Unverified);
    }
    Ok(current)
  }

  pub(super) fn preflight_with_worker(target: &ConsoleSession) -> Result<PreflightCode, HostError> {
    launch_preflight_worker(target, WorkerMode::Preflight)
  }

  fn preflight_transition_with_worker(target: &ConsoleSession) -> Result<PreflightCode, HostError> {
    launch_preflight_worker(target, WorkerMode::TransitionPreflight)
  }

  fn launch_preflight_worker(target: &ConsoleSession, mode: WorkerMode<'_>) -> Result<PreflightCode, HostError> {
    let exit = launch_preflight_worker_exit(target, mode)?;
    PreflightCode::from_exit_code(exit).ok_or(HostError::Unverified)
  }

  fn launch_preflight_worker_exit(target: &ConsoleSession, mode: WorkerMode<'_>) -> Result<u32, HostError> {
    let worker_executable = std::env::current_exe().map_err(|_| HostError::Unavailable)?.with_file_name("auv-device-unlock-worker.exe");
    if !worker_executable.is_absolute() {
      return Err(HostError::Unavailable);
    }
    let timeout = if matches!(&mode, WorkerMode::TransitionPreflight | WorkerMode::FullTransitionPreflight) {
      Duration::from_secs(8)
    } else {
      Duration::from_secs(5)
    };
    let mut worker = launch_worker(&worker_executable, mode, target)?;
    // No credential is transferred. The worker only checks its session,
    // identity, locked login, and current input desktop on a fresh thread.
    // SAFETY: The process handle is owned and remains live through this wait.
    if unsafe { WaitForSingleObject(worker.process.0, timeout.as_millis() as u32) } != WAIT_OBJECT_0 {
      return Err(HostError::WorkerUnavailable);
    }
    worker.completed = true;
    let mut exit = u32::MAX;
    // SAFETY: The exit-code pointer is live and the process handle is valid.
    unsafe { GetExitCodeProcess(worker.process.0, &mut exit) }.map_err(|_| HostError::Unverified)?;
    Ok(exit)
  }

  fn preflight_pipe_with_worker(target: &ConsoleSession) -> PreflightCode {
    if let Err(error) = checked_target(target) {
      return match error {
        HostError::StaleSession => PreflightCode::StaleSession,
        HostError::NotLocked => PreflightCode::NotLocked,
        _ => PreflightCode::ConsoleUnavailable,
      };
    }
    let Ok(worker_executable) = std::env::current_exe() else {
      return PreflightCode::WorkerUnavailable;
    };
    let worker_executable = worker_executable.with_file_name("auv-device-unlock-worker.exe");
    if !worker_executable.is_absolute() {
      return PreflightCode::WorkerUnavailable;
    }
    let mut nonce = [0u8; 16];
    // SAFETY: The system RNG writes only to this initialized, live nonce.
    if unsafe { BCryptGenRandom(None, &mut nonce, BCRYPT_USE_SYSTEM_PREFERRED_RNG) }.is_err() {
      return PreflightCode::PipeUnavailable;
    }
    let pipe_name = format!("\\\\.\\pipe\\auv-device-preflight-{}-{}", std::process::id(), hex(&nonce));
    let Ok(pipe) = restricted_pipe(&pipe_name) else {
      return PreflightCode::PipeUnavailable;
    };
    let Ok(mut worker) = launch_worker(&worker_executable, WorkerMode::PipePreflight(&pipe_name), target) else {
      return PreflightCode::WorkerUnavailable;
    };
    if connect_worker(&pipe, &worker).is_err() {
      return PreflightCode::PipeConnectFailed;
    }
    if checked_target(target).is_err() {
      return PreflightCode::StaleSession;
    }
    if write_payload(&pipe, &public_payload()).is_err() {
      return PreflightCode::PipeTransferFailed;
    }
    // SAFETY: The exact one-shot worker process remains live through this wait.
    if unsafe { WaitForSingleObject(worker.process.0, Duration::from_secs(5).as_millis() as u32) } != WAIT_OBJECT_0 {
      return PreflightCode::WorkerUnavailable;
    }
    worker.completed = true;
    let mut exit = u32::MAX;
    // SAFETY: The process handle and exit-code pointer are valid.
    if unsafe { GetExitCodeProcess(worker.process.0, &mut exit) }.is_err() {
      return PreflightCode::WorkerUnverified;
    }
    let Some(code) = PreflightCode::from_exit_code(exit) else {
      return PreflightCode::WorkerUnverified;
    };
    if matches!(code, PreflightCode::ReadyDefault | PreflightCode::ReadyWinlogon) && checked_target(target).is_err() {
      return PreflightCode::StaleSession;
    }
    code
  }

  fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    bytes
      .iter()
      .flat_map(|byte| {
        [
          DIGITS[(byte >> 4) as usize] as char,
          DIGITS[(byte & 15) as usize] as char,
        ]
      })
      .collect()
  }

  fn restricted_pipe(name: &str) -> Result<OwnedHandle, HostError> {
    let sddl = wide(std::ffi::OsStr::new("D:P(A;;GA;;;SY)"));
    let mut descriptor = windows::Win32::Security::PSECURITY_DESCRIPTOR::default();
    // SAFETY: The SDDL pointer and out-pointer are live. The resulting LocalAlloc
    // descriptor remains live while CreateNamedPipeW copies its security data.
    unsafe { ConvertStringSecurityDescriptorToSecurityDescriptorW(PCWSTR(sddl.as_ptr()), SDDL_REVISION_1, &mut descriptor, None) }
      .map_err(|_| HostError::Unavailable)?;
    let descriptor = SecurityDescriptor(descriptor);
    let attributes = SECURITY_ATTRIBUTES {
      nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
      lpSecurityDescriptor: descriptor.0.0,
      bInheritHandle: false.into(),
    };
    let name = wide(std::ffi::OsStr::new(name));
    // SAFETY: The UTF-16 name and security descriptor are live for this call.
    // One instance and FILE_FLAG_FIRST_PIPE_INSTANCE reject pre-created pipes.
    let handle = unsafe {
      CreateNamedPipeW(
        PCWSTR(name.as_ptr()),
        PIPE_ACCESS_OUTBOUND | FILE_FLAG_FIRST_PIPE_INSTANCE | FILE_FLAG_OVERLAPPED,
        PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
        1,
        512,
        512,
        0,
        Some(&attributes),
      )
    };
    if handle.is_invalid() {
      Err(HostError::Unavailable)
    } else {
      Ok(OwnedHandle(handle))
    }
  }

  fn launch_worker(exe: &Path, mode: WorkerMode<'_>, target: &ConsoleSession) -> Result<Worker, HostError> {
    // Stable SIDs from Windows contain only this alphabet. This also keeps the
    // non-secret command line unambiguous to the Windows argument parser.
    if !target.account_sid.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-') {
      return Err(HostError::WorkerUnavailable);
    }
    let exe_wide = wide(exe.as_os_str());
    let arguments = match mode {
      WorkerMode::Unlock(pipe_name) => format!("{} {} {} {}", pipe_name, target.session_id, target.logon_time, target.account_sid),
      WorkerMode::Preflight => format!("--preflight {} {} {}", target.session_id, target.logon_time, target.account_sid),
      WorkerMode::PipePreflight(pipe_name) => {
        format!("--preflight-pipe {} {} {} {}", pipe_name, target.session_id, target.logon_time, target.account_sid)
      }
      WorkerMode::TransitionPreflight => {
        format!("--preflight-transition {} {} {}", target.session_id, target.logon_time, target.account_sid)
      }
      WorkerMode::FullTransitionPreflight => {
        format!("--preflight-transition-full {} {} {}", target.session_id, target.logon_time, target.account_sid)
      }
    };
    let command = format!("\"{}\" {arguments}", exe.display());
    let mut command_wide = wide(std::ffi::OsStr::new(&command));
    let mut raw_token = HANDLE::default();
    // SAFETY: This call opens the current process token into one owned handle.
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_DUPLICATE | TOKEN_QUERY, &mut raw_token) }
      .map_err(|_| HostError::WorkerIdentity)?;
    let token = OwnedHandle(raw_token);
    let mut primary = HANDLE::default();
    // SAFETY: The LocalSystem service token is live. The duplicated primary
    // token is owned here and modified only for the selected console session.
    unsafe { DuplicateTokenEx(token.0, TOKEN_ALL_ACCESS, None, SecurityImpersonation, TokenPrimary, &mut primary) }
      .map_err(|_| HostError::WorkerIdentity)?;
    let primary = OwnedHandle(primary);
    // SAFETY: SetTokenInformation reads one live u32 session ID.
    unsafe { SetTokenInformation(primary.0, TokenSessionId, (&target.session_id as *const u32).cast::<c_void>(), size_of::<u32>() as u32) }
      .map_err(|_| HostError::WorkerIdentity)?;
    let mut desktop = wide(std::ffi::OsStr::new("winsta0\\default"));
    let startup = STARTUPINFOW {
      cb: size_of::<STARTUPINFOW>() as u32,
      lpDesktop: PWSTR(desktop.as_mut_ptr()),
      ..Default::default()
    };
    let mut process = PROCESS_INFORMATION::default();
    // SAFETY: All pointers are live. Handles are not inherited, and no secret
    // is in the command line or environment. OS grants this process the
    // selected session through the adjusted LocalSystem primary token.
    unsafe {
      CreateProcessAsUserW(
        primary.0,
        PCWSTR(exe_wide.as_ptr()),
        PWSTR(command_wide.as_mut_ptr()),
        None,
        None,
        false,
        Default::default(),
        None,
        PCWSTR::null(),
        &startup,
        &mut process,
      )
    }
    .map_err(|_| HostError::WorkerUnavailable)?;
    Ok(Worker {
      process: OwnedHandle(process.hProcess),
      _thread: OwnedHandle(process.hThread),
      pid: process.dwProcessId,
      completed: false,
    })
  }

  fn connect_worker(pipe: &OwnedHandle, worker: &Worker) -> Result<(), HostError> {
    // This overlapped connection has a deadline, so a failed worker cannot
    // strand the serving thread before any credential is sent.
    // SAFETY: This creates one owned unnamed event for the overlapped call.
    let event = OwnedHandle(unsafe { CreateEventW(None, true, false, PCWSTR::null()) }.map_err(|_| HostError::Unavailable)?);
    let mut overlapped = OVERLAPPED {
      hEvent: event.0,
      ..Default::default()
    };
    // SAFETY: The pipe and OVERLAPPED remain live through completion/cancel.
    let pending = match unsafe { ConnectNamedPipe(pipe.0, Some(&mut overlapped)) } {
      Ok(()) => false,
      Err(error) if error.code() == ERROR_PIPE_CONNECTED.to_hresult() => false,
      Err(error) if error.code() == ERROR_IO_PENDING.to_hresult() => true,
      Err(_) => return Err(HostError::WorkerUnavailable),
    };
    if pending {
      // SAFETY: The event remains live for the wait.
      if unsafe { WaitForSingleObject(event.0, 10_000) } != WAIT_OBJECT_0 {
        // SAFETY: Cancel the exact pending operation before its stack-owned
        // OVERLAPPED is dropped; GetOverlappedResult waits for final completion.
        let _ = unsafe { CancelIoEx(pipe.0, Some(&overlapped)) };
        let mut ignored = 0;
        let _ = unsafe { GetOverlappedResult(pipe.0, &overlapped, &mut ignored, true) };
        return Err(HostError::WorkerUnavailable);
      }
      let mut ignored = 0;
      // SAFETY: Completed event and live OVERLAPPED identify this connect.
      unsafe { GetOverlappedResult(pipe.0, &overlapped, &mut ignored, false) }.map_err(|_| HostError::WorkerUnavailable)?;
    }
    let mut pid = 0u32;
    // SAFETY: GetNamedPipeClientProcessId writes one u32 for the connected peer.
    unsafe { GetNamedPipeClientProcessId(pipe.0, &mut pid) }.map_err(|_| HostError::WorkerIdentity)?;
    if pid != worker.pid {
      return Err(HostError::WorkerIdentity);
    }
    Ok(())
  }

  fn write_payload(pipe: &OwnedHandle, payload: &[u8]) -> Result<(), HostError> {
    // One fixed-size write hides credential length for the unlock mode.
    // SAFETY: The payload and OVERLAPPED remain live until completion/cancel.
    let event = OwnedHandle(unsafe { CreateEventW(None, true, false, PCWSTR::null()) }.map_err(|_| HostError::TransferFailed)?);
    let mut overlapped = OVERLAPPED {
      hEvent: event.0,
      ..Default::default()
    };
    let mut written = 0u32;
    let pending = match unsafe { WriteFile(pipe.0, Some(payload), Some(&mut written), Some(&mut overlapped)) } {
      Ok(()) => false,
      Err(error) if error.code() == ERROR_IO_PENDING.to_hresult() => true,
      Err(_) => return Err(HostError::TransferFailed),
    };
    if pending {
      // SAFETY: The event remains live until the operation has completed.
      if unsafe { WaitForSingleObject(event.0, 5_000) } != WAIT_OBJECT_0 {
        let _ = unsafe { CancelIoEx(pipe.0, Some(&overlapped)) };
        let _ = unsafe { GetOverlappedResult(pipe.0, &overlapped, &mut written, true) };
        return Err(HostError::TransferFailed);
      }
      unsafe { GetOverlappedResult(pipe.0, &overlapped, &mut written, false) }.map_err(|_| HostError::TransferFailed)?;
    }
    if written as usize != payload.len() {
      return Err(HostError::TransferFailed);
    }
    Ok(())
  }

  fn read_payload(pipe_name: &str) -> Result<(OwnedHandle, Zeroizing<Vec<u8>>), HostError> {
    let pipe_wide = wide(std::ffi::OsStr::new(pipe_name));
    // SAFETY: The validated path is a local pipe. The server compares this
    // client's process ID to the exact one-shot worker before writing.
    let pipe = OwnedHandle(
      unsafe {
        CreateFileW(
          PCWSTR(pipe_wide.as_ptr()),
          GENERIC_READ.0,
          FILE_SHARE_MODE(0),
          None,
          OPEN_EXISTING,
          Default::default(),
          HANDLE::default(),
        )
      }
      .map_err(|_| HostError::WorkerUnavailable)?,
    );
    let mut payload = Zeroizing::new(vec![0u8; PIPE_PAYLOAD_BYTES]);
    let mut offset = 0usize;
    while offset < payload.len() {
      let mut read = 0u32;
      // SAFETY: ReadFile writes only to the remaining initialized slice.
      unsafe { ReadFile(pipe.0, Some(&mut payload[offset..]), Some(&mut read), None) }.map_err(|_| HostError::TransferFailed)?;
      if read == 0 {
        return Err(HostError::TransferFailed);
      }
      offset += read as usize;
    }
    Ok((pipe, payload))
  }

  fn public_payload() -> Vec<u8> {
    let mut payload = vec![0u8; PIPE_PAYLOAD_BYTES];
    payload[..PUBLIC_PIPE_MARKER.len()].copy_from_slice(PUBLIC_PIPE_MARKER);
    payload
  }

  pub(super) fn run_worker(pipe_name: &str, session_id: u32, logon_time: i64, account_sid: &str) -> Result<(), HostError> {
    if !pipe_name.starts_with("\\\\.\\pipe\\auv-device-unlock-") || pipe_name.len() > 128 {
      return Err(HostError::WorkerUnavailable);
    }
    let target = ConsoleSession {
      session_id,
      logon_time,
      account_sid: account_sid.to_owned(),
      domain: String::new(),
      user: String::new(),
      lock_state: ConsoleLockState::Locked,
    };
    checked_target(&target)?;
    let (_pipe, payload) = read_payload(pipe_name)?;
    let length = u16::from_le_bytes([payload[0], payload[1]]) as usize;
    if length == 0 || length > 128 {
      return Err(HostError::TransferFailed);
    }
    let mut units = Zeroizing::new(Vec::with_capacity(length));
    for index in 0..length {
      units.push(u16::from_le_bytes([payload[2 + index * 2], payload[3 + index * 2]]));
    }
    let credential = Zeroizing::new(String::from_utf16(&units).map_err(|_| HostError::TransferFailed)?);
    checked_target(&target)?;
    unlock_existing_session(&target, &credential).map_err(|_| HostError::Unverified)?;
    Ok(())
  }

  pub(super) fn run_preflight_worker(session_id: u32, logon_time: i64, account_sid: &str) -> PreflightCode {
    let target = ConsoleSession {
      session_id,
      logon_time,
      account_sid: account_sid.to_owned(),
      domain: String::new(),
      user: String::new(),
      lock_state: ConsoleLockState::Locked,
    };
    match checked_target(&target) {
      Ok(()) => {}
      Err(HostError::StaleSession) => return PreflightCode::StaleSession,
      Err(HostError::NotLocked) => return PreflightCode::NotLocked,
      Err(_) => return PreflightCode::UnsupportedOsState,
    }
    preflight_target(&target)
  }

  fn preflight_target(target: &ConsoleSession) -> PreflightCode {
    match preflight_locked_worker(target) {
      Ok(InputDesktop::Default) => PreflightCode::ReadyDefault,
      Ok(InputDesktop::Winlogon) => PreflightCode::ReadyWinlogon,
      Err(ConsoleUnlockError::StaleSession) => PreflightCode::StaleSession,
      Err(ConsoleUnlockError::NotLocked) => PreflightCode::NotLocked,
      Err(ConsoleUnlockError::WorkerIdentity) => PreflightCode::WorkerIdentity,
      Err(ConsoleUnlockError::DesktopUnavailable) => PreflightCode::DesktopUnavailable,
      Err(_) => PreflightCode::UnsupportedOsState,
    }
  }

  fn locked_host_target() -> Result<ConsoleSession, PreflightCode> {
    if verify_local_system_process_in_session(0).is_err() {
      return Err(PreflightCode::HostIdentity);
    }
    let target = match observe_console() {
      Ok(Some(session)) => session,
      Ok(None) => return Err(PreflightCode::StaleSession),
      Err(_) => return Err(PreflightCode::ConsoleUnavailable),
    };
    if target.lock_state != ConsoleLockState::Locked {
      return Err(PreflightCode::NotLocked);
    }
    Ok(target)
  }

  pub(super) fn run_preflight_host() -> PreflightCode {
    let target = match locked_host_target() {
      Ok(target) => target,
      Err(code) => return code,
    };
    host_preflight_code(super::preflight_locked_with_worker(&target))
  }

  fn host_preflight_code(result: Result<PreflightCode, HostError>) -> PreflightCode {
    match result {
      Ok(code) => code,
      Err(HostError::StaleSession) => PreflightCode::StaleSession,
      Err(HostError::NotLocked) => PreflightCode::NotLocked,
      Err(HostError::WorkerIdentity) => PreflightCode::WorkerIdentity,
      Err(HostError::WorkerUnavailable) => PreflightCode::WorkerUnavailable,
      Err(HostError::Unverified) => PreflightCode::WorkerUnverified,
      Err(_) => PreflightCode::UnsupportedOsState,
    }
  }

  pub(super) fn run_transition_preflight_host() -> PreflightCode {
    let target = match locked_host_target() {
      Ok(target) => target,
      Err(code) => return code,
    };
    if let Err(error) = checked_target(&target) {
      return host_preflight_code(Err(error));
    }
    let code = host_preflight_code(preflight_transition_with_worker(&target));
    if code == PreflightCode::TransitionReady {
      // A worker exit alone is not evidence that this login stayed locked.
      if let Err(error) = checked_target(&target) {
        return host_preflight_code(Err(error));
      }
    }
    code
  }

  pub(super) fn run_transition_preflight_worker(session_id: u32, logon_time: i64, account_sid: &str) -> PreflightCode {
    match transition_preflight_result(session_id, logon_time, account_sid, TransitionDesktopAccess::Limited) {
      Ok(result) => transition_code(result),
      Err(code) => code,
    }
  }

  fn transition_preflight_result(
    session_id: u32,
    logon_time: i64,
    account_sid: &str,
    access: TransitionDesktopAccess,
  ) -> Result<TransitionPreflightResult, PreflightCode> {
    let target = ConsoleSession {
      session_id,
      logon_time,
      account_sid: account_sid.to_owned(),
      domain: String::new(),
      user: String::new(),
      lock_state: ConsoleLockState::Locked,
    };
    match checked_target(&target) {
      Ok(()) => {}
      Err(HostError::StaleSession) => return Err(PreflightCode::StaleSession),
      Err(HostError::NotLocked) => return Err(PreflightCode::NotLocked),
      Err(_) => return Err(PreflightCode::ConsoleUnavailable),
    }
    Ok(preflight_default_to_winlogon(&target, access))
  }

  fn transition_code(result: TransitionPreflightResult) -> PreflightCode {
    match result {
      TransitionPreflightResult::ReadyWinlogon => PreflightCode::TransitionReady,
      TransitionPreflightResult::WorkerIdentity => PreflightCode::WorkerIdentity,
      TransitionPreflightResult::TargetChanged => PreflightCode::StaleSession,
      TransitionPreflightResult::NotLocked => PreflightCode::NotLocked,
      TransitionPreflightResult::InitialDesktopUnavailable => PreflightCode::TransitionInitialDesktop,
      TransitionPreflightResult::ReturnRejected { .. } => PreflightCode::TransitionReturnRejected,
      TransitionPreflightResult::WinlogonUnavailable => PreflightCode::TransitionWinlogonUnavailable,
      TransitionPreflightResult::WinlogonBindUnavailable => PreflightCode::TransitionWinlogonBindUnavailable,
      TransitionPreflightResult::UnsupportedOsState => PreflightCode::UnsupportedOsState,
    }
  }

  pub(super) fn run_full_transition_preflight_worker(session_id: u32, logon_time: i64, account_sid: &str) -> u32 {
    match transition_preflight_result(session_id, logon_time, account_sid, TransitionDesktopAccess::PrototypeFull) {
      Ok(TransitionPreflightResult::ReturnRejected {
        inserted,
        win32_error,
      }) => encode_return_rejection(inserted, win32_error),
      Ok(result) => transition_code(result).exit_code(),
      Err(code) => code.exit_code(),
    }
  }

  pub(super) fn run_full_transition_preflight_host() -> TransitionDiagnostic {
    let target = match locked_host_target() {
      Ok(target) => target,
      Err(code) => return TransitionDiagnostic::code(code),
    };
    if let Err(error) = checked_target(&target) {
      return TransitionDiagnostic::code(host_preflight_code(Err(error)));
    }
    let exit = match launch_preflight_worker_exit(&target, WorkerMode::FullTransitionPreflight) {
      Ok(exit) => exit,
      Err(error) => return TransitionDiagnostic::code(host_preflight_code(Err(error))),
    };
    let report = full_transition_report(exit);
    if report.code == PreflightCode::TransitionReady {
      // The desktop transition is useful only for the same still-locked login.
      if let Err(error) = checked_target(&target) {
        return TransitionDiagnostic::code(host_preflight_code(Err(error)));
      }
    }
    report
  }

  pub(super) fn run_pipe_preflight_host() -> PreflightCode {
    let target = match locked_host_target() {
      Ok(target) => target,
      Err(code) => return code,
    };
    preflight_pipe_with_worker(&target)
  }

  pub(super) fn run_pipe_preflight_worker(pipe_name: &str, session_id: u32, logon_time: i64, account_sid: &str) -> PreflightCode {
    if !pipe_name.starts_with("\\\\.\\pipe\\auv-device-preflight-") || pipe_name.len() > 128 {
      return PreflightCode::PipeUnavailable;
    }
    let target = ConsoleSession {
      session_id,
      logon_time,
      account_sid: account_sid.to_owned(),
      domain: String::new(),
      user: String::new(),
      lock_state: ConsoleLockState::Locked,
    };
    match checked_target(&target) {
      Ok(()) => {}
      Err(HostError::StaleSession) => return PreflightCode::StaleSession,
      Err(HostError::NotLocked) => return PreflightCode::NotLocked,
      Err(_) => return PreflightCode::ConsoleUnavailable,
    }
    let (_pipe, payload) = match read_payload(pipe_name) {
      Ok(payload) => payload,
      Err(HostError::WorkerUnavailable) => return PreflightCode::PipeConnectFailed,
      Err(_) => return PreflightCode::PipeTransferFailed,
    };
    if payload.as_slice() != public_payload().as_slice() {
      return PreflightCode::PipePayloadMismatch;
    }
    match checked_target(&target) {
      Ok(()) => preflight_target(&target),
      Err(HostError::StaleSession) => PreflightCode::StaleSession,
      Err(HostError::NotLocked) => PreflightCode::NotLocked,
      Err(_) => PreflightCode::ConsoleUnavailable,
    }
  }
}
