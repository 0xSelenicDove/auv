# macOS foreground scroll preparation

## Reproduced bug

The canvas benchmark's no-effect foreground scroll was reproduced on the fork.
With `local.auv.CanvasFixture` inactive at the top, `input.scrollUntil` at
window-local `(450,400)`, `dy=400`, `foreground-preferred`, and a 150 ms settle
reported successful `foreground_system_events` delivery, 800 pixels delivered,
and `end_by_no_visual_progress` after two steps. The independent native
accessibility scrollbar remained zero. The final image showed archive row 001.

Explicit `app.activate` followed by the same instant scroll moved the scrollbar
to `0.0364963503649635`. Activation alone, without timed delivery, was enough in
this reproduction. This explains one foreground path failure; it does not
establish the cause of every prior benchmark stall or of Chromium background
scroll failures.

## Root cause and fix

`WindowApi::scroll_impl` selected global HID for foreground delivery but did
not prepare its target window. Global input is hit-tested against the current
desktop; accepting the post cannot prove that the intended window consumed it.
Clicks and drags already used exact-window preparation.

The foreground scroll candidate now uses that same lifecycle: validate/activate
and confirm focus for the exact target, deliver the input, then restore the
preparation lease. Preparation errors stop before posting. The existing native
focus confirmation waits for its predicate, so there is no additional fixed
settle delay for preparation. Delivery continues to report `verified: false`;
focus disturbance is now accurately `foreground`.

Background-only never enters the foreground branch. Its delivery strategy and
absence of activation are retained. Background-preferred gains preparation
only if it reaches its existing foreground fallback; no visual-stall-triggered
fallback or automatic foreground retry has been added.

## Regression and evidence

`foreground_scroll_prepares_window_and_moves_receiver` is an opt-in live test.
It needs the canvas fixture reset to top, a disposable cover application with
bundle ID `local.auv.ScrollCover`, and existing macOS Accessibility permission.
The cover is a copy of the same synthetic fixture, with that bundle ID and name;
it overlaps the target rather than interacting with user data.

The test activates the cover, reads the target scrollbar through AX, performs
one foreground scroll, and asserts that the receiving scrollbar moved. It then
activates the cover again, performs background-only scroll, and asserts that
foreground ownership remains with the cover and focus disturbance remains none.
It failed on the original production code at the receiver-motion assertion and
passed after the fix, including with preparation's settle set to zero.

```sh
cargo test -p auv-driver-macos foreground_scroll_prepares_window_and_moves_receiver -- --ignored --nocapture
```

Further live validation: a 250 ms timed scroll of 400 pixels moved the scrollbar
from zero to `0.0364963503649635` and returned exactly 400 delivered pixels with
raw verification false. CLI wall time was 1.899 s, including startup, recording,
preparation, and the 150 ms post-input settle. This is a single diagnostic
measurement, not a performance distribution or a 250 ms timing guarantee.

A live CLI search starting from the covered target found `Archive record 012 |
AR-1012 | Stored` after one step. An independent native screenshot showed row
012 and AX reported scrollbar `0.0364963503649635`.

Final captured evidence: [before](evidence/foreground-scroll/before.png) and
[after](evidence/foreground-scroll/after.png). Both contain fictional fixture
records only. Local full logs are in ignored `docs/notes/scroll-diagnosis/`.
No new permission or persistent application setting was granted.

## Limits and follow-up

The shared motion/stream scheduler invokes scroll for each sample. Recipient
preparation therefore repeats and costs time. Amortizing preparation requires a
bounded lifecycle that still handles focus changes; it is marked at the call
site as follow-up rather than skipping target validation using cached state.

This change removes the need for explicit activation in this receiver case. It
is not an end-to-end token benchmark. Measure full sessions, including retries
and verification, before claiming a total token reduction. The standard macOS
suite keeps live tests ignored; the receiver regression was also run explicitly.

Checks: `cargo test -p auv-driver-macos` (112 passed, 8 ignored across unit and
integration suites), `cargo test -p auv-scan --lib` (24 passed),
`cargo test -p auv-cli-invoke --lib` (102 passed, 1 ignored), built CLI search and
timed-scroll probes, explicit live receiver regression, formatting, and diff
checks. No Swift or Rust/Swift FFI declarations were changed.
