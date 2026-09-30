//! Per-user Aqua host for the existing locked macOS console session.

mod session;
mod vault;

use std::fs::{self, DirBuilder};
use std::os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rustix::process::{geteuid, getuid};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use zeroize::Zeroizing;

use crate::{HostError, InputFailure, MAGIC, MAX_PAYLOAD, Operation, VERSION, socket_path};

pub async fn serve() -> Result<(), HostError> {
  let uid = getuid().as_raw();

  if uid == 0 || geteuid().as_raw() != uid {
    return Err(HostError::Unauthorized);
  }

  let home = PathBuf::from(std::env::var_os("HOME").ok_or(HostError::Unavailable)?);
  prepare_socket_dir(&home, uid)?;
  let path = socket_path(&home);

  if let Ok(metadata) = fs::symlink_metadata(&path) {
    if !metadata.file_type().is_socket() || metadata.uid() != uid {
      return Err(HostError::Unauthorized);
    }

    fs::remove_file(&path).map_err(|_| HostError::Unavailable)?;
  }

  let listener = UnixListener::bind(&path).map_err(|_| HostError::Unavailable)?;
  fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).map_err(|_| HostError::Unavailable)?;
  // A serial accept loop prevents concurrent attempts for this one account.
  // The daemon still serializes at the Device policy boundary.
  loop {
    let (mut stream, _) = listener.accept().await.map_err(|_| HostError::Unavailable)?;
    let status = match tokio::time::timeout(Duration::from_secs(18), handle(&mut stream, &home, uid)).await {
      Ok(Ok(())) => 0,
      Ok(Err(error)) => status(error),
      Err(_) => status(HostError::Unavailable),
    };
    let _ = stream.write_all(&[status]).await;
  }
}

fn prepare_socket_dir(home: &Path, uid: u32) -> Result<(), HostError> {
  if !home.is_absolute() {
    return Err(HostError::Unavailable);
  }

  let library = home.join("Library");
  let support = library.join("Application Support");
  let auv = support.join("AUV");
  let entry = auv.join("device-entry");

  for path in [home, &library, &support, &auv, &entry] {
    if !path.exists() {
      DirBuilder::new().mode(0o700).create(path).map_err(|_| HostError::Unavailable)?;
    }

    let metadata = fs::symlink_metadata(path).map_err(|_| HostError::Unavailable)?;

    if !metadata.file_type().is_dir() || metadata.uid() != uid || metadata.mode() & 0o022 != 0 {
      return Err(HostError::Unauthorized);
    }
  }

  // The private directory restricts discovery. The protocol admits only
  // the root daemon so every vault mutation has matching policy metadata.
  fs::set_permissions(entry, fs::Permissions::from_mode(0o700)).map_err(|_| HostError::Unavailable)
}

async fn handle(stream: &mut UnixStream, home: &Path, uid: u32) -> Result<(), HostError> {
  let started = Instant::now();
  let peer = stream.peer_cred().map_err(|_| HostError::Unauthorized)?.uid();
  // Every operation must pass through the root daemon's local principal
  // check, metadata generation, account lock, and audit boundary. A user
  // process reaching its own socket cannot bypass those checks by directly
  // replacing/removing a credential or triggering locked-session input.
  if peer != 0 {
    return Err(HostError::Unauthorized);
  }

  let mut header = [0_u8; 12];
  stream.read_exact(&mut header).await.map_err(|_| HostError::InvalidRequest)?;
  let (operation, length) = decode_header(&header, uid)?;
  let mut payload = Zeroizing::new(vec![0_u8; length]);
  stream.read_exact(&mut payload).await.map_err(|_| HostError::InvalidRequest)?;

  match operation {
    Operation::Enroll if !payload.is_empty() => {
      let secret = std::str::from_utf8(&payload).map_err(|_| HostError::InvalidRequest)?;

      if secret.chars().any(char::is_control) {
        return Err(HostError::InvalidRequest);
      }

      vault::enroll(home, uid, &payload)
    }
    Operation::Probe | Operation::Unlock if !payload.is_empty() => {
      let selector = std::str::from_utf8(&payload).map_err(|_| HostError::InvalidRequest)?;

      if operation == Operation::Probe {
        session::probe_locked(home, uid, selector)
      } else {
        // The native primitive clears retained input without a preliminary
        // Return. DeviceService verifies the same session after this helper
        // returns; see the installed gate in the macOS session reference.
        session::unlock(home, uid, selector, started)
      }
    }
    Operation::Remove if payload.is_empty() => vault::remove(home, uid),
    _ => Err(HostError::InvalidRequest),
  }
}

fn decode_header(header: &[u8; 12], uid: u32) -> Result<(Operation, usize), HostError> {
  if &header[..4] != MAGIC || header[4] != VERSION || u32::from_be_bytes(header[6..10].try_into().unwrap()) != uid {
    return Err(HostError::Unauthorized);
  }

  let length = usize::from(u16::from_be_bytes(header[10..12].try_into().unwrap()));

  if length > MAX_PAYLOAD {
    return Err(HostError::InvalidRequest);
  }

  Ok((Operation::try_from(header[5])?, length))
}

fn status(error: HostError) -> u8 {
  match error {
    HostError::Unavailable => 8,
    HostError::Unauthorized => 1,
    HostError::InvalidRequest => 2,
    HostError::StaleSession => 3,
    HostError::NotLocked => 4,
    HostError::VaultUnavailable => 5,
    HostError::InputUnavailable => 6,
    HostError::InputUnavailableAt(InputFailure::InvalidRequest) => 9,
    HostError::InputUnavailableAt(InputFailure::IdentityMismatch) => 10,
    HostError::InputUnavailableAt(InputFailure::PermissionMissing) => 11,
    HostError::InputUnavailableAt(InputFailure::SessionChanged) => 12,
    HostError::InputUnavailableAt(InputFailure::EventUnavailable) => 13,
    HostError::InputUnavailableAt(InputFailure::WakeUnavailable) => 14,
    HostError::InputUnavailableAt(InputFailure::FocusUnavailable) => 15,
    HostError::InputUnavailableAt(InputFailure::FocusLost) => 16,
    HostError::InputUnavailableAt(InputFailure::DeadlineExceeded) => 17,
    HostError::InputUnavailableAt(InputFailure::Unavailable) => 6,
    HostError::OutcomeUnverified => 7,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn request_for_another_uid_is_rejected_before_vault_access() {
    let uid = getuid().as_raw();
    let mut header = [0_u8; 12];
    header[..4].copy_from_slice(MAGIC);
    header[4] = VERSION;
    header[5] = Operation::Probe as u8;
    header[6..10].copy_from_slice(&uid.wrapping_add(1).to_be_bytes());

    assert_eq!(decode_header(&header, uid), Err(HostError::Unauthorized));
  }

  #[test]
  fn unknown_operation_is_an_invalid_request() {
    let uid = getuid().as_raw();
    let mut header = [0_u8; 12];
    header[..4].copy_from_slice(MAGIC);
    header[4] = VERSION;
    header[5] = 5;
    header[6..10].copy_from_slice(&uid.to_be_bytes());

    assert_eq!(decode_header(&header, uid), Err(HostError::InvalidRequest));
    assert_eq!(status(HostError::InvalidRequest), 2);
  }

  #[test]
  fn private_unlock_stages_survive_the_helper_response_byte() {
    for stage in [
      InputFailure::InvalidRequest,
      InputFailure::IdentityMismatch,
      InputFailure::PermissionMissing,
      InputFailure::SessionChanged,
      InputFailure::EventUnavailable,
      InputFailure::WakeUnavailable,
      InputFailure::FocusUnavailable,
      InputFailure::FocusLost,
      InputFailure::DeadlineExceeded,
    ] {
      let error = HostError::InputUnavailableAt(stage);

      assert_eq!(crate::decode_status(status(error)), Err(error));
    }

    assert_eq!(crate::decode_status(6), Err(HostError::InputUnavailable));
  }

  #[tokio::test]
  async fn same_uid_cannot_bypass_daemon_policy_for_any_operation() {
    let uid = getuid().as_raw();

    if uid == 0 {
      return;
    }

    for operation in [
      Operation::Enroll,
      Operation::Probe,
      Operation::Remove,
      Operation::Unlock,
    ] {
      let (mut client, mut server) = UnixStream::pair().unwrap();
      let mut header = [0_u8; 12];
      header[..4].copy_from_slice(MAGIC);
      header[4] = VERSION;
      header[5] = operation as u8;
      header[6..10].copy_from_slice(&uid.to_be_bytes());
      client.write_all(&header).await.unwrap();

      assert_eq!(handle(&mut server, Path::new("/missing"), uid).await, Err(HostError::Unauthorized));
    }
  }
}
