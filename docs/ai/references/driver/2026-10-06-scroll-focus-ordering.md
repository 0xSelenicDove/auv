# Foreground scroll readiness after focus changes

Classification: bug fix. Evidence level: source-backed correction and local
live AppKit receiver tests on macOS; broader application behavior is unmeasured.

The focus-switch failure recorded in
[wheel event location evidence](2026-10-06-scroll-event-location.md) was
reproduced before this change. A diagnostic read of WindowServer ordering showed
the cover ahead of the target while AX reported the target focused. The cover
consumed 300 of the requested 400 px; the remaining 100 reached the target.
Posting and AX focus were insufficient readiness evidence.

`input_target_is_focused` now requires both current AX app/window focus and
WindowServer ordering. It considers the target itself and visible, nontransparent
regular windows, ignoring other floating overlays. This also permits a floating
target. The existing one-second `confirm_input_focus` wait uses that same
predicate. Missing ordering evidence takes the existing preparation/error path.
There is no new delay, retry, activation backend, or verification claim.

The regression independently asserts WindowServer order before each delivered
sample, deliberately transfers focus to the cover after the first consumed
sample, and verifies that later input moves only the target. Three repeated
corrected runs passed, followed by one final run after including floating targets
in the ordering check. The inactive-target/background-only and twelve-sample
header-pointer regressions also passed. Background-only delivery remains
separate and never enters this foreground preparation path.

This is readiness for activation of a window, not an atomic recipient reservation
or proof that higher-level overlays cannot intercept input. Changes after the
predicate and post remain outside this slice. A stricter point-specific occlusion
contract needs an owner-approved consumer and separate receiver evidence.

Validation passed: `scripts/generate-swift-bridge`, macOS driver
`swift build --build-system native` (the existing generated-header workaround),
`cargo fmt --check`, `cargo check`, `cargo test` (default targets: 109 passed,
one ignored), `cargo test -p auv-driver-macos --lib` (111 passed, eight ignored),
and `cargo run --quiet -- invoke --help`. Live tests were run separately with
`--ignored --nocapture`; generated files are not committed.

The disposable local fixtures now open without automatic activation. They are
closed outside measured sessions. The user approved brief live checks and then
explicitly approved the full three-method model-session benchmark. Its protocol,
metrics, retained setup corrections and conclusions are recorded separately in
[the accompanying benchmark report](2026-10-06-scroll-routing-benchmark.md); receiver-test success alone is not token
savings evidence.
