//! LocalSystem-owned Windows credential enrollment store.
//!
//! Machine-scope DPAPI is encryption at rest, not an account boundary. The
//! protected directory and every blob therefore require a SYSTEM-only DACL.
//! An enrollment is only PENDING after writing; readiness requires a separate
//! locked-session retrieval under the installed LocalSystem identity.

use crate::device_session::ConsoleSession;

#[derive(Debug, thiserror::Error)]
pub enum VaultError {
  #[error("the Windows enrollment vault is unavailable")]
  Unavailable,
  #[error("the Windows enrollment vault permissions are invalid")]
  Permissions,
  #[error("the account SID is invalid")]
  InvalidAccount,
  #[error("this account is not enrolled")]
  NotEnrolled,
  #[error("the protected credential cannot be retrieved")]
  RetrievalFailed,
  #[error("the selected account does not have a locked console session")]
  NotLocked,
}

/// Persist an OS credential under the stable account SID. The local management
/// service must authorize its real peer SID before calling this function. A
/// successful write is PENDING until `verify_while_locked` succeeds.
// The DeviceLocalService enrollment backend holds the per-SID account lock
// across metadata invalidation, this write, and publication of PENDING.
pub fn enroll(account_sid: &str, credential: &str) -> Result<(), VaultError> {
  native::enroll(account_sid, credential)
}

/// Delete one account's protected credential after local peer authorization.
pub fn remove(account_sid: &str) -> Result<(), VaultError> {
  native::remove(account_sid)
}

/// Prove a LocalSystem process can decrypt this account's item while its
/// current physical-console session remains locked. No credential is returned.
pub fn verify_while_locked(target: &ConsoleSession) -> Result<(), VaultError> {
  native::verify_while_locked(target)
}

/// Host-internal retrieval. Never expose this through DeviceService, CLI,
/// tracing, audit, or a remote request/response.
pub(crate) fn retrieve(account_sid: &str) -> Result<zeroize::Zeroizing<String>, VaultError> {
  native::retrieve(account_sid)
}

#[cfg(not(target_os = "windows"))]
mod native {
  use super::VaultError;
  use crate::device_session::ConsoleSession;
  use zeroize::Zeroizing;

  pub(super) fn enroll(_: &str, _: &str) -> Result<(), VaultError> {
    Err(VaultError::Unavailable)
  }
  pub(super) fn remove(_: &str) -> Result<(), VaultError> {
    Err(VaultError::Unavailable)
  }
  pub(super) fn verify_while_locked(_: &ConsoleSession) -> Result<(), VaultError> {
    Err(VaultError::Unavailable)
  }
  pub(super) fn retrieve(_: &str) -> Result<Zeroizing<String>, VaultError> {
    Err(VaultError::Unavailable)
  }
}

#[cfg(target_os = "windows")]
mod native {
  use std::ffi::{OsStr, c_void};
  use std::fs::{self, File};
  use std::io::Write;
  use std::mem::{align_of, size_of};
  use std::os::windows::ffi::OsStrExt;
  use std::os::windows::io::FromRawHandle;
  use std::path::{Path, PathBuf};

  use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GENERIC_WRITE, HANDLE, HLOCAL, LocalFree};
  use windows::Win32::Security::Authorization::{
    ConvertSecurityDescriptorToStringSecurityDescriptorW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
  };
  use windows::Win32::Security::Cryptography::{
    BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom, CRYPT_INTEGER_BLOB, CRYPTPROTECT_LOCAL_MACHINE, CRYPTPROTECT_UI_FORBIDDEN,
    CryptProtectData, CryptUnprotectData,
  };
  use windows::Win32::Security::{
    DACL_SECURITY_INFORMATION, GetFileSecurityW, GetTokenInformation, IsWellKnownSid, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR,
    SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER, TokenUser, WinLocalSystemSid,
  };
  use windows::Win32::Storage::FileSystem::{
    CREATE_NEW, CreateDirectoryW, CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_REPARSE_POINT, FILE_SHARE_MODE, GetFileAttributesW,
    MOVEFILE_REPLACE_EXISTING, MoveFileExW,
  };
  use windows::Win32::System::Com::CoTaskMemFree;
  use windows::Win32::System::Memory::LocalSize;
  use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
  use windows::Win32::System::Threading::{GetCurrentProcess, GetCurrentProcessId, OpenProcessToken};
  use windows::Win32::UI::Shell::{FOLDERID_ProgramData, KNOWN_FOLDER_FLAG, SHGetKnownFolderPath};
  use windows::core::{PCWSTR, PWSTR};
  use zeroize::{Zeroize, Zeroizing};

  use super::VaultError;
  use crate::device_session::{ConsoleLockState, ConsoleSession, observe_console};

  const SYSTEM_OWNER_AND_DACL: &str = "O:SYD:P(A;;GA;;;SY)";
  // NOTICE(device-unlock-windows-sddl): Windows maps GA to file-all access on
  // a created filesystem object. This read requests only OWNER and DACL, so
  // the returned form omits G and must still contain one protected SYSTEM ACE;
  // see `device_entry/storage_windows.rs`.
  const SYSTEM_OBJECT_SDDL: &str = "O:SYD:P(A;;FA;;;SY)";
  const MAX_BLOB: u64 = 4096;

  struct Token(HANDLE);
  impl Drop for Token {
    fn drop(&mut self) {
      // SAFETY: OpenProcessToken returned this owned kernel handle once.
      let _ = unsafe { CloseHandle(self.0) };
    }
  }

  fn require_system_host() -> Result<(), VaultError> {
    let mut session = u32::MAX;
    // SAFETY: ProcessIdToSessionId writes one live u32. The service-side vault
    // is accessible only from the LocalSystem service in Session 0.
    unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut session) }.map_err(|_| VaultError::Permissions)?;
    if session != 0 {
      return Err(VaultError::Permissions);
    }
    let mut raw = HANDLE::default();
    // SAFETY: OpenProcessToken gives one owned token handle on success.
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw) }.map_err(|_| VaultError::Permissions)?;
    let token = Token(raw);
    let mut bytes = 0u32;
    // SAFETY: A null buffer queries the required TokenUser size.
    let _ = unsafe { GetTokenInformation(token.0, TokenUser, None, 0, &mut bytes) };
    if bytes < size_of::<TOKEN_USER>() as u32 {
      return Err(VaultError::Permissions);
    }
    if align_of::<TOKEN_USER>() > align_of::<usize>() {
      return Err(VaultError::Permissions);
    }
    let mut data = vec![0usize; (bytes as usize).div_ceil(size_of::<usize>())];
    // SAFETY: A usize Vec is aligned for TOKEN_USER on Windows and owns at
    // least the queried byte capacity. The embedded SID lives in this buffer.
    unsafe { GetTokenInformation(token.0, TokenUser, Some(data.as_mut_ptr().cast()), bytes, &mut bytes) }
      .map_err(|_| VaultError::Permissions)?;
    if (bytes as usize) < size_of::<TOKEN_USER>() {
      return Err(VaultError::Permissions);
    }
    // SAFETY: GetTokenInformation wrote a complete aligned TOKEN_USER header.
    let user = unsafe { data.as_ptr().cast::<TOKEN_USER>().read() };
    if user.User.Sid.0.is_null() {
      return Err(VaultError::Permissions);
    }
    // SAFETY: The SID points into the live TokenUser allocation.
    if !unsafe { IsWellKnownSid(user.User.Sid, WinLocalSystemSid) }.as_bool() {
      return Err(VaultError::Permissions);
    }
    Ok(())
  }

  fn wide(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(Some(0)).collect()
  }

  fn sid_name(sid: &str) -> Result<String, VaultError> {
    if !sid.starts_with("S-1-") || sid.len() > 128 || !sid.bytes().all(|byte| byte.is_ascii_digit() || byte == b'S' || byte == b'-') {
      return Err(VaultError::InvalidAccount);
    }
    Ok(format!("{sid}.dpapi"))
  }

  struct Descriptor(PSECURITY_DESCRIPTOR);
  impl Descriptor {
    fn system_only() -> Result<Self, VaultError> {
      // NOTICE(device-unlock-vault-owner): A SYSTEM service may otherwise
      // create this object with Administrators as its default owner. The
      // verifier requires SYSTEM ownership before any PIN is stored.
      let sddl = wide(OsStr::new(SYSTEM_OWNER_AND_DACL));
      let mut descriptor = PSECURITY_DESCRIPTOR::default();
      // SAFETY: The UTF-16 SDDL and output pointer are live for this call.
      unsafe { ConvertStringSecurityDescriptorToSecurityDescriptorW(PCWSTR(sddl.as_ptr()), SDDL_REVISION_1, &mut descriptor, None) }
        .map_err(|_| VaultError::Unavailable)?;
      Ok(Self(descriptor))
    }
    fn attributes(&self) -> SECURITY_ATTRIBUTES {
      SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: self.0.0,
        bInheritHandle: false.into(),
      }
    }
  }
  impl Drop for Descriptor {
    fn drop(&mut self) {
      // SAFETY: The SDDL conversion allocated this descriptor with LocalAlloc.
      unsafe { LocalFree(HLOCAL(self.0.0)) };
    }
  }

  fn vault_dir() -> Result<PathBuf, VaultError> {
    require_system_host()?;
    // SAFETY: SHGetKnownFolderPath allocates a UTF-16 string with CoTaskMemAlloc.
    let path = unsafe { SHGetKnownFolderPath(&FOLDERID_ProgramData, KNOWN_FOLDER_FLAG(0), HANDLE::default()) }
      .map_err(|_| VaultError::Unavailable)?;
    // SAFETY: The returned pointer is a terminated Windows path.
    let path_text = unsafe { path.to_string() }.map_err(|_| VaultError::Unavailable);
    // SAFETY: CoTaskMemFree releases the folder path once after decoding.
    unsafe { CoTaskMemFree(Some(path.0.cast::<c_void>())) };
    let root = PathBuf::from(path_text?).join("AUVDeviceEnrollments");
    let descriptor = Descriptor::system_only()?;
    let root_wide = wide(root.as_os_str());
    // SAFETY: This only creates the leaf below the OS ProgramData directory,
    // with a protected DACL. An existing leaf is checked below before use.
    match unsafe { CreateDirectoryW(PCWSTR(root_wide.as_ptr()), Some(&descriptor.attributes())) } {
      Ok(()) => {}
      Err(error) if error.code() == ERROR_ALREADY_EXISTS.to_hresult() => {}
      Err(_) => return Err(VaultError::Unavailable),
    }
    verify_acl(&root)?;
    Ok(root)
  }

  fn verify_acl(path: &Path) -> Result<(), VaultError> {
    let path_wide = wide(path.as_os_str());
    // SAFETY: GetFileAttributesW reads a valid terminated path.
    let attributes = unsafe { GetFileAttributesW(PCWSTR(path_wide.as_ptr())) };
    if attributes == u32::MAX || attributes & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 {
      return Err(VaultError::Permissions);
    }
    let mut bytes = 0u32;
    let security_info = OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION;
    // SAFETY: A zero-capacity call queries the required self-relative security
    // descriptor size. The failure return is expected here.
    let _ = unsafe { GetFileSecurityW(PCWSTR(path_wide.as_ptr()), security_info.0, PSECURITY_DESCRIPTOR::default(), 0, &mut bytes) };
    if bytes == 0 || bytes > 64 * 1024 {
      return Err(VaultError::Permissions);
    }
    let mut data = vec![0usize; (bytes as usize).div_ceil(size_of::<usize>())];
    // SAFETY: The pointer-sized backing allocation is aligned for Windows
    // security descriptors, has at least the queried byte capacity, and stays
    // live through SDDL conversion.
    unsafe {
      GetFileSecurityW(PCWSTR(path_wide.as_ptr()), security_info.0, PSECURITY_DESCRIPTOR(data.as_mut_ptr().cast()), bytes, &mut bytes)
    }
    .ok()
    .map_err(|_| VaultError::Permissions)?;
    let mut output = PWSTR::null();
    // SAFETY: The descriptor is live; Windows allocates the output with LocalAlloc.
    unsafe {
      ConvertSecurityDescriptorToStringSecurityDescriptorW(
        PSECURITY_DESCRIPTOR(data.as_mut_ptr().cast()),
        SDDL_REVISION_1,
        security_info,
        &mut output,
        None,
      )
    }
    .map_err(|_| VaultError::Permissions)?;
    // SAFETY: The returned output is a terminated SDDL string.
    let text = unsafe { output.to_string() }.map_err(|_| VaultError::Permissions);
    // SAFETY: Free the LocalAlloc output exactly once.
    unsafe { LocalFree(HLOCAL(output.0.cast())) };
    if text? != SYSTEM_OBJECT_SDDL {
      return Err(VaultError::Permissions);
    }
    Ok(())
  }

  fn item_path(sid: &str) -> Result<PathBuf, VaultError> {
    Ok(vault_dir()?.join(sid_name(sid)?))
  }

  fn protect(sid: &str, credential: &str) -> Result<Zeroizing<Vec<u8>>, VaultError> {
    let mut plain = Zeroizing::new(credential.as_bytes().to_vec());
    let input = CRYPT_INTEGER_BLOB {
      cbData: plain.len() as u32,
      pbData: plain.as_mut_ptr(),
    };
    let mut entropy_bytes = sid.as_bytes().to_vec();
    let entropy = CRYPT_INTEGER_BLOB {
      cbData: entropy_bytes.len() as u32,
      pbData: entropy_bytes.as_mut_ptr(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    // SAFETY: Both input buffers stay live; DPAPI allocates output with LocalAlloc.
    let protected = unsafe {
      CryptProtectData(
        &input,
        PCWSTR::null(),
        Some(&entropy),
        None,
        None,
        CRYPTPROTECT_LOCAL_MACHINE | CRYPTPROTECT_UI_FORBIDDEN,
        &mut output,
      )
    };
    if protected.is_err() {
      // SAFETY: DPAPI may have set an output allocation before returning an
      // error; release it if present. This output is encrypted, not plaintext.
      unsafe { LocalFree(HLOCAL(output.pbData.cast())) };
      return Err(VaultError::Unavailable);
    }
    if output.pbData.is_null() || output.cbData == 0 || output.cbData as u64 > MAX_BLOB {
      unsafe { LocalFree(HLOCAL(output.pbData.cast())) };
      return Err(VaultError::Unavailable);
    }
    // SAFETY: DPAPI initialized output.cbData bytes at output.pbData.
    let encrypted = Zeroizing::new(unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec());
    // SAFETY: Release exactly the allocation returned by DPAPI.
    unsafe { LocalFree(HLOCAL(output.pbData.cast())) };
    entropy_bytes.zeroize();
    Ok(encrypted)
  }

  pub(super) fn enroll(sid: &str, credential: &str) -> Result<(), VaultError> {
    if credential.is_empty() || credential.chars().any(char::is_control) || credential.encode_utf16().count() > 128 {
      return Err(VaultError::RetrievalFailed);
    }
    let destination = item_path(sid)?;
    let encrypted = protect(sid, credential)?;
    let mut nonce = [0u8; 16];
    // SAFETY: The system RNG writes only to the live nonce.
    if unsafe { BCryptGenRandom(None, &mut nonce, BCRYPT_USE_SYSTEM_PREFERRED_RNG) }.is_err() {
      return Err(VaultError::Unavailable);
    }
    let mut name = String::new();
    for byte in nonce {
      use std::fmt::Write as _;
      write!(&mut name, "{byte:02x}").map_err(|_| VaultError::Unavailable)?;
    }
    let temporary = destination.with_extension(format!("{name}.tmp"));
    let descriptor = Descriptor::system_only()?;
    let temporary_wide = wide(temporary.as_os_str());
    // SAFETY: Create a new item with its own SYSTEM-only DACL. CREATE_NEW
    // prevents overwriting any preexisting path or following a link.
    let raw = unsafe {
      CreateFileW(
        PCWSTR(temporary_wide.as_ptr()),
        GENERIC_WRITE.0,
        FILE_SHARE_MODE(0),
        Some(&descriptor.attributes()),
        CREATE_NEW,
        FILE_ATTRIBUTE_NORMAL,
        HANDLE::default(),
      )
    }
    .map_err(|_| VaultError::Unavailable)?;
    // SAFETY: File now exclusively owns the successful CreateFileW handle.
    let mut file = unsafe { File::from_raw_handle(raw.0) };
    let written = file.write_all(&encrypted).and_then(|_| file.sync_all());
    drop(file);
    if written.is_err() {
      let _ = fs::remove_file(&temporary);
      return Err(VaultError::Unavailable);
    }
    let destination_wide = wide(destination.as_os_str());
    // SAFETY: Both paths are fixed children of the ACL-verified vault root.
    // Replace is atomic on this local filesystem; the new item carries the
    // SYSTEM-only DACL from its own CreateFileW call.
    let moved = unsafe { MoveFileExW(PCWSTR(temporary_wide.as_ptr()), PCWSTR(destination_wide.as_ptr()), MOVEFILE_REPLACE_EXISTING) };
    if moved.is_err() {
      let _ = fs::remove_file(&temporary);
      return Err(VaultError::Unavailable);
    }
    verify_acl(&destination)?;
    // Write-time readback proves the service identity can decrypt now. Policy
    // must still wait for verify_while_locked before calling enrollment READY.
    retrieve(sid).map(|_| ())
  }

  pub(super) fn remove(sid: &str) -> Result<(), VaultError> {
    let path = item_path(sid)?;
    if !path.exists() {
      return Err(VaultError::NotEnrolled);
    }
    verify_acl(&path)?;
    fs::remove_file(path).map_err(|_| VaultError::Unavailable)
  }

  pub(super) fn retrieve(sid: &str) -> Result<Zeroizing<String>, VaultError> {
    let path = item_path(sid)?;
    if !path.exists() {
      return Err(VaultError::NotEnrolled);
    }
    verify_acl(&path)?;
    let metadata = fs::metadata(&path).map_err(|_| VaultError::RetrievalFailed)?;
    if metadata.len() == 0 || metadata.len() > MAX_BLOB {
      return Err(VaultError::RetrievalFailed);
    }
    let mut encrypted = Zeroizing::new(fs::read(path).map_err(|_| VaultError::RetrievalFailed)?);
    if encrypted.len() as u64 != metadata.len() {
      return Err(VaultError::RetrievalFailed);
    }
    let input = CRYPT_INTEGER_BLOB {
      cbData: encrypted.len() as u32,
      pbData: encrypted.as_mut_ptr(),
    };
    let mut entropy_bytes = sid.as_bytes().to_vec();
    let entropy = CRYPT_INTEGER_BLOB {
      cbData: entropy_bytes.len() as u32,
      pbData: entropy_bytes.as_mut_ptr(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    // SAFETY: Input and entropy remain live; DPAPI allocates the output.
    let unprotected = unsafe { CryptUnprotectData(&input, None, Some(&entropy), None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut output) };
    if unprotected.is_err() {
      wipe_dpapi_plain(&mut output);
      return Err(VaultError::RetrievalFailed);
    }
    // SAFETY: LocalSize inspects the successful DPAPI LocalAlloc allocation.
    let allocated = if output.pbData.is_null() {
      0
    } else {
      unsafe { LocalSize(HLOCAL(output.pbData.cast())) }
    };
    if output.pbData.is_null() || output.cbData == 0 || output.cbData > 512 || output.cbData as usize > allocated {
      wipe_dpapi_plain(&mut output);
      return Err(VaultError::RetrievalFailed);
    }
    // SAFETY: DPAPI initialized the returned allocation for output.cbData.
    let plain = Zeroizing::new(unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec());
    wipe_dpapi_plain(&mut output);
    entropy_bytes.zeroize();
    let value = std::str::from_utf8(&plain).map_err(|_| VaultError::RetrievalFailed)?.to_owned();
    Ok(Zeroizing::new(value))
  }

  fn wipe_dpapi_plain(output: &mut CRYPT_INTEGER_BLOB) {
    if output.pbData.is_null() {
      return;
    }
    // SAFETY: DPAPI returns a LocalAlloc allocation. On an error, cbData might
    // not be consistent with the allocation; LocalSize bounds all writes.
    unsafe {
      let allocated = LocalSize(HLOCAL(output.pbData.cast()));
      for index in 0..(output.cbData as usize).min(allocated) {
        output.pbData.add(index).write_volatile(0);
      }
      LocalFree(HLOCAL(output.pbData.cast()));
    }
    output.pbData = std::ptr::null_mut();
    output.cbData = 0;
  }

  pub(super) fn verify_while_locked(target: &ConsoleSession) -> Result<(), VaultError> {
    let current = observe_console().map_err(|_| VaultError::NotLocked)?.ok_or(VaultError::NotLocked)?;
    if !target.same_login(&current) || current.lock_state != ConsoleLockState::Locked {
      return Err(VaultError::NotLocked);
    }
    retrieve(&current.account_sid)?;
    let after = observe_console().map_err(|_| VaultError::NotLocked)?.ok_or(VaultError::NotLocked)?;
    if !target.same_login(&after) || after.lock_state != ConsoleLockState::Locked {
      return Err(VaultError::NotLocked);
    }
    Ok(())
  }

  #[cfg(test)]
  mod tests {
    use super::*;

    #[test]
    fn vault_descriptor_sets_system_owner_explicitly() {
      // ROOT CAUSE:
      //
      // A LocalSystem process created the vault directory with Administrators
      // as its default owner when the descriptor specified only a DACL.
      // Before the fix, the first enrollment left an empty, unusable vault.
      // The creation descriptor now names SYSTEM as owner before any PIN write.
      let descriptor = Descriptor::system_only().expect("vault security descriptor");
      let mut owner = windows::Win32::Security::PSID::default();
      let mut defaulted = windows::Win32::Foundation::BOOL::default();
      // SAFETY: The descriptor stays live while Windows returns a borrowed
      // owner SID pointer into it and a Boolean default-owner flag.
      unsafe { windows::Win32::Security::GetSecurityDescriptorOwner(descriptor.0, &mut owner, &mut defaulted) }.expect("descriptor owner");
      assert!(!defaulted.as_bool());
      assert!(unsafe { IsWellKnownSid(owner, WinLocalSystemSid) }.as_bool());
    }
  }
}
