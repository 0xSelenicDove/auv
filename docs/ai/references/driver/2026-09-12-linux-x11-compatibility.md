# Legacy Linux X11 compatibility driver

Date: 2026-09-12. Classification: owner-approved feature.

## Implemented boundary

[`auv-driver-linux-x11`](../../../../crates/auv-driver-linux-x11/README.md)
is an explicitly selected Rust driver, descriptor `linux.x11`. Continued use
is not recommended; use the existing Wayland driver for new deployments.
The owner approved Enigo 0.6.1 (`default-features = false`, `x11rb`) for input
and the existing xcap 0.6.2 for capture.

The crate reuses `auv-driver-common` display, capture, input options, and
`InputActionResult`. Foreground dispatch returns `verified = false`; semantic
verification belongs to the caller. It implements monitor listing/capture,
contained region capture, pointer movement/click/drag/wheel, scoped shortcuts,
and Unicode text. Parameter validation happens before input mutation, and
scoped key/button releases are attempted after errors without replaying input.

`DISPLAY` must identify an existing authenticated X11 server. Reject Wayland
process environments and changing DISPLAY/XAUTHORITY; xcap caches its connection
process-wide. This restriction also applies to other xcap users in the process.
Coordinates are root pixels with scale 1, independent of Xft font DPI.

## Validation evidence

### 2026-10-05 OSWorld input-seam completion

Evidence level: **live-validated in isolated Xvfb**, plus native Linux compile;
this is not an OSWorld end-to-end result.

The X11 backend now implements the existing shared held-key controller and
logical-mouse coordinator. The root facade and daemon Runner therefore use the
same bounded down/up, sampled movement, sampled drag, cancellation, timeout,
and release-cleanup contracts as the other Linux backend. A new additive
`ScrollScreenPoint` Runner RPC and registered `input.scrollPoint` invoke command
expose foreground wheel delivery to paired clients.

The updated Tk receiver observed left-button down/up and Shift key down/up while
the driver also completed sampled move/drag and logical mouse lifecycle calls.
On Docker/OrbStack Debian bookworm arm64 with Rust 1.91, Xvfb 800x600x24, and
Tk 8.6, the ignored live regression passed: `1 passed; 0 failed`. Native Linux
`cargo check -p auv-driver-linux-x11 -p auv-driver --tests` also passed.

`buf lint`, generated-code regeneration, and
`buf breaking proto --against '.git#branch=main,subdir=proto'` passed. The
schema change is additive. This closes the missing OSWorld action primitives;
cursor compositing, an OSWorld action adapter, benchmark VM reset/setup, and
semantic task evaluation remain separate work.

The follow-up Kubernetes live validation is recorded in
[`../ops/2026-10-05-osworld-kubernetes-x11-evidence.md`](../ops/2026-10-05-osworld-kubernetes-x11-evidence.md).
It exercises paired HTTP control and co-located Unix/X11 socket sharing against
an independent receiver. That run found and fixed fractional sampled-drag
points being rejected by the integral XTEST coordinate boundary; both
topologies then completed sampled drags with receiver-observed endpoints.

### 2026-10-04 facade and Runner integration

Evidence level: **live-validated for the named environment**, not a general
Linux support claim.

The integrated `auv-driver::LocalDriver` path was built and exercised on an
Ubuntu 24.04 x86_64 Kubernetes Pod on `neko-gpu-1`. The desktop was XFCE on
Xorg `:99`, 1920x1080, using the Xorg dummy display driver and llvmpipe. HAMi
DRA allocated an RTX 4080 SUPER to the Pod, but Xorg did not render on that GPU;
this evidence does not establish GPU-accelerated Xorg or physical-seat behavior.

Results:

| Check | Result |
| --- | --- |
| Linux `cargo check -p auv-driver -p auv-cli` | Passed; includes the daemon LocalDriver Runner |
| Linux `cargo test -p auv-driver-linux-x11 -p auv-driver` | 18 passed; live test ignored by default |
| Linux `cargo test -p auv-cli --lib` | 39 passed |
| Root facade live regression on Xorg `:99` | `LocalDriver` selected `linux.x11`; `display.list` returned `DUMMY0`, 1920x1080 |
| `auv invoke display.list --json` through the integrated local facade | Completed; returned `DUMMY0`, 1920x1080 |
| X11 crate Clippy, all targets, `-D warnings` | Passed |
| Root driver Clippy, all targets, no dependencies, `-D warnings` | Passed |
| macOS driver checks/tests; `cargo fmt --check`; `git diff --check` | Passed |

The root-facade regression is
[`crates/auv-driver/tests/linux_x11.rs`](../../../../crates/auv-driver/tests/linux_x11.rs).
It is ignored by default because xcap pins the process display and the test
requires a dedicated X11 server. The named-environment live commands were:

```bash
env -u WAYLAND_DISPLAY DISPLAY=:99 XDG_SESSION_TYPE=x11 \
  pixi exec --spec rust=1.91.1 -- \
  cargo test -p auv-driver --test linux_x11 -- --ignored --nocapture

env -u WAYLAND_DISPLAY DISPLAY=:99 XDG_SESSION_TYPE=x11 \
  pixi exec --spec rust=1.91.1 -- \
  cargo run -p auv-cli --quiet -- invoke display.list --json
```

The regression receipt was `1 passed; 0 failed`. It asserted that
`LocalDriver::descriptor()` and the opened `LocalDriverSession` both reported
`linux.x11`, that the session variant was `LinuxX11`, and that facade-level
`display.list` returned at least one display. The invoke receipt completed with
one primary display named `DUMMY0`, frame `1920x1080`, scale factor `1.0`.
These receipts identify the selector and display capability exercised; they do
not extend the evidence beyond the named dummy-Xorg environment.

Full dependency Clippy remains blocked by a pre-existing
`auv-driver-linux/src/atspi.rs:119` `needless_borrow` warning. This integration
does not change that unrelated source.

The independent Tk receiver validation in the same named environment exercised
full/region capture, Unicode keyboard input, named keys and shortcuts, mouse
buttons, both wheel axes, drag receipt, and pointer position. The desktop and
AUV ran as root without Xauthority inside the isolated Pod, so this is not
evidence for a normal authenticated user login.

### 2026-09-12 initial crate evidence

The initial crate-only validation used an isolated workspace containing exact
copies of the new crate and `auv-driver-common`, with dependencies from the
repository lockfile. The later integration evidence above supersedes its
architecture-level build boundary.

Environment: Docker/OrbStack, `rust:1.95-bookworm` arm64, Debian 12,
Xvfb 21.1.7 (800x600x24), Python 3/Tk 8.6.13. Installed development packages:

```bash
apt-get update
apt-get install -y --no-install-recommends \
  pkg-config libxcb1-dev libxrandr-dev libxkbcommon-dev libwayland-dev \
  libegl1-mesa-dev libgbm-dev libdbus-1-dev libpipewire-0.3-dev libclang-dev \
  xvfb xauth x11-utils x11-xserver-utils python3-tk
```

xcap's Linux dependency graph also compiles Wayland/EGL/GBM/PipeWire libraries;
these are not this driver's capture transport. Missing Wayland and EGL development
packages were found during the clean build and added to the setup documentation.

Results:

| Check | Result |
| --- | --- |
| Linux `cargo test -p auv-driver-linux-x11` | 10 passed; live test ignored by default |
| Linux crate Clippy, all targets, `-D warnings` | Passed |
| Xvfb `--test xvfb -- --ignored --nocapture` | 1 passed, 0 failed |
| macOS crate tests | 2 passed, including unsupported session opening |
| macOS crate Clippy, all targets, `-D warnings` | Passed |
| Root `cargo check --offline`, `cargo test --offline` | Passed (default workspace members) |
| Root `cargo run --offline --quiet -- invoke --help` | Passed |
| `cargo fmt --check`, `git diff --check` | Passed |

Live probe command (use Docker `--init` for xvfb-run readiness signals):

```bash
env -u WAYLAND_DISPLAY XDG_SESSION_TYPE=x11 \
  xvfb-run -a -s '-screen 0 800x600x24' \
  cargo test -p auv-driver-linux-x11 --test xvfb -- --ignored --nocapture
```

The [test](../../../../crates/auv-driver-linux-x11/tests/xvfb.rs) opens a real
Tk application and checks red captured pixels, crop coordinates, invalid crops,
application-owned text `AUV Ω`, blue pixels after submission, right-click and
both wheel axes, and final pointer position after dragging. It disposes of its
application and temporary files; xvfb-run disposes of the server.

Two fixture details matter: Tk's default Ctrl+A moves to the start of a line,
so the fixture explicitly binds select-all. The driver documents that
`replace_existing` requires this application convention. Tk 8.6 also maps
horizontal wheel button 7 to Shift+button 5; the assertion checks the toolkit's
observed event, following [Tk source](https://github.com/tcltk/tk/blob/core-8-6-13/generic/tkEvent.c#L1135-L1140).

## Local driver integration and deliberate limits

The initial test was Xvfb application evidence, and the later live test used a
dummy Xorg server in an isolated Pod. Neither is a physical-seat support claim.
Physical-seat Xorg, GPU-accelerated Xorg, XWayland, multiple X screens, non-US
layouts, and OSWorld remain unvalidated. Rootless XWayland cannot be assumed to
expose native Wayland surfaces or a full desktop screenshot.

Window/AT-SPI APIs, background input, clipboard ownership, cursor compositing,
and overlays are
intentionally omitted from the X11 backend. The root facade can apply its
existing capture-driven Tesseract OCR to an X11 display capture; this does not
add X11 window targeting. Ctrl+A replacement is app-dependent; drag is a direct
move only when callers choose the `drag` convenience; `drag_mouse` supports
sampled trajectories. Input errors can occur after partial delivery.

`auv-driver::LocalDriver` selects this backend only when `WAYLAND_DISPLAY` is
missing or empty and `DISPLAY` is nonempty. A nonempty `WAYLAND_DISPLAY` keeps
the existing Wayland backend, including XWayland sessions where both variables
are present. With neither variable present, the facade preserves the Wayland
default so existing diagnostics continue to report the missing compositor.

The facade exposes display capture and foreground input through the existing
shared contracts. X11 window discovery/capture and clipboard paste remain
explicitly unsupported; the daemon services return their normal unimplemented
status for those calls. This integration does not claim that Electron can now
run OSWorld.
