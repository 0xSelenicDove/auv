# Foreground scroll focus fast path

## Scope and contract

This narrow optimization follows the [foreground preparation fix](2026-10-06-foreground-scroll-preparation.md).
Each macOS foreground wheel sample still validates Accessibility permission,
process liveness, and WindowServer window ownership. A fresh AX read confirms
the frontmost application and exact focused window. If they already match,
scroll skips System Events activation and AX raise. Otherwise, the existing
full preparation path validates AX window availability, activates, raises,
and confirms focus before delivery.

There is no cached focus state or new motion lifecycle. The same branch serves
instant, timed, and streaming foreground scroll, including the existing
background-preferred foreground fallback. Background-only never enters it.
Other input preparation paths are unchanged. Raw delivery remains unverified.

The fast path validates WindowServer identity without enumerating all AX
windows: the exact focused AX window supplies focus support evidence. An
intermediate implementation enumerated AX windows on every sample; rapid
CLI trials exposed transient lookup failures. Those failed trials are excluded
from the latency comparison. Missing focused AX state uses full preparation,
and preparation errors remain errors rather than authorizing input.

## Live receiver checks

`timed_foreground_scroll_rechecks_focus_between_samples` uses the synthetic
CanvasFixture and ScrollCover. After the first sample is consumed, its callback
activates and confirms the cover. Later samples must restore exact target focus,
move the target scrollbar, leave the cover scrollbar unchanged, and report the
400-pixel delivered total with `verified: false`.

Waiting for first-sample consumption matters: global HID posting is asynchronous.
Changing focus while that event is queued can reroute it. The test checks a
focus change between consumed samples; it does not establish atomic protection
against arbitrary concurrent desktop changes. The shared scheduler's existing
desktop-admission deferral remains in `scroll_motion.rs`.

The existing inactive-window receiver regression also passes, including
background-only foreground preservation. Its cover assertions now confirm
fresh AX focus rather than relying on NSWorkspace's worker-thread snapshot.

```sh
cargo test -p auv-driver-macos timed_foreground_scroll_rechecks_focus_between_samples -- --ignored --nocapture
cargo test -p auv-driver-macos foreground_scroll_prepares_window_and_moves_receiver -- --ignored --nocapture
```

## Limited CLI latency probe

Same debug CLI, synthetic fixture, three sequential invocations per revision:
`input.scroll 450 400 --dy 400 --duration-ms 250 --settle-ms 150`, target
`app:local.auv.CanvasFixture`, foreground-preferred, compact JSON. The fixture
was reset before each series. All final comparison commands completed and
reported 400 delivered pixels; independent AX showed target movement.

| Revision | CLI wall time, seconds |
| --- | --- |
| Before | 0.766, 0.666, 0.658 |
| After | 1.848, 0.448, 0.465 |

Median decreased from 0.666 to 0.465 seconds (30%). The first after-run was
slower; two warm runs show the improvement, not a guaranteed latency bound.
This tiny debug-build probe includes CLI startup, recording, and settle. It is
not a release performance distribution, a duration guarantee, or an agent
session token benchmark. Local stdout/stderr and timing records are ignored
under `docs/notes/scroll-diagnosis/`.

## Validation

Generated Rust/Swift bindings with `scripts/generate-swift-bridge`; Cargo build,
macOS driver tests, shared driver tests, both explicit live receiver tests,
formatting and diff checks passed. SwiftPM passed with
`swift build --build-system native`. The default SwiftPM build system resolved
the relative generated-header path against the parent `native/` directory and
failed; a fresh scratch directory reproduced it. Native build-system selection
is a validation workaround, not a manifest or generated-file change.
