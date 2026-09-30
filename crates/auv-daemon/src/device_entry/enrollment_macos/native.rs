//! Native macOS account lookup through the system password database.
//! Enrollment and the unlock host consume resolved account facts only.

use std::ffi::{CStr, CString};
use std::path::PathBuf;
use std::ptr;

use super::super::local::unix_account_id as account_id;
use auv_api_server::device_local::LocalControlError;

#[derive(Debug, Eq, PartialEq)]
pub(in crate::device_entry) struct Account {
  pub name: String,
  pub uid: u32,
  pub id: String,
  pub home: PathBuf,
}

pub(super) fn resolve_user(user: &str) -> Result<Account, LocalControlError> {
  if user.is_empty() || user.trim() != user || user.len() > 256 {
    return Err(LocalControlError::InvalidAccount);
  }

  let name = CString::new(user).map_err(|_| LocalControlError::InvalidAccount)?;
  passwd_lookup(|record, buffer, found| {
    // SAFETY: the CString and output pointers remain live through this call.
    unsafe { libc::getpwnam_r(name.as_ptr(), record, buffer.as_mut_ptr().cast(), buffer.len(), found) }
  })
}

pub(in crate::device_entry) fn resolve_uid(uid: u32) -> Result<Account, LocalControlError> {
  passwd_lookup(|record, buffer, found| {
    // SAFETY: the output pointers and buffer remain live through this call.
    unsafe { libc::getpwuid_r(uid, record, buffer.as_mut_ptr().cast(), buffer.len(), found) }
  })
}

fn passwd_lookup(
  mut lookup: impl FnMut(&mut libc::passwd, &mut [u8], &mut *mut libc::passwd) -> libc::c_int,
) -> Result<Account, LocalControlError> {
  let mut buffer = vec![0u8; 4096];

  loop {
    // SAFETY: all-zero passwd is only an output record; no field is read
    // until a successful lookup has populated it.
    let mut record: libc::passwd = unsafe { std::mem::zeroed() };
    let mut found = ptr::null_mut();
    let result = lookup(&mut record, &mut buffer, &mut found);

    if result == libc::ERANGE && buffer.len() < 1024 * 1024 {
      buffer.resize(buffer.len() * 2, 0);
      continue;
    }

    if result != 0 {
      return Err(LocalControlError::Persistence);
    }

    if found.is_null() || record.pw_name.is_null() || record.pw_dir.is_null() {
      return Err(LocalControlError::InvalidAccount);
    }

    // SAFETY: a successful getpw*_r supplies NUL-terminated pointers that
    // remain valid while this output buffer is live. Copy them immediately.
    let name = unsafe { CStr::from_ptr(record.pw_name) }.to_str().map_err(|_| LocalControlError::InvalidAccount)?.to_owned();
    // SAFETY: same successful lookup and output-buffer lifetime as pw_name.
    let home = unsafe { CStr::from_ptr(record.pw_dir) }.to_str().map_err(|_| LocalControlError::InvalidAccount)?;
    let home = PathBuf::from(home);

    if name.is_empty() || !home.is_absolute() {
      return Err(LocalControlError::InvalidAccount);
    }

    let uid = record.pw_uid;

    return Ok(Account {
      name,
      uid,
      id: account_id(uid),
      home,
    });
  }
}
