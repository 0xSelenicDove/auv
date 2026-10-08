# Post-sync focus reproduction and fixed-operation comparison

Classification: test-only investigation and documentation. Evidence level:
local direct-CLI probes against two frozen release binaries, with retained
failures and an exact-row audit. Production code and the skill are unchanged.

**The foreground-focus failure reproduces before and after the merge.** These
probes do not establish a newly introduced focus regression. Fixed-operation
timings show substantial variance in both versions; they do not explain the
full model-session token and time spike by themselves.

## Focus reproduction

The target was the existing `local.auv.RepeatedSearchFixture` window, selected
by app and title. `input.clickPoint 90 57 --relative-to window` with
`--input-policy foreground-preferred` failed before delivery with
`macos native confirm_input_focus failed` in both binaries. The old binary
took 1.469 seconds and the merged binary 1.296 seconds. Explicit CUA title-bar
activation followed by fresh AX observation did not change the outcome:
old 1.316 seconds, merged 1.313 seconds. An old `app.activate` call reported
`verified_foreground`; that app-level result did not satisfy the input
driver's exact-window readiness check. A merged background-only click reported
window-targeted delivery in 0.196 seconds; this is delivery evidence, not
verification of the button's semantic effect.

`Window.swift`'s `input_target_is_focused` and `confirm_input_focus` functions
are unchanged between the pre-sync fork and the merge. The predicate requires
a running process, AX frontmost state, the exact AX focused-window ID, and the
target's position in the visible CG window ordering. These probes establish
that this boundary rejects the fixture; they do not isolate which condition
fails. The listing's `is_main: false` is not such evidence: the Rust mapping
defaults missing main-window metadata to false. Do not relax the focus guard
based on this reproduction.

## Identical search operations

Each attempt started with the exposed Reset to top button, verified at scrollbar
value zero through CUA outside the timer. Both binaries invoked:

```sh
auv invoke input.scrollUntil 450 500 --dy 400 \
  --until 'text:Birch transfer' --settle-ms 40 --max-steps 100 \
  --confirmations 2 --input-policy background-only --no-overlay \
  --target app:local.auv.RepeatedSearchFixture \
  --title 'AUV Repeated Search - Synthetic Benchmark' \
  --store-root <attempt-specific-directory> --compact-json
```

Wall time includes CLI launch, execution, recording and exit. All completed
searches stopped after one delivered step and returned
`Birch transfer | BK-1600 | Stored`. A subsequent image audit found the exact
complete triple in all four retained 900×682 artifacts. The auditor also uses
Apple Vision, so it is independent of the command result but not an independent
recognition engine.

| Attempt | Binary | Seconds | Outcome |
| --- | --- | ---: | --- |
| 1 | Old | 27.021 | Exact row audited |
| 2 | Merged | 29.950 | Exact row audited |
| 3 | Merged | 1.537 | Exact row audited |
| 4 | Old | 60-second limit | Timed out; capture probe overlapped |
| 5 | Old | 1.079 | Exact row audited; no overlap |

Attempt 4 remains counted and is explicitly confounded: capture probe 1
accidentally ran concurrently during its latter part. No final artifact or
completed run record was produced before the subprocess timeout. Attempt 5
is an additional disclosed probe, not a replacement used to hide the timeout.

The clean fast observations differ by 0.457 seconds (42.4% relative to the
old observation), but there is only one fast observation per binary. The
earlier slow observations differ by 2.930 seconds (10.8%). Neither pair is a
reliable speed estimate. This agrees with the earlier
[stage profile](2026-10-06-scroll-stage-profile.md), which measured large
recognition-time variation, but no per-stage profiler ran in this comparison.

## Capture-only probes

These used the same app/title, a unique store per call and `window.capture`.
Capture probe 1 overlapped search attempt 4; subsequent capture probes ran
sequentially, after that search had exited.

| Probe | Binary | Seconds | Outcome |
| --- | --- | ---: | --- |
| 1 | Old | 40.094 | Native capture timeout; xcap fallback failed; overlap |
| 2 | Merged | 0.613 | Native capture, 1800×1364, scale 2 |
| 3 | Merged | 0.558 | Native capture, 1800×1364, scale 2 |
| 4 | Old | 10.492 | Native timeout; xcap fallback succeeded at 1800×1364 |
| 5 | Old | 0.736 | Native capture, 900×682, scale 1 |

Successful normal captures do not show a capture-only slowdown here. They also
confirm the underlying resolution difference, while all persisted images remain
900×682. Native capture timeouts/fallbacks occur in the old binary too. These
measurements do not prove that resolution causes OCR cost or that the merged
binary never suffers capture timeouts.

## Boundaries and next slice

The old release binary is frozen from `0ba6bac1`; the pre-sync fork tip
`6d796567` only added documentation. The merged release binary is frozen from
`50aa4972`; the current `4948a8fc` also only adds documentation. Executable
hashes, arguments, complete JSON responses, stderr, images, audit and retained
timeout metadata are in the
[evidence pack](evidence/post-sync-isolation/protocol.json), with
[checksums](evidence/post-sync-isolation/SHA256SUMS). Original local artifact
paths in stdout identify the recording source; copied evidence filenames in
the pack are the durable files. The scratch helper does not catch its
60-second timeout, so that outcome is recorded separately in measurements.

No model sessions ran and no provider token totals were measured. Consequently
this is a runtime investigation, not evidence of improved token usage. It does
not replace the negative
[full-session benchmark](2026-10-08-post-sync-benchmark.md).

Candidate next slice: isolate the failing exact-window focus condition in a
test-only reproduction using the existing native boundary, then fix only a
proven predicate bug. Separately, profile repeated recognition at the current
capture resolution before changing OCR policy. The earlier
[resolution experiment](2026-10-06-retina-ocr-resolution.md) lost mixed small
text, so a speed change must preserve that coverage. Neither candidate is
implemented here.

Validation: retained live probes and post-measurement image audit; evidence
checksums and `git diff --check`. No Rust or Swift source changed.
