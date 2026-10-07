# Foreground wheel event location

Classification: bug fix. Evidence level: source-backed fix and local live
AppKit receiver regression on macOS, not a cross-application support claim.

The approved slice investigates scroll-search stalls before changing the input
path. The synthetic CanvasFixture starts at the top with the pointer over its
header, outside the scroll view. `foreground_scroll_from_header_moves_each_sample`
posts twelve 400 px foreground-preferred scrolls at window point (450, 400), with
150 ms settle, and independently checks that its AX scrollbar advances after
every sample. Raw delivery still reports `verified: false`.

## Correction

`scroll_point` warped the cursor and posted a mouse-move event, but constructed
the wheel event without assigning its location. A warp alone is insufficient to
establish the target for an asynchronously posted wheel event. The correction
sets `scrollEvent.location = location`, matching the explicit location already
used in window-targeted wheel delivery. It adds no delay, retry, activation,
backend, or semantic-success claim. Cursor restoration code is unchanged.

The shared foreground wheel function serves instant, timed and streaming input.
Background-only delivery stays on its separate window-targeted path.

## Local reproduction results

Base: `21d4c085`. The test was added before production changes. Each movement
trial starts with a CUA reset and a confirmed scrollbar value of zero. These
are receiver tests, not model-session token or latency benchmarks.

| Configuration | Outcome |
| --- | --- |
| Original wheel code, initial run | Failed on sample 0: posting succeeded, scrollbar remained 0 |
| Explicit location, initial run | All 12 samples advanced; final scrollbar 0.4379562043795621 |
| Explicit location, repeat | All 12 samples advanced |
| Original wheel code restored as control | Failed again on sample 0 with scrollbar 0 |
| Explicit location restored, final run | All 12 samples advanced |

The movement test passed three complete runs with the correction and failed
both runs without it. This demonstrates the header-pointer failure on this
receiver. It does not establish a token reduction or explain every earlier
model-session stall.

The existing `foreground_scroll_prepares_window_and_moves_receiver` live test
passed with the correction: an inactive target moved and background-only input
left the cover focused. Its first invocation lacked the cover fixture and
failed during setup; the cover was launched and the test rerun.

## Remaining failures and limits

Update: the focus-switch mismatch below was subsequently reproduced and
corrected in [foreground scroll focus ordering](2026-10-06-scroll-focus-ordering.md).
The following records the results at the event-location change, not a current
claim that its separate ordering failure remains unfixed.

`timed_foreground_scroll_rechecks_focus_between_samples` failed with both the
correction and the original wheel code. In each run the cover consumed 375 px
after the first consumed target sample and a deliberate focus switch. Cover
scrollbar values were 0 -> 0.03421532846715328 with the correction and
0.03421532846715328 -> 0.06843065693430657 with original wheel code. Fresh AX
focus checks alone did not prevent this receiver-routing failure. Location
stamping does not pin an event recipient across focus changes. Investigation
of that existing failure is a candidate next slice; no speculative delays or
recipient-routing changes are included here.

Two exploratory runs added a pointer-position assertion and failed that extra
assertion after receiver movement. One reported expected (290, 152), observed
(299, 184) through the existing `NSEvent.mouseLocation` query. The assertion was
removed because this slice had not established the setup/query coordinate
invariant. Pointer restoration is not claimed as independently validated;
the original restoration mechanism remains in place. Resolving the query/setup
discrepancy is another candidate investigation, not evidence of a new cause.

An initial test invocation used `--exact` without the module prefix and selected
zero tests. It is not counted among the receiver runs.

## Validation

Live regression commands:

```sh
cargo test -p auv-driver-macos --lib foreground_scroll_from_header_moves_each_sample -- --ignored --nocapture
cargo test -p auv-driver-macos --lib foreground_scroll_prepares_window_and_moves_receiver -- --ignored --nocapture
cargo test -p auv-driver-macos --lib timed_foreground_scroll_rechecks_focus_between_samples -- --ignored --nocapture
```

The first two passed with the correction; the third retains the baseline failure
described above. Native validation regenerates Swift bindings with
`scripts/generate-swift-bridge` and uses `swift build --build-system native`
in the macOS driver's Swift package, following the existing generated-header
build-system workaround. Generated files are not committed.

Other checks passed: `cargo fmt --check`, `cargo check`, `cargo test`
(default targets: 109 passed, one ignored), `cargo test -p auv-driver-macos --lib`
(111 passed, eight ignored), `git diff --check`, and
`cargo run --quiet -- invoke --help`. Ignored live tests are accounted for
separately above; passing the default suite does not erase their failures.
