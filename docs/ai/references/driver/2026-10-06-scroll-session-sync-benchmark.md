# Scroll-search session benchmark after upstream sync

Date: 2026-10-06. Evidence level: six measured model sessions with independently
verified synthetic AppKit receiver state. Scope: benchmark and documentation.

## Result

Compared our pre-sync optimized build (`7b646006`) with upstream plus our
changes (`93195467`). Both completed all three tasks. The combined revision
used **21.6% fewer input-plus-output tokens including cached input**, but only
**1.1% fewer uncached input-plus-output tokens**. This small, variable pilot
does not establish a substantial additional uncached-token improvement.

| Metric, three sessions per revision | Pre-sync optimized | Combined |
| --- | ---: | ---: |
| Independently verified successes | 3/3 | 3/3 |
| Input + output, including cached input | 664,210 | 520,441 |
| Input minus cached input + output | 59,282 | 58,617 |
| Shell calls | 22 | 18 |
| Total session seconds | 186.69 | 197.70 |

All six sessions returned completion usage; no exclusions, replacements, or
missing-usage imputations were needed. Cache accounting and model discovery
varied. The combined revision was not faster in aggregate. These measurements
are neither account-quota nor monetary-cost estimates.

## Complete trial record

| Trial | Revision | Task | Tokens including cache | Uncached input + output | Seconds | Shell calls |
| --- | --- | --- | ---: | ---: | ---: | ---: |
| 12 | Pre-sync | Nearby | 151,558 | 13,062 | 26.36 | 5 |
| 13 | Combined | Nearby | 222,389 | 24,117 | 85.46 | 9 |
| 14 | Combined | Deep | 158,889 | 17,321 | 63.08 | 5 |
| 15 | Pre-sync | Deep | 266,082 | 14,050 | 118.98 | 9 |
| 16 | Pre-sync | Nearby | 246,570 | 32,170 | 41.35 | 8 |
| 17 | Combined | Nearby | 139,163 | 17,179 | 49.16 | 4 |

Every trial returned the correct observed code/status and left the requested
record visible. The nearby answer was `AR-1012`, `Stored`; the deep answer was
`VK-7392`, `Ready for review`. Fresh native coordinator screenshots independently
confirmed receiver state after every run.

Trial 15 experienced a ScreenCaptureKit 10-second timeout, with xcap fallback
also failing. The model inspected state and recovered, including an additional
capture/search. Its full usage and time remain in the totals; this does not
prove that upstream caused or fixed the timeout. Trial 16 encountered a shell
discovery error from assigning zsh's read-only `status` variable, then recovered.
That overhead also remains. Trial 13 performed more discovery calls than its
matched pre-sync session. These differing model trajectories limit attribution
of the aggregate difference to code changes.

## Method and evidence

The protocol was registered before trial 12: three counterbalanced matched
pairs, pre-sync/combined nearby, combined/pre-sync deep, pre-sync/combined nearby.
This isolates the upstream sync relative to our already optimized fork; it
does not rerun the original unoptimized baseline or combine incompatible trial
sets into a new overall saving. See the
[original benchmark](2026-10-06-scroll-session-benchmark.md) for that comparison.

Fresh ephemeral Codex CLI `0.150.1` sessions used `gpt-5.6-sol`, low reasoning,
and a 300-second limit. Each matched pair had byte-identical prompts and the
same executable path, switched between immutable binaries only after a trial
finished. Agents saw installed AUV guidance, actual command help, complete
result envelopes, and their own evidence. Discovery, recovery, images and
final answers were included in recorded session usage. Fixture/source/previous
trial results were forbidden. The coordinator reset the viewport through native
UI and required `verified_foreground` from cover activation before every run.
The model searched through `input.scrollUntil` with foreground-preferred input
and least verbose supported lossless JSON. Correct answers were withheld.

The combined binary was built from `93195467` using `cargo build -p auv-cli`;
the pre-sync immutable binary is the same one used in the earlier benchmark.
Both are debug builds. Binary hashes, prompt hashes, usage, operation counts,
setup verification, and evidence hashes are in the
[sanitized metrics export](evidence/scroll-session-sync-benchmark/metrics.json).
Unrelated desktop window metadata and raw logs remain in ignored local notes.

Usage comes from `turn.completed`: input plus output, with cached input a
subset of input. Reasoning output is not counted twice. CLI JSONL does not
enumerate built-in image-view calls, so image-call counts remain unknown.
The command-output tokenizer count in the export is a secondary text proxy,
not the total session usage.

Returned final evidence is preserved byte-for-byte:
[12](evidence/scroll-session-sync-benchmark/trial-12.png),
[13](evidence/scroll-session-sync-benchmark/trial-13.webp),
[14](evidence/scroll-session-sync-benchmark/trial-14.webp),
[15](evidence/scroll-session-sync-benchmark/trial-15.png),
[16](evidence/scroll-session-sync-benchmark/trial-16.png),
[17](evidence/scroll-session-sync-benchmark/trial-17.webp).
The combined sessions successfully used the upstream WebP final-evidence path.
Image encoding/resolution differences do not independently establish token
savings because the sessions also used different discovery and recovery paths.

Validation: all six completion records, usage arithmetic, identical matched
prompts, verified setup receipts, artifact hashes, and `git diff --check`.
No production code changed. A useful next measurement would hold discovery
steps fixed to separate capture changes from agent variation; it remains a
follow-up experiment rather than an implementation request.
