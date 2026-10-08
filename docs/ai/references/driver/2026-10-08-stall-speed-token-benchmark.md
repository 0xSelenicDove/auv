# Stall fix: model-session speed and token benchmark

Classification: test-only benchmark and evidence documentation. Evidence level:
four fresh model sessions, each independently verified at eight exact records.
**This pilot shows a performance regression, not a speed or token win.** No
runtime or skill changes were made during measurement.

Compare pre-fix `9eefd507` with fixed `f9abd0e5`, using each version's frozen
binary and bundled skill. The binary hashes and complete prompts are in the
[registered manifest and evidence](evidence/stall-speed-token-benchmark/README.md).

## Results

Total tokens are provider input plus output, including cached input exactly once.
Wall time includes model launch, help/skill reading, daemon startup if chosen,
recovery, screenshot inspection and final answer. Reset and independent audit
are outside measurement.

| Task | Version | Verified records | Total tokens | Seconds |
| --- | --- | ---: | ---: | ---: |
| A | Before | 8/8 | 340,313 | 127.3 |
| A | Fixed | 8/8 | 389,950 | 124.1 |
| B | Fixed | 8/8 | 649,980 | 201.7 |
| B | Before | 8/8 | 188,570 | 75.6 |

Across both tasks, before used **528,883 tokens and 202.9 seconds**; fixed used
**1,039,930 tokens and 325.8 seconds**, with equal 16/16 verified records per
version. Fixed used **96.6% more total tokens** and took **60.6% longer**.
Uncached input plus output also rose from 77,811 to 95,930 (**23.3%**).

Task A was 2.5% faster with 14.6% more tokens. Task B was substantially slower
and more expensive. All four attempts completed, and no observed operation
failed or reported a capture timeout. The specific competing-client stall was
not forced in these sessions; its prevention is supported separately by the
[stall regression evidence](2026-10-07-runner-capture-stall-fix.md).

## Observed workflow differences

The subjects were allowed to choose their fastest supported workflow. Baseline
A used a private Runner with explicit Device selection. Baseline B used sequential
direct CLI calls. Both fixed attempts chose a private Runner and unqualified
invokes through `AUV_ENDPOINT`. Route and navigation choices were not held fixed.

| Task/version | Completed shell calls | Observed operation results | Window captures | Scroll searches |
| --- | ---: | ---: | ---: | ---: |
| A/before | 9 | 10 | 1 | 8 |
| A/fixed | 15 | 13 | 2 | 9 |
| B/fixed | 16 | 39 | 10 | 10 |
| B/before | 6 | 9 | 0 | 8 |

Fixed B reset to the top repeatedly, added nine manual scrolls and ten captures,
and polled artifact directories with sleeps while a command was pending. Its
later recovery revisited two already requested targets to obtain complete rows.
These operations succeeded but added work and model turns. Baseline B advanced
through the targets and used the final screenshots already returned by searches.
Most of the aggregate token increase was cached input replayed across subsequent
model turns. This is consistent with additional orchestration and verification
work; it is not a controlled attribution of cost to the Rust scheduling change.

Candidate next slice: reduce repeated resets, routine recaptures and artifact
polling by preserving the observed viewport, inspecting complete existing final
evidence and waiting for a yielded command. Validate that workflow on a fixed
route before another free-choice model pilot. This report implements no change.

## Method and limits

Same 180-row read-only AppKit canvas; two sets of eight named targets, top initial
viewport, foreground uncovered window, gpt-5.6-sol low, Codex CLI 0.150.1 and
900-second timeout. Order: before/A, fixed/A, fixed/B, before/B. Each fresh
isolated session read its complete matching skill; automatic skills, inherited
project context, plugins, network search, extra agents and other-app access were
disabled or prohibited. No existing daemon was supplied. No builds or independent
OCR audits ran concurrently with measured sessions. No replacement attempts or
pooling with previous pilots.

Each answer matched the withheld code/status oracle. All 32 original evidence
images contained the complete corresponding triple on one OCR line, ignoring
separator punctuation, and their file hashes matched original operation receipts.
All four independent final-window images contained the final target. The
coordinator visually checked both task B final states. The CLI JSON stream does
not independently expose subject image-view calls; screenshot inspection remains
a subject claim corroborated by exact answers and saved pixels.

Two tasks per version form a small synthetic pilot. Model decisions, route choice,
image sizes and macOS state confound generalization. The measurements establish
these observed session costs; a fixed-route comparison is needed to isolate
runtime latency. No native or without-skill arm was included in this rerun.

All private subject daemons exited, temporary authentication copies were removed,
and the owned fixture was closed. No permissions were changed. The installed
binary and skill stayed at the fixed version throughout.

## Evidence

[Metrics](evidence/stall-speed-token-benchmark/metrics.json),
[protocol](evidence/stall-speed-token-benchmark/protocol.md),
[manifest](evidence/stall-speed-token-benchmark/manifest.json),
[checksums](evidence/stall-speed-token-benchmark/SHA256SUMS).
Raw transcripts remain local under `docs/notes/stall-speed-token-benchmark/`.
