# Windows center-point scroll at 200% scaling

Classification: reproduced bug fix in `auv-driver-windows`.

The Windows benchmark at fork `4cd053e94047f2378aeb02bede5f772f558f4da4`
reported successful wheel delivery without viewport motion. Both default posted
wheel delivery and foreground SendInput reproduced this using `(0.5, 0.5)` in a
180-row Windows Forms canvas. The DWM frame was 1804×1364 physical pixels while
GetWindowRect/PrintWindow produced a 916×697 capture in the unaware CLI thread.
The same physical point was being passed to virtualized child hit testing or
virtual desktop normalization. A smaller explicit point happened to work.

The driver now enters per-monitor-v2 DPI awareness around native enumeration,
posted mouse operations, cursor reads and pointer normalization. PrintWindow
capture follows the target window's awareness for both bitmap sizing and render,
because an unaware app renders at its own logical extent even in an aware caller.
A non-Send/non-Sync guard restores the old calling-thread context on return,
error and unwind. No process-wide DPI setting or host configuration is changed.
The guard must stay inside synchronous native calls, rather than crossing await
or worker-thread boundaries.

On the same 200% display, both center-point wheel paths now move the canvas and
find Amber transfer. A physical-size bitmap alone produced 1832×1378 pixels with
the app in one quarter and black padding elsewhere, causing false no-motion
stops. That intermediate B follow-up returned only Amber and remains retained.
Sizing the bitmap in the target's rendering context removes that padding.
Exact capture border-to-DWM coordinate mapping remains the existing deferred
precision slice. Delivery remains best-effort and is not semantic verification.

A subsequent center-point probe also reproduced a sparse-row failure without
padding: different record labels/codes below the mean pixel threshold were
misclassified as no progress. `auv-scan` now resets the still-view streak when
already-required OCR content changes. This adds no capture or recognition call.
Pixel-only end searches retain their existing behavior; the explicit step budget
bounds OCR instability. Two fake-surface regressions cover reaching a later
target despite pixel-still evidence and stopping at unchanged bottom content.

Validation: release CLI and embedded Windows helper build; a nested-context
error-path test; a live unaware-window capture test from an aware host;
125 driver tests pass with the three live clipboard tests
excluded; 28 scan tests pass; package formatting and diff whitespace checks pass.
The initial full driver suite
had two clipboard failures, including failure restoring an oversized clipboard
test payload. Those failures are unrelated to DPI handling and are retained in
the local evidence rather than represented as a passing full suite.

Native benchmark B separately failed because the selected model was at capacity
after 503 retries, with no final usage event. DPI changes cannot repair provider
capacity. Follow-up B runs are separate experiments; original attempts and their
metrics must remain preserved. The native relay additionally publishes responses
by atomic rename to avoid reading a partially written JSON response, observed
once in native A. That harness correction is separate from the driver patch.

API reference: [SetThreadDpiAwarenessContext](https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-setthreaddpiawarenesscontext).
