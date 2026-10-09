//! Window accessibility tree snapshots via Microsoft UI Automation (UIA).
//!
//! Mirrors the spirit of the macOS driver's AX tree capture: it walks a single
//! window's accessibility hierarchy and flattens it into an ordered list of
//! nodes carrying role, name, identifier, and screen bounds. macOS reads the
//! `AXUIElement` tree; Windows reads the UIA tree through the COM
//! `IUIAutomation` interface and its control-view tree walker.
//!
//! The snapshot is read-only structure: it captures what the tree looks like,
//! not how to act on it. Acting on a node (invoke/focus/value) is a separate
//! concern delivered through the input and control surfaces.
// TODO(windows-ax-value-write): UIA ValuePattern writes remain deferred until
// an owner-approved consumer needs them. Read-only ValuePattern text,
// path-targeted SetFocus, and result-item selection are exposed for Apple Music.

use auv_driver_common::error::DriverResult;
use auv_driver_common::geometry::Rect;
use auv_driver_common::input::{DisturbanceLevel, InputActionResult, InputAttempt, InputDeliveryPath};
use auv_driver_common::window::Window;

use crate::error::invalid_input;

// NOTICE: traversal bounds keep a pathological or very deep UI tree from
// producing an unbounded snapshot (and guard the recursive walk against deep
// stacks). They are independent limits: depth caps how far down we descend,
// node count caps total breadth-times-depth output.
pub const MAX_DEPTH: usize = 40;
pub const MAX_NODES: usize = 2_000;

/// One node in a flattened accessibility tree snapshot.
///
/// `path` is a `/`-joined chain of child indices from the root (e.g. `0/2/1`),
/// so a node's position in the original tree is recoverable from the flat list.
#[derive(Clone, Debug, PartialEq)]
pub struct AxNode {
  pub depth: usize,
  pub path: String,
  pub control_type: String,
  pub name: String,
  /// Current text exposed through UIA ValuePattern, when supported.
  pub value: Option<String>,
  pub automation_id: String,
  pub class_name: String,
  pub focused: bool,
  pub bounds: Rect,
}

/// A flattened, depth-first accessibility tree snapshot for one window.
#[derive(Clone, Debug, PartialEq)]
pub struct AxTreeSnapshot {
  pub window_ref: String,
  pub nodes: Vec<AxNode>,
}

/// Captures the accessibility tree for `window` via UI Automation.
pub fn snapshot_window(window: &Window) -> DriverResult<AxTreeSnapshot> {
  native::snapshot_window(window)
}

/// Moves keyboard focus to a node from a fresh [`AxTreeSnapshot`].
///
/// The node path is resolved again against the current UIA control-view tree,
/// then `IUIAutomationElement::SetFocus` performs the action. Callers should
/// refresh the snapshot and retry when the tree changes between observation
/// and action.
pub fn focus_node(window: &Window, node_path: &str) -> DriverResult<InputActionResult> {
  let indices = child_indices(node_path)?;
  native::focus_node(window, &indices)?;
  Ok(InputActionResult {
    selected_path: InputDeliveryPath::AxFocus,
    attempts: vec![InputAttempt::success(InputDeliveryPath::AxFocus)],
    verified: false,
    mouse_disturbance: DisturbanceLevel::None,
    focus_disturbance: DisturbanceLevel::Foreground,
    clipboard_disturbance: DisturbanceLevel::None,
  })
}

/// Selects or invokes a node from a recent [`AxTreeSnapshot`].
///
/// `SelectionItemPattern::Select` is preferred because result containers such
/// as WinUI `GridViewItem` expose semantic selection. `InvokePattern::Invoke`
/// is the typed fallback for actionable nodes that do not expose selection.
pub fn select_node(window: &Window, node_path: &str) -> DriverResult<InputActionResult> {
  let indices = child_indices(node_path)?;
  let selected_pattern = native::select_node(window, &indices)?;
  Ok(InputActionResult {
    selected_path: InputDeliveryPath::AxPress,
    attempts: if selected_pattern == "InvokePattern.Invoke" {
      vec![
        InputAttempt::failure(InputDeliveryPath::AxPress, "SelectionItemPattern was unavailable"),
        InputAttempt {
          path: InputDeliveryPath::AxPress,
          succeeded: true,
          message: Some(selected_pattern.to_string()),
        },
      ]
    } else {
      vec![InputAttempt {
        path: InputDeliveryPath::AxPress,
        succeeded: true,
        message: Some(selected_pattern.to_string()),
      }]
    },
    verified: false,
    mouse_disturbance: DisturbanceLevel::None,
    focus_disturbance: DisturbanceLevel::Foreground,
    clipboard_disturbance: DisturbanceLevel::None,
  })
}

fn child_indices(path: &str) -> DriverResult<Vec<usize>> {
  let mut parts = path.split('/');
  if parts.next() != Some("0") {
    return Err(invalid_input(format!("UIA node path {path:?} must start at root 0")));
  }
  parts
    .map(|part| part.parse::<usize>().map_err(|_| invalid_input(format!("UIA node path {path:?} contains invalid child index {part:?}"))))
    .collect()
}

/// Builds a screen-space rectangle from UIA bounding-rectangle edges.
///
/// UIA reports bounds as inclusive `left/top` and exclusive `right/bottom`
/// edges in physical screen pixels; this converts them to an origin/size
/// rectangle in the same screen space as window frames.
fn rect_from_edges(left: i32, top: i32, right: i32, bottom: i32) -> Rect {
  Rect::new(f64::from(left), f64::from(top), f64::from(right - left), f64::from(bottom - top))
}

#[cfg(target_os = "windows")]
mod native {
  use auv_driver_common::error::DriverResult;
  use auv_driver_common::geometry::Rect;
  use auv_driver_common::window::Window;
  use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize};
  use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationCacheRequest, IUIAutomationElement, IUIAutomationInvokePattern,
    IUIAutomationSelectionItemPattern, IUIAutomationTreeWalker, IUIAutomationValuePattern, TreeScope_Element, UIA_AutomationIdPropertyId,
    UIA_BoundingRectanglePropertyId, UIA_ClassNamePropertyId, UIA_HasKeyboardFocusPropertyId, UIA_InvokePatternId,
    UIA_LocalizedControlTypePropertyId, UIA_NamePropertyId, UIA_SelectionItemPatternId, UIA_ValuePatternId, UIA_ValueValuePropertyId,
  };
  use windows::core::{BSTR, Result as WindowsResult};

  use super::{AxNode, AxTreeSnapshot, MAX_DEPTH, MAX_NODES, rect_from_edges};
  use crate::error::backend;
  use crate::window::window_handle;

  /// Balances a successful `CoInitializeEx` with `CoUninitialize` on the same
  /// thread. When COM was already initialized in a different apartment model
  /// (`RPC_E_CHANGED_MODE`), `uninit` stays false so we do not tear down an
  /// initialization we did not perform.
  struct ComGuard {
    uninit: bool,
  }

  impl Drop for ComGuard {
    fn drop(&mut self) {
      if self.uninit {
        unsafe { CoUninitialize() };
      }
    }
  }

  fn init_com() -> ComGuard {
    // UIA is happiest in an MTA; if the thread is already STA this returns
    // RPC_E_CHANGED_MODE and we proceed against the existing apartment.
    let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    ComGuard { uninit: hr.is_ok() }
  }

  pub(super) fn snapshot_window(window: &Window) -> DriverResult<AxTreeSnapshot> {
    let hwnd = window_handle(window)?;
    let _com = init_com();

    let automation: IUIAutomation = unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }
      .map_err(|error| backend(format!("failed to create UI Automation client: {error}")))?;
    let root =
      unsafe { automation.ElementFromHandle(hwnd) }.map_err(|error| backend(format!("failed to resolve window UI element: {error}")))?;
    let walker =
      unsafe { automation.ControlViewWalker() }.map_err(|error| backend(format!("failed to get UI Automation control walker: {error}")))?;

    // One fresh request per snapshot; Element scope avoids fetching an
    // unbounded subtree before the existing traversal limits can stop it.
    // The caching idea comes from Windows-MCP; see THIRD_PARTY_NOTICES.md
    // and docs/ai/references/driver/2026-10-08-windows-accessibility-text.md.
    let cache = snapshot_cache(&automation).ok();
    let cached_root = cache.as_ref().and_then(|request| unsafe { root.BuildUpdatedCache(request) }.ok());
    let mut nodes = Vec::new();
    walk(&walker, cached_root.as_ref().unwrap_or(&root), cache.as_ref(), cached_root.is_some(), 0, "0".to_string(), &mut nodes);
    Ok(AxTreeSnapshot {
      window_ref: window.reference.id.clone(),
      nodes,
    })
  }

  pub(super) fn focus_node(window: &Window, child_indices: &[usize]) -> DriverResult<()> {
    let (_com, element) = resolve_element(window, child_indices)?;
    unsafe { element.SetFocus() }.map_err(|error| backend(format!("failed to focus UIA node: {error}")))
  }

  pub(super) fn select_node(window: &Window, child_indices: &[usize]) -> DriverResult<&'static str> {
    let (_com, element) = resolve_element(window, child_indices)?;
    if let Ok(pattern) = unsafe { element.GetCurrentPatternAs::<IUIAutomationSelectionItemPattern>(UIA_SelectionItemPatternId) } {
      unsafe { pattern.Select() }.map_err(|error| backend(format!("failed to select UIA node: {error}")))?;
      return Ok("SelectionItemPattern.Select");
    }
    let pattern = unsafe { element.GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId) }
      .map_err(|error| backend(format!("UIA node supports neither SelectionItemPattern nor InvokePattern: {error}")))?;
    unsafe { pattern.Invoke() }.map_err(|error| backend(format!("failed to invoke UIA node: {error}")))?;
    Ok("InvokePattern.Invoke")
  }

  fn resolve_element(window: &Window, child_indices: &[usize]) -> DriverResult<(ComGuard, IUIAutomationElement)> {
    let hwnd = window_handle(window)?;
    let com = init_com();
    let automation: IUIAutomation = unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }
      .map_err(|error| backend(format!("failed to create UI Automation client: {error}")))?;
    let mut element =
      unsafe { automation.ElementFromHandle(hwnd) }.map_err(|error| backend(format!("failed to resolve window UI element: {error}")))?;
    let walker =
      unsafe { automation.ControlViewWalker() }.map_err(|error| backend(format!("failed to get UI Automation control walker: {error}")))?;
    for index in child_indices {
      let mut child =
        unsafe { walker.GetFirstChildElement(&element) }.map_err(|error| backend(format!("failed to resolve UIA child 0: {error}")))?;
      for sibling_index in 0..*index {
        child = unsafe { walker.GetNextSiblingElement(&child) }
          .map_err(|error| backend(format!("failed to resolve UIA child {}: {error}", sibling_index + 1)))?;
      }
      element = child;
    }
    Ok((com, element))
  }

  /// Depth-first traversal that appends each visited element, then descends
  /// through its control-view children. Traversal stops widening once the node
  /// budget is exhausted and stops descending past the depth limit.
  fn walk(
    walker: &IUIAutomationTreeWalker,
    element: &IUIAutomationElement,
    cache: Option<&IUIAutomationCacheRequest>,
    cached: bool,
    depth: usize,
    path: String,
    nodes: &mut Vec<AxNode>,
  ) {
    if nodes.len() >= MAX_NODES {
      return;
    }
    // Cache only this visited element. Providers that reject caching retain
    // the original live-property path; cached read failures also fall back.
    nodes.push(node_from_element(element, cached, depth, path.clone()));
    if depth >= MAX_DEPTH {
      return;
    }

    let mut next = unsafe {
      if let Some(request) = cache {
        walker.GetFirstChildElementBuildCache(element, request).map(|child| (child, true)).or_else(|error| {
          // windows-core 0.58 converts a successful null interface to
          // Error::empty (S_OK): no child, not a cache failure.
          if error.code().is_ok() {
            Err(error)
          } else {
            walker.GetFirstChildElement(element).map(|child| (child, false))
          }
        })
      } else {
        walker.GetFirstChildElement(element).map(|child| (child, false))
      }
    }
    .ok();
    let mut index = 0usize;
    while let Some((child, child_cached)) = next {
      if nodes.len() >= MAX_NODES {
        break;
      }
      walk(walker, &child, cache, child_cached, depth + 1, format!("{path}/{index}"), nodes);
      next = unsafe {
        if let Some(request) = cache {
          walker.GetNextSiblingElementBuildCache(&child, request).map(|sibling| (sibling, true)).or_else(|error| {
            if error.code().is_ok() {
              Err(error)
            } else {
              walker.GetNextSiblingElement(&child).map(|sibling| (sibling, false))
            }
          })
        } else {
          walker.GetNextSiblingElement(&child).map(|sibling| (sibling, false))
        }
      }
      .ok();
      index += 1;
    }
  }

  fn snapshot_cache(automation: &IUIAutomation) -> WindowsResult<IUIAutomationCacheRequest> {
    let request = unsafe { automation.CreateCacheRequest()? };
    unsafe {
      request.SetTreeScope(TreeScope_Element)?;
      for property in [
        UIA_LocalizedControlTypePropertyId,
        UIA_NamePropertyId,
        UIA_AutomationIdPropertyId,
        UIA_ClassNamePropertyId,
        UIA_HasKeyboardFocusPropertyId,
        UIA_BoundingRectanglePropertyId,
        UIA_ValueValuePropertyId,
      ] {
        request.AddProperty(property)?;
      }
      request.AddPattern(UIA_ValuePatternId)?;
    }
    Ok(request)
  }

  fn node_from_element(element: &IUIAutomationElement, cached: bool, depth: usize, path: String) -> AxNode {
    AxNode {
      depth,
      path,
      control_type: bstr_or_default(unsafe {
        if cached {
          element.CachedLocalizedControlType().or_else(|_| element.CurrentLocalizedControlType())
        } else {
          element.CurrentLocalizedControlType()
        }
      }),
      name: bstr_or_default(unsafe {
        if cached {
          element.CachedName().or_else(|_| element.CurrentName())
        } else {
          element.CurrentName()
        }
      }),
      value: value_or_none(element, cached),
      automation_id: bstr_or_default(unsafe {
        if cached {
          element.CachedAutomationId().or_else(|_| element.CurrentAutomationId())
        } else {
          element.CurrentAutomationId()
        }
      }),
      class_name: bstr_or_default(unsafe {
        if cached {
          element.CachedClassName().or_else(|_| element.CurrentClassName())
        } else {
          element.CurrentClassName()
        }
      }),
      focused: unsafe {
        if cached {
          element.CachedHasKeyboardFocus().or_else(|_| element.CurrentHasKeyboardFocus())
        } else {
          element.CurrentHasKeyboardFocus()
        }
      }
      .map(|value| value.as_bool())
      .unwrap_or(false),
      bounds: bounds_or_default(element, cached),
    }
  }

  fn bstr_or_default(result: WindowsResult<BSTR>) -> String {
    result.map(|value| value.to_string()).unwrap_or_default()
  }

  fn value_or_none(element: &IUIAutomationElement, cached: bool) -> Option<String> {
    if cached {
      if let Ok(pattern) = unsafe { element.GetCachedPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) } {
        if let Ok(value) = unsafe { pattern.CachedValue() } {
          return Some(value.to_string());
        }
      }
    }
    let pattern = unsafe { element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) }.ok()?;
    let value = unsafe { pattern.CurrentValue() }.ok()?.to_string();
    // An exposed empty value differs from an unsupported ValuePattern.
    Some(value)
  }

  fn bounds_or_default(element: &IUIAutomationElement, cached: bool) -> Rect {
    match unsafe {
      if cached {
        element.CachedBoundingRectangle().or_else(|_| element.CurrentBoundingRectangle())
      } else {
        element.CurrentBoundingRectangle()
      }
    } {
      Ok(rect) => rect_from_edges(rect.left, rect.top, rect.right, rect.bottom),
      Err(_) => Rect::default(),
    }
  }

  #[cfg(test)]
  mod cache_tests {
    use super::*;

    // Uses the owned synthetic fixture when running the Windows validation.
    // ROOT CAUSE: a cache failure must not erase otherwise readable properties,
    // and cached ValuePattern reads must retain Some("") rather than None.
    #[test]
    #[ignore = "requires the owned synthetic fixture in reset state; run alone with --ignored --test-threads=1"]
    fn cached_snapshot_and_missing_cache_fallback_preserve_fixture_nodes() {
      let windows = crate::window::list_windows().expect("list windows");
      let window = windows
        .iter()
        .find(|window| window.title.as_deref() == Some("AUV Record Editor - Synthetic Benchmark"))
        .expect("owned synthetic fixture must be running in reset state");
      let _com = init_com();
      let automation: IUIAutomation = unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }.unwrap();
      let root = unsafe { automation.ElementFromHandle(window_handle(window).unwrap()) }.unwrap();
      let walker = unsafe { automation.ControlViewWalker() }.unwrap();
      let request = snapshot_cache(&automation).unwrap();
      let cached_root = unsafe { root.BuildUpdatedCache(&request) }.unwrap();
      assert_eq!(unsafe { cached_root.CachedName() }.unwrap(), unsafe { root.CurrentName() }.unwrap());
      let mut live = Vec::new();
      walk(&walker, &root, None, false, 0, "0".into(), &mut live);
      let mut cached = Vec::new();
      walk(&walker, &cached_root, Some(&request), true, 0, "0".into(), &mut cached);
      assert_eq!(cached, live);
      // An uncached element intentionally takes every cached-property failure
      // branch, including pattern/rectangle/focus reads, and falls back live.
      assert_eq!(node_from_element(&root, true, 0, "0".into()), node_from_element(&root, false, 0, "0".into()));
      assert!(cached.iter().any(|node| node.value.as_deref() == Some("")));
    }
  }
}

#[cfg(not(target_os = "windows"))]
mod native {
  use auv_driver_common::error::{DriverError, DriverResult};
  use auv_driver_common::window::Window;

  use super::AxTreeSnapshot;

  pub(super) fn snapshot_window(_window: &Window) -> DriverResult<AxTreeSnapshot> {
    Err(DriverError::unsupported("accessibility.snapshot_window"))
  }

  pub(super) fn focus_node(_window: &Window, _child_indices: &[usize]) -> DriverResult<()> {
    Err(DriverError::unsupported("accessibility.focus_node"))
  }

  pub(super) fn select_node(_window: &Window, _child_indices: &[usize]) -> DriverResult<&'static str> {
    Err(DriverError::unsupported("accessibility.select_node"))
  }
}

#[cfg(test)]
#[path = "accessibility_test.rs"]
mod tests;
