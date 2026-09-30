//! Platform observations used by existing-session Device lock and unlock.
//!
//! Supported host candidates route DeviceService through the same target-local
//! switch, enrollment, account locks, and audit as DeviceLocalService.

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "macos")]
mod enrollment_macos;

#[cfg(target_os = "macos")]
mod host_macos;

#[cfg(target_os = "windows")]
mod enrollment_windows;
#[cfg(target_os = "windows")]
mod host_windows;

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "linux")]
mod host_linux;

#[cfg(target_os = "linux")]
mod vault_linux;

#[cfg(target_os = "linux")]
mod enrollment_linux;

#[cfg(target_os = "linux")]
mod pam_native;

mod audit;
mod metadata;
mod policy;
#[cfg(target_os = "windows")]
pub(crate) mod storage_windows;

#[cfg(target_os = "windows")]
pub(crate) fn windows_store_root() -> Result<std::path::PathBuf, String> {
  storage_windows::root_path().map_err(|error| format!("failed to locate Windows Device entry storage: {error}"))
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
mod local;
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub(crate) use local::LocalState;
