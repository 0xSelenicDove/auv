# Scroll Search Evidence

Date: 2026-10-06

## Scope

First contribution from the computer-control benchmark follow-up: preserve
local `input.scrollUntil`'s final screenshot and clarify no-progress diagnosis.
This is an approved feature slice. It does not change scroll delivery policy,
pixel thresholds, stop conditions, or the Runner protocol.

## Behavior

Local invoke retains the capture from the final observation and publishes it
through the existing tracing PNG writer with purpose
`auv.scan.scroll_until_final_capture`. The invoke result carries the artifact
receipt, including the file path when the frontend store supplies one. Pixels
are not added to JSON. The capture is moved rather than cloned; no extra capture
or OCR call is made. Only the latest observation is retained.

Recording availability is checked before entering the blocking input pool,
because tracing context is thread-local. Without recording, observation captures
remain opted out. Dry runs produce no screenshot. Artifact publication follows
the existing best-effort invoke policy: a storage failure does not turn completed
scroll delivery into a failure.

Both local and Runner invoke human reports label a no-progress stop's boundary
as unconfirmed. An unchanged viewport can result from a boundary, ineffective
input, occlusion, or delayed content. Delivery evidence is not semantic success.
The existing `end_by_no_visual_progress` machine-readable reason is unchanged.
See the [scroll delta contract](2026-10-06-scroll-delta-contract.md) for upstream
Chromium occlusion evidence; it is not a diagnosis of every stalled scroll.

## Validation

Hermetic regression coverage in `input_test.rs` checks final screenshot pixels,
artifact receipts, capture count, absence of OCR for an end search, dry-run
behavior, and the unconfirmed-boundary report. Existing scroll-loop tests cover
initial matches, later matches, budget exhaustion, no-progress stops, and
observer cancellation. These checks do not establish live driver reliability.

Commands run:

- `cargo test -p auv-cli-invoke --lib`: 100 passed, 1 existing live-app test ignored.
- `cargo test -p auv-scan --lib`: 24 passed.
- `cargo fmt --all --check`: passed (existing rustfmt configuration warnings).
- `git diff --check`: passed.

The fresh checkout required `git submodule update --init --recursive` and CMake
for the macOS media build. CMake was installed through Homebrew; no project
dependency was added.

## Next Slices

- Runner final screenshots need capture references rather than transferring every
  observation's pixels to the invoke client. The current protocol cannot provide
  a final artifact reference; this remains explicitly deferred at the call site.
- Lossless `--compact-json` now removes formatting whitespace; see
  [compact JSON measurements](../invoke-cli/2026-10-06-compact-json.md).
  Command-specific field projection remains deferred.
- The foreground no-effect case was reproduced and fixed using exact-window
  preparation; see [receiver evidence](2026-10-06-foreground-scroll-preparation.md).
  Other stall causes and background-delivery reliability remain separate.
- Benchmark reusable workflows with first-run discovery costs separated from
  repeated execution and failed runs.
