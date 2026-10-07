# Scroll-until stage profile

Classification: test-only and docs. Evidence level: four independently verified
local synthetic AppKit searches. Production behavior and the installed skill
are unchanged. OCR accounts for most of the slow-run time and most of the
observed timing variation; this identifies a candidate optimization boundary,
not an implemented speed improvement or token reduction.

| Run | Build | Total seconds | Input | Settle | Capture | OCR | Loop remainder |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | Debug | 44.061 | 0.011 | 6.704 | 5.581 | 29.621 | 2.144 |
| 2 | Release | 41.239 | 0.015 | 6.698 | 5.162 | 29.291 | 0.074 |
| 3 | Release | 16.981 | 0.009 | 6.695 | 5.538 | 4.660 | 0.079 |
| 4 | Debug | 19.379 | 0.014 | 6.692 | 5.464 | 5.065 | 2.143 |

Each search delivered 22 steps of 420 logical pixels, with 300 ms settling,
and made 23 captures and 23 recognition calls. All returned the same text match,
delivery result and final visibility of `VK-7392 / Ready for review`.
Capture used `macos.screencapturekit.ffi` without fallback, at 1800×1364 pixels.
All delivery results reported no focus disturbance. Test windows were closed
after verification, and enumeration confirmed zero remaining fixture windows.

The [registered protocol](evidence/scroll-stage-profile/protocol.md) used
debug/release/release/debug order with a fresh top viewport for each run.
The [ignored integration test](../../../../crates/auv-scan/tests/scroll_stage_profile.rs)
wraps the existing `WindowScrollUntilSurface` IO boundary and invokes the public
`scroll_until` loop. It records each call's elapsed time without changing policy,
waits, captures, OCR settings or navigation. The remainder is total loop time
minus input, wait, capture and OCR durations. It includes pixel crop/copy and
comparison, text matching, observer bookkeeping and capture drops; it is not a
pure pixel-processing timer. Setup, fixture reset, final PNG encoding, persistence
and independent screenshot verification are outside the measured loop. The test
does not launch or activate apps and is ignored during ordinary test runs.

For a manually started fixture at the top, run:

```sh
AUV_SCROLL_PROFILE_OUTPUT=/absolute/task-output/run.json \
  cargo test -p auv-scan --test scroll_stage_profile -- --ignored --nocapture
```

The output directory must exist. Add `--release` before the package arguments to
profile optimized Rust. The test writes metrics and a final PNG, asserts the
expected search result and step count, and restricts its target to the named
synthetic fixture. PNGs contain fictional benchmark records only.

## Interpretation and next slice

Capture totals and settling remained nearly constant. Release reduced loop
overhead by about two seconds, but the OCR boundary varied by about 25 seconds
in both builds. This narrows the variance seen in the
[build-mode pilot](2026-10-06-scroll-speed-build-mode.md) to recognition rather
than input delivery or pixel comparison. The wrapper does not separate Rust
image copying, Swift image preparation, Vision inference and language processing
inside that boundary, so it cannot attribute the variation to a particular
internal stage or to host scheduling.

The current `VisionApi::recognize_text_in_capture_with_options` always supplies
a crop, including a full-window region. Native `find_ocr_text_rgba` enlarges that
crop by 2× in each dimension, then runs accurate recognition with language
correction and the default Chinese/English language set. Consequently this
Retina capture is normally analyzed at 3600×2728 pixels. These are source-backed
facts; the profile has not measured the individual costs of those choices.

Candidate next slice: compare current OCR with full-resolution OCR that avoids
the additional enlargement for full Retina captures. Measure recognition time,
matched text and coordinate accuracy on small text and mixed-language fixtures
before adopting a change. Preserve subregion behavior and independent final
screenshot verification. Do not assume the English canvas validates a global
language restriction or a lower-accuracy recognition mode. This candidate is
not implemented in the profiling slice. The subsequent
[resolution experiment](2026-10-06-retina-ocr-resolution.md) rejected that
candidate after mixed small text failed recognition. Its per-call analysis also
locates most timing variation in the first recognition call.

[Per-call timings, results, executable hashes and checksums](evidence/scroll-stage-profile/metrics.json)
retain every outcome. The measured base revision was `a98dbe14` plus the test-only
profiler. No model session ran; token usage and end-to-end session speed were
not measured. Validation: four opt-in live passes, `cargo test -p auv-scan`
(24 unit tests, two doctests; profiler ignored normally), `cargo fmt --check`,
`cargo check`, `cargo test` (default targets), `git diff --check`, and
`cargo run --quiet -- invoke --help`. Existing rustfmt configuration and CLI
unused-import warnings remain outside this slice.
