# Capture timeout investigation and callback lifecycle fix

Classification: bug fix and a narrow skill correction. Evidence level: native
callback contract tests, complete workspace tests and a synthetic macOS live
probe. This fixes late callback handling; the underlying intermittent macOS
capture stall is **not established as resolved**.

The failed model sessions contained ScreenCaptureKit timeouts, failed xcap
fallbacks and occasional -3801 errors. Unified-log correlation for a failed
pre-fix process shows Screen Capture permission checks returning authValue 2.
Permission denial is therefore not a sufficient explanation for every timeout.
The sessions also issued overlapping input/capture work while shell commands
were still running. That is an observed workflow problem, not proof that
concurrency causes the OS stall.

## Fix

The old synchronous Swift bridge waited ten seconds on a semaphore, but its
lookup callback remained live after that wait expired. A late callback would
still start `SCScreenshotManager.captureSampleBuffer`, overlapping fallback or
recovery after the caller had already received a timeout. Mutable completion
values were owned by the waiting function rather than a terminal request state.

`WindowCaptureOperation` now owns the callback result behind a lock and one
monotonic deadline. Expired lookup callbacks cannot start screenshot work.
Expired or duplicate completions cannot replace a terminal result. A valid
completion preserves the exact image and captured frame. Timeout messages now
identify shareable-content lookup versus screenshot delivery and retain native
error detail instead of treating every timeout as a permission problem.
ScreenCaptureKit's screenshot method provides no cancellation handle; an OS
request already started may finish after expiry, but its result is discarded.
See [Apple's capture API](https://developer.apple.com/documentation/screencapturekit/scscreenshotmanager/capturesamplebuffer(contentfilter:configuration:completionhandler:)).

The bundled and installed skill also tells callers to wait for a yielded shell
session before issuing more input or recovery to the same window. Independent
help reads and unrelated work remain possible. Its model-session effect has not
been measured. No permission or host configuration changes were made.

## Validation and limits

- Before the production fix, an isolated native check completed 32 sequential
  captures, then 64 captures across two threads. The intermittent benchmark
  stall was not reproduced by those probes.
- The callback contract check covers expired lookup, completion arriving after
  the deadline but before wait returns, screenshot-stage timeout, duplicate
  delivery, and preservation of the image/frame. The late-completion case failed
  against the initial deadline implementation and passed after the expiry guard.
  This is a regression for the new lifecycle contract, not a reproduction of
  the OS stall.
- The initial direct native test omitted window-resolution setup and hit a
  WindowServer initialization assertion. It was corrected to match first-party
  callers before collecting the successful capture results; that harness error
  is not reported as the original timeout.
- After the fix, 64 concurrent native captures completed. Eight sequential
  searches through a private Runner completed with background-only input,
  no overlay and no explicit window raise. All eight retained 900×682 final
  images had matching digests and complete oracle rows. The native stress probe
  overlapped the Runner probe; this is behavior evidence, not a clean timing
  comparison. The eight Runner calls took 41.5 seconds, including a 16-second
  final call; long-call behavior still needs investigation.
- Rust workspace: **1,163 passed, 0 failed, 20 ignored**, 140 targets.
  `cargo check`, format and diff checks passed. The native SwiftPM build passed
  with `--build-system native`. The default build backend resolves the existing
  relative bridging header from a different directory, and this Command Line
  Tools installation lacks XCTest. Contract tests therefore reuse the existing
  Rust-driven standalone `swiftc` fixture pattern rather than adding a test
  package or changing the native package layout. Generated bridge files remain
  ignored. Both skill validators passed and installed guidance matches the repo.

The test window and private daemon were closed. Homebrew was not changed.
A full model-token benchmark has not been rerun. The next candidate slice is
stage-specific reproduction of the macOS stall with one live route and no
concurrent input/recovery; adding persistent capture streams or a broad retry
policy is outside this fix.

[Manifest](evidence/capture-deadline-fix/manifest.json),
[live results](evidence/capture-deadline-fix/summary.json),
[image audit](evidence/capture-deadline-fix/image-checks.json),
[permission correlation](evidence/capture-deadline-fix/permission-correlation.json),
[checksums](evidence/capture-deadline-fix/SHA256SUMS).
