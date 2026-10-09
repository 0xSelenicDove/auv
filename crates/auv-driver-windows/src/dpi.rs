//! Keep Win32 geometry in the physical screen units used by DWM frames.
//!
//! Scope awareness to each synchronous native operation: callers may embed the
//! driver in a DPI-unaware application, and async workers need their own scope.

#[cfg(target_os = "windows")]
pub(crate) struct DpiScope {
  previous: windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT,
  // The previous context belongs to this thread; never restore it on another.
  _thread: std::marker::PhantomData<std::rc::Rc<()>>,
}

#[cfg(target_os = "windows")]
impl DpiScope {
  pub(crate) fn physical_pixels() -> auv_driver_common::DriverResult<Self> {
    Self::enter(windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)
  }

  /// PrintWindow asks the target application to render. Its GDI extent follows
  /// the target's awareness, rather than the caller's physical frame. Size and
  /// render the DIB within the same context to avoid black padding.
  pub(crate) fn window_rendering(hwnd: windows::Win32::Foundation::HWND) -> auv_driver_common::DriverResult<Self> {
    // SAFETY: the selected HWND supplies a context, not a global setting.
    Self::enter(unsafe { windows::Win32::UI::HiDpi::GetWindowDpiAwarenessContext(hwnd) })
  }

  fn enter(context: windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT) -> auv_driver_common::DriverResult<Self> {
    use windows::Win32::UI::HiDpi::SetThreadDpiAwarenessContext;
    // SAFETY: this changes only the calling thread and returns its old context.
    let previous = unsafe { SetThreadDpiAwarenessContext(context) };
    if previous.0.is_null() {
      return Err(crate::error::backend("SetThreadDpiAwarenessContext failed"));
    }
    Ok(Self {
      previous,
      _thread: std::marker::PhantomData,
    })
  }
}

#[cfg(target_os = "windows")]
impl Drop for DpiScope {
  fn drop(&mut self) {
    // SAFETY: the guard cannot cross threads and restores a context returned by
    // SetThreadDpiAwarenessContext, including on error and unwinding paths.
    unsafe {
      windows::Win32::UI::HiDpi::SetThreadDpiAwarenessContext(self.previous);
    }
  }
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
  use super::DpiScope;
  use windows::Win32::UI::HiDpi::{
    AreDpiAwarenessContextsEqual, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, DPI_AWARENESS_CONTEXT_UNAWARE, GetThreadDpiAwarenessContext,
    SetThreadDpiAwarenessContext,
  };

  #[test]
  fn nested_scopes_restore_unaware_host_after_error() {
    std::thread::spawn(|| unsafe {
      let original = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_UNAWARE);
      assert!(!original.0.is_null());
      let operation = || -> auv_driver_common::DriverResult<()> {
        let _outer = DpiScope::physical_pixels()?;
        {
          let _inner = DpiScope::physical_pixels()?;
          assert!(AreDpiAwarenessContextsEqual(GetThreadDpiAwarenessContext(), DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2).as_bool());
        }
        assert!(AreDpiAwarenessContextsEqual(GetThreadDpiAwarenessContext(), DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2).as_bool());
        Err(crate::error::backend("test error"))
      };
      assert!(operation().is_err());
      assert!(AreDpiAwarenessContextsEqual(GetThreadDpiAwarenessContext(), DPI_AWARENESS_CONTEXT_UNAWARE).as_bool());
      SetThreadDpiAwarenessContext(original);
    })
    .join()
    .unwrap();
  }

  #[test]
  #[ignore = "creates a dedicated Win32 window without activation; exercises high-DPI PrintWindow sizing"]
  fn unaware_window_capture_uses_target_render_extent_from_aware_host() {
    use auv_driver_common::{CoordinateSpace, Rect, Window, WindowRef};
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::UI::WindowsAndMessaging::{
      CreateWindowExW, DestroyWindow, GetWindowRect, SW_SHOWNOACTIVATE, ShowWindow, WINDOW_EX_STYLE, WS_OVERLAPPEDWINDOW,
    };
    use windows::core::w;

    struct OwnedWindow(HWND);
    impl Drop for OwnedWindow {
      fn drop(&mut self) {
        // SAFETY: this test owns the window on its creating thread.
        unsafe {
          let _ = DestroyWindow(self.0);
        }
      }
    }

    let (owned, expected) = {
      let _unaware = DpiScope::enter(DPI_AWARENESS_CONTEXT_UNAWARE).unwrap();
      // SAFETY: STATIC is a built-in class and all optional pointers are null.
      // The window receives no desktop input and is shown without activation.
      let owned = OwnedWindow(unsafe {
        CreateWindowExW(
          WINDOW_EX_STYLE::default(),
          w!("STATIC"),
          w!("AUV DPI capture regression"),
          WS_OVERLAPPEDWINDOW,
          100,
          100,
          400,
          300,
          None,
          None,
          None,
          None,
        )
        .unwrap()
      });
      let mut rect = RECT::default();
      unsafe {
        let _ = ShowWindow(owned.0, SW_SHOWNOACTIVATE);
        GetWindowRect(owned.0, &mut rect).unwrap();
      }
      (owned, (rect.right - rect.left, rect.bottom - rect.top))
    };
    let _host = DpiScope::physical_pixels().unwrap();
    let window = Window {
      reference: WindowRef {
        id: (owned.0.0 as isize).to_string(),
      },
      title: None,
      app_name: None,
      app_bundle_id: None,
      process_id: None,
      frame: Rect::new(100.0, 100.0, 400.0, 300.0),
      coordinate_space: CoordinateSpace::Screen,
      is_main: false,
      is_visible: false,
    };
    let capture = crate::capture::capture_window(&window).unwrap();
    assert_eq!((capture.image.width(), capture.image.height()), (expected.0 as u32, expected.1 as u32));
    assert!(unsafe { AreDpiAwarenessContextsEqual(GetThreadDpiAwarenessContext(), DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) }.as_bool());
  }
}
