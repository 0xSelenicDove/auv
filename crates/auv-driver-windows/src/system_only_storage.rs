//! Shared LocalSystem-only object checks for Windows Device persistence.
//!
//! Verify the opened handle so a path substitution cannot change the object
//! between an ACL check and a read.

use std::ffi::OsStr;
use std::fs::File;
use std::io;
use std::mem::{align_of, size_of};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle};

use windows::Win32::Foundation::{HANDLE, HLOCAL, LocalFree};
use windows::Win32::Security::Authorization::{
  ConvertSecurityDescriptorToStringSecurityDescriptorW, ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo,
  SDDL_REVISION_1, SE_FILE_OBJECT,
};
use windows::Win32::Security::{
  DACL_SECURITY_INFORMATION, GetTokenInformation, IsWellKnownSid, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES,
  TOKEN_QUERY, TOKEN_USER, TokenUser, WinLocalSystemSid,
};
use windows::Win32::Storage::FileSystem::{
  BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT, GetFileInformationByHandle,
};
use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows::Win32::System::Threading::{GetCurrentProcess, GetCurrentProcessId, OpenProcessToken};
use windows::core::{PCWSTR, PWSTR};

const SYSTEM_OWNER_AND_DACL: &str = "O:SYD:P(A;;GA;;;SY)";
// NOTICE(device-entry-windows-sddl): Windows stores GA as file-all access.
// OWNER and DACL reads omit G, so this exact SYSTEM-only form is expected.
const SYSTEM_OBJECT_SDDL: &str = "O:SYD:P(A;;FA;;;SY)";

fn denied() -> io::Error {
  io::Error::new(io::ErrorKind::PermissionDenied, "Device storage requires a LocalSystem-owned, SYSTEM-only object")
}

fn wide(value: &OsStr) -> Vec<u16> {
  value.encode_wide().chain(Some(0)).collect()
}

pub fn require_system_host() -> io::Result<()> {
  let mut session = u32::MAX;
  // SAFETY: Windows writes one live session ID for this process.
  unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut session) }.map_err(|_| denied())?;

  if session != 0 {
    return Err(denied());
  }

  let mut token = HANDLE::default();
  // SAFETY: Windows returns one owned token handle for the current process.
  unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }.map_err(|_| denied())?;
  // SAFETY: OpenProcessToken returned one uniquely owned kernel handle.
  let token = unsafe { std::os::windows::io::OwnedHandle::from_raw_handle(token.0) };
  let mut bytes = 0u32;
  // SAFETY: A null output buffer queries the required TOKEN_USER size.
  let _ = unsafe { GetTokenInformation(HANDLE(token.as_raw_handle()), TokenUser, None, 0, &mut bytes) };

  if bytes < size_of::<TOKEN_USER>() as u32 || bytes > 64 * 1024 || align_of::<TOKEN_USER>() > align_of::<usize>() {
    return Err(denied());
  }

  let mut data = vec![0usize; (bytes as usize).div_ceil(size_of::<usize>())];
  // SAFETY: The aligned word buffer has the queried capacity and stays live
  // while Windows writes TOKEN_USER and IsWellKnownSid reads its embedded SID.
  unsafe { GetTokenInformation(HANDLE(token.as_raw_handle()), TokenUser, Some(data.as_mut_ptr().cast()), bytes, &mut bytes) }
    .map_err(|_| denied())?;

  if (bytes as usize) < size_of::<TOKEN_USER>() {
    return Err(denied());
  }

  // SAFETY: The successful call initialized an aligned TOKEN_USER header.
  let user = unsafe { data.as_ptr().cast::<TOKEN_USER>().read() };

  if user.User.Sid.0.is_null() || !unsafe { IsWellKnownSid(user.User.Sid, WinLocalSystemSid) }.as_bool() {
    return Err(denied());
  }

  Ok(())
}

pub struct Descriptor(pub(crate) PSECURITY_DESCRIPTOR);

impl Descriptor {
  pub fn system_only() -> io::Result<Self> {
    let text = wide(OsStr::new(SYSTEM_OWNER_AND_DACL));
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    // SAFETY: The SDDL string and output pointer are live during conversion.
    unsafe { ConvertStringSecurityDescriptorToSecurityDescriptorW(PCWSTR(text.as_ptr()), SDDL_REVISION_1, &mut descriptor, None) }
      .map_err(io::Error::other)?;
    Ok(Self(descriptor))
  }

  pub fn attributes(&self) -> SECURITY_ATTRIBUTES {
    SECURITY_ATTRIBUTES {
      nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
      lpSecurityDescriptor: self.0.0,
      bInheritHandle: false.into(),
    }
  }
}

impl Drop for Descriptor {
  fn drop(&mut self) {
    // SAFETY: SDDL conversion returned this LocalAlloc descriptor once.
    unsafe { LocalFree(HLOCAL(self.0.0)) };
  }
}

pub fn verify_object(file: &File, directory: bool) -> io::Result<()> {
  let handle = HANDLE(file.as_raw_handle());
  let mut information = BY_HANDLE_FILE_INFORMATION::default();
  // SAFETY: Windows writes the initialized BY_HANDLE_FILE_INFORMATION value.
  unsafe { GetFileInformationByHandle(handle, &mut information) }.map_err(|_| denied())?;
  let attributes = information.dwFileAttributes;

  if attributes & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 || (attributes & FILE_ATTRIBUTE_DIRECTORY.0 != 0) != directory {
    return Err(denied());
  }

  let mut descriptor = PSECURITY_DESCRIPTOR::default();
  let security_info = OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION;
  // SAFETY: Windows returns one LocalAlloc security descriptor for this open
  // handle; the handle stays live and cannot be substituted by a path rename.
  unsafe { GetSecurityInfo(handle, SE_FILE_OBJECT, security_info, None, None, None, None, Some(&mut descriptor)) }
    .ok()
    .map_err(|_| denied())?;

  if descriptor.0.is_null() {
    return Err(denied());
  }

  let mut output = PWSTR::null();
  // SAFETY: The descriptor is live; conversion allocates the output SDDL.
  let converted =
    unsafe { ConvertSecurityDescriptorToStringSecurityDescriptorW(descriptor, SDDL_REVISION_1, security_info, &mut output, None) };
  // SAFETY: Release the one descriptor returned by GetSecurityInfo.
  unsafe { LocalFree(HLOCAL(descriptor.0)) };
  converted.map_err(|_| denied())?;
  // SAFETY: The returned SDDL string remains live through decoding.
  let actual = unsafe { output.to_string() }.map_err(|_| denied());
  // SAFETY: Release the one SDDL allocation returned by conversion.
  unsafe { LocalFree(HLOCAL(output.0.cast())) };

  if actual? != SYSTEM_OBJECT_SDDL {
    return Err(denied());
  }

  Ok(())
}
