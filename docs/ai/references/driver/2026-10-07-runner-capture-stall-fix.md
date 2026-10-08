# macOS Runner capture stall fix

Classification: bug fix. Investigation began 2026-10-07; final validation completed 2026-10-08.
Evidence level: reproduced native/CLI failure, focused regression tests, and live synthetic receiver validation on this Mac. This is not an end-to-end model token benchmark.

## Reproduction and cause boundary

An isolated Runner completed 32 background searches and 16 foreground searches.
Two live AUV capture processes reproduced the failure: a second capture client
could make both processes wait for ScreenCaptureKit and then fail. An idle first
Runner was sufficient; simultaneous screenshots or input were not required.
The peer capture failed after 40.139 seconds in the idle-owner reproduction.
In the reset-to-top eight-target sequence, seven searches completed and the eighth
failed after 42.651 seconds when a second client was introduced.

Owned-process samples showed replayd XPC connection recovery loops during
shareable-content lookup. The xcap fallback also waited in ScreenCaptureKit's
window-image path, so it did not escape the failing service. These observations
identify the conflicting AUV client boundary; they do not establish Apple's
internal cause. A [related Apple Developer Forums report](https://developer.apple.com/forums/thread/772365)
describes multi-application shareable-content failures, but is not independent
proof that it is the identical OS bug. Local permission logs showed granted
screen capture for the failing processes; no permission reset was performed.

The Runner also performed capture and OCR synchronously on its current-thread
Tokio runtime, preventing unrelated RPCs from progressing during native waits.
A test-only real OCR reproduction failed before moving native work off that
thread and passed afterward. Scheduling alone kept window.list responsive
(about 53 ms during a native stall), but did not fix the competing-client failure.

## Fix

- The macOS driver retains one advisory capture ownership lock for the process
  lifetime. The first capture/probe claims it; subsequent calls in that process
  reuse it. Process exit releases the lock. The shared inode is never unlinked.
- A competing AUV process fails before native capture or xcap fallback starts,
  with a message directing the caller to reuse the owning Runner or wait for its
  exit. This is an operational ownership error, not a missing permission.
- Unqualified live CLI invokes honor an explicitly configured `AUV_ENDPOINT`,
  using the existing Device/Run selection path. Explicit selectors still apply;
  unselected hermetic scan commands remain local.
- Capture, OCR, find-text and native permission probe RPCs use the existing Tokio
  blocking pool. The Runner event thread remains available for other requests.
  Capture refs, OCR caching, recorded artifacts and pixel ownership are unchanged.
- The bundled and installed computer-control skill explain endpoint reuse and
  the ownership error. No new dependency, pixel roundtrip or native API was added.

A direct-image ScreenCaptureKit experiment still reproduced the failure and was
reverted. The existing native screenshot callback deadline remains in place.

## Validation

The corrected release build completed all eight reset-to-top searches while
20 concurrent capture calls reused the same Runner. At the eighth search, a
separate daemon attempted capture and was rejected in **0.0837 seconds**.
All 20 reused captures used `macos.screencapturekit.ffi`, with no fallback;
they took 0.207–0.563 seconds each. No search hit the native capture timeout.
The eight searches took 126.731 seconds in total; the last took 27.414 seconds.

The earlier failing run completed seven searches and failed the eighth after
42.651 seconds. Its normal searches were faster than this final probe. These
small sequential runs establish failure prevention, not an overall throughput
improvement or a statistically controlled speed comparison.

Each final recorded WebP was independently OCR-audited for the complete expected
label, code and status on one row. OCR separator punctuation was ignored.
All eight images were 900×682 with recorded source size 1800×1364 and scale 2;
file hashes matched their original artifact receipts. No recapture was used.

Results, binary hashes, saved synthetic images and validation receipts are in
[`evidence/runner-capture-stall-fix/`](evidence/runner-capture-stall-fix/).

Final workspace validation: **1,166 passed, 0 failed, 20 ignored** across 140 test
targets. Formatting, workspace check, release build, CLI help, native SwiftPM
build and both skill validations passed. The tested release binary was installed
in `~/.local/bin/auv`, which is ahead of Homebrew in the existing PATH; `auv
--version` now reports 0.0.31. Homebrew's package remains separately installed.

## Limits

The lock coordinates participating AUV processes using the same per-user temporary
directory. It does not coordinate unrelated screenshot applications, older AUV builds without
the ownership guard, or AUV
processes configured with different temporary directories. It deliberately
prevents a second independent AUV capture Runner while the owner lives; callers
must reuse the owner or let it exit. This conservative admission rule can be
reconsidered when concurrent-client native probes pass reliably. It prevents the
reproduced conflict rather than claiming to repair ScreenCaptureKit globally.

No total-token improvement is claimed from these native/CLI checks.
