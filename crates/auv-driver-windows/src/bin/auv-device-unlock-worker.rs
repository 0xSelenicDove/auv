//! One-shot Windows console worker. Installed only with the privileged host.

#[cfg(target_os = "windows")]
fn main() {
  let mut args = std::env::args().skip(1);
  let Some(first) = args.next() else {
    std::process::exit(2)
  };
  if first == "--preflight-host" {
    if args.next().is_some() {
      std::process::exit(2);
    }
    finish_host_preflight(auv_driver_windows::device_unlock_host::run_preflight_host());
  }
  if first == "--preflight-pipe-host" {
    if args.next().is_some() {
      std::process::exit(2);
    }
    finish_host_preflight(auv_driver_windows::device_unlock_host::run_pipe_preflight_host());
  }
  if first == "--preflight-transition-host" {
    if args.next().is_some() {
      std::process::exit(2);
    }
    finish_host_preflight(auv_driver_windows::device_unlock_host::run_transition_preflight_host());
  }
  if first == "--preflight-transition-full-host" {
    if args.next().is_some() {
      std::process::exit(2);
    }
    let report = auv_driver_windows::device_unlock_host::run_full_transition_preflight_host();
    if let (Some(inserted), Some(win32_error)) = (report.inserted, report.win32_error) {
      println!("preflight={} code={} count={} win32_error={}", report.code.as_str(), report.code.exit_code(), inserted, win32_error);
    } else {
      println!("preflight={} code={}", report.code.as_str(), report.code.exit_code());
    }
    std::process::exit(if report.code == auv_driver_windows::device_unlock_host::PreflightCode::TransitionReady {
      0
    } else {
      report.code.exit_code() as i32
    });
  }
  if first == "--preflight" {
    let Some(session) = args.next().and_then(|value| value.parse::<u32>().ok()) else {
      std::process::exit(2)
    };
    let Some(logon) = args.next().and_then(|value| value.parse::<i64>().ok()) else {
      std::process::exit(2)
    };
    let Some(sid) = args.next() else {
      std::process::exit(2)
    };
    if args.next().is_some() {
      std::process::exit(2);
    }
    let code = auv_driver_windows::device_unlock_host::run_preflight_worker(session, logon, &sid);
    std::process::exit(code.exit_code() as i32);
  }
  if first == "--preflight-pipe" {
    let Some(pipe) = args.next() else {
      std::process::exit(2)
    };
    let Some(session) = args.next().and_then(|value| value.parse::<u32>().ok()) else {
      std::process::exit(2)
    };
    let Some(logon) = args.next().and_then(|value| value.parse::<i64>().ok()) else {
      std::process::exit(2)
    };
    let Some(sid) = args.next() else {
      std::process::exit(2)
    };
    if args.next().is_some() {
      std::process::exit(2);
    }
    let code = auv_driver_windows::device_unlock_host::run_pipe_preflight_worker(&pipe, session, logon, &sid);
    std::process::exit(code.exit_code() as i32);
  }
  if first == "--preflight-transition" {
    let Some(session) = args.next().and_then(|value| value.parse::<u32>().ok()) else {
      std::process::exit(2)
    };
    let Some(logon) = args.next().and_then(|value| value.parse::<i64>().ok()) else {
      std::process::exit(2)
    };
    let Some(sid) = args.next() else {
      std::process::exit(2)
    };
    if args.next().is_some() {
      std::process::exit(2);
    }
    let code = auv_driver_windows::device_unlock_host::run_transition_preflight_worker(session, logon, &sid);
    std::process::exit(code.exit_code() as i32);
  }
  if first == "--preflight-transition-full" {
    let Some(session) = args.next().and_then(|value| value.parse::<u32>().ok()) else {
      std::process::exit(2)
    };
    let Some(logon) = args.next().and_then(|value| value.parse::<i64>().ok()) else {
      std::process::exit(2)
    };
    let Some(sid) = args.next() else {
      std::process::exit(2)
    };
    if args.next().is_some() {
      std::process::exit(2);
    }
    let exit = auv_driver_windows::device_unlock_host::run_full_transition_preflight_worker(session, logon, &sid);
    std::process::exit(exit as i32);
  }
  let pipe = first;
  let Some(session) = args.next().and_then(|value| value.parse::<u32>().ok()) else {
    std::process::exit(2)
  };
  let Some(logon) = args.next().and_then(|value| value.parse::<i64>().ok()) else {
    std::process::exit(2)
  };
  let Some(sid) = args.next() else {
    std::process::exit(2)
  };
  if args.next().is_some() {
    std::process::exit(2);
  }
  // Exit status is intentionally coarse. Native errors and credentials are
  // never formatted to stdout, stderr, a file, or the process command line.
  if auv_driver_windows::device_unlock_host::run_worker(&pipe, session, logon, &sid).is_err() {
    std::process::exit(1);
  }
}

#[cfg(target_os = "windows")]
fn finish_host_preflight(code: auv_driver_windows::device_unlock_host::PreflightCode) -> ! {
  println!("preflight={} code={}", code.as_str(), code.exit_code());
  std::process::exit(
    if matches!(
      code,
      auv_driver_windows::device_unlock_host::PreflightCode::ReadyDefault
        | auv_driver_windows::device_unlock_host::PreflightCode::ReadyWinlogon
        | auv_driver_windows::device_unlock_host::PreflightCode::TransitionReady
    ) {
      0
    } else {
      code.exit_code() as i32
    },
  );
}

#[cfg(not(target_os = "windows"))]
fn main() {
  std::process::exit(2);
}
