# Scroll-search model-session benchmark

Date: 2026-10-06, America/Vancouver. Evidence level: measured model sessions
and independently verified synthetic AppKit receiver behavior.

## Result

The two matched searches that both completed successfully used **68.0% fewer
recorded input-plus-output tokens** on the optimized fork. Excluding cached
input, input-plus-output volume decreased **21.2%**. One comparison is from the
planned sample; the other is a labeled supplemental deep-search run. These
are conditional pilot results, not the complete planned six-run aggregate.

| Successful matched comparison | Baseline tokens, including cache | Optimized tokens, including cache | Reduction | Baseline uncached input + output | Optimized uncached input + output |
| --- | ---: | ---: | ---: | ---: | ---: |
| Nearby, planned trials 09/10 | 470,356 | 142,164 | 69.8% | 30,548 | 28,244 |
| Deep, supplemental trials 08/11 | 434,282 | 147,289 | 66.1% | 29,418 | 19,033 |
| Combined successful subset | 904,638 | 289,453 | 68.0% | 59,966 | 47,277 |

Both answers and final visibility were checked with fresh native screenshots.
The baseline needed 24 shell calls and five explicit window captures across
these two sessions; the optimized revision needed eight shell calls and no
extra captures. Both optimized sessions reused final search capture receipts
and selected lossless compact JSON. The baseline also performed two search
recoveries and one step-budget continuation. Raw delivery remained unverified;
successful posting alone did not qualify a trial as a success.

The original six-run sample has an unknown optimized token total: one session
timed out without completion usage. It would be incorrect to count that as zero
or report the successful subset as an overall success-normalized saving.

## Method

Compared `f87265e4` (before the four fork changes) with `7b646006` (final capture
reuse, compact JSON, foreground scroll preparation, and the focus fast path).
The baseline was built from a Git archive; immutable debug binaries were saved
before running trials. Their SHA-256 hashes are in the evidence export.

Fresh Codex CLI `0.150.1` sessions used `gpt-5.6-sol`, low reasoning, a 300-second
limit, identical prompts within each matched task, and one common executable
path switched between revisions. The agents could read the installed AUV
skill and actual command help, use foreground-preferred `input.scrollUntil`,
retain complete envelopes, recover once after ineffective input, and inspect
returned screenshot paths. They could not inspect fixture/source files or
other trials. Discovery was included rather than removed from token totals.

The fixture contains 180 fictional records drawn on a canvas, absent from AX.
Tasks were to find `Archive record 012` or `Kestrel handoff`, return the observed
code/status, and leave the record visible. The coordinator reset the viewport
through native UI, then explicitly activated the disposable ScrollCover app
and required `verified_foreground` before each controlled session. The correct
answers were withheld from measured agents. Coordinator verification was
outside their token/time measurements.

Usage comes from each CLI `turn.completed` event: `input_tokens + output_tokens`.
Cached input is a subset of input, not an additional amount. The second measure
is `input_tokens - cached_input_tokens + output_tokens`. Neither measure is a
dollar-cost or account-quota estimate; pricing weights and quota accounting are
not measured. Output reasoning tokens are not added a second time.

## Complete controlled record

| Trial | Revision | Task | Outcome | Recorded tokens | Uncached input + output | Session seconds | Shell calls |
| --- | --- | --- | --- | ---: | ---: | ---: | ---: |
| 05 | Baseline | Nearby | Not found; activation mismatch | 211,944 | 21,608 | 69.45 | 6 |
| 06 | Optimized | Nearby | Verified success | 149,490 | 19,186 | 37.15 | 4 |
| 07 | Optimized | Deep | Service timeout before AUV execution | Unknown | Unknown | 300.08 | 0 |
| 08 | Baseline | Deep | Verified success after recovery/continuation | 434,282 | 29,418 | 77.75 | 11 |
| 09 | Baseline | Nearby | Verified success after recovery | 470,356 | 30,548 | 86.91 | 13 |
| 10 | Optimized | Nearby | Verified success | 142,164 | 28,244 | 34.81 | 4 |
| 11 | Optimized, supplemental | Deep | Verified success | 147,289 | 19,033 | 40.67 | 4 |

Planned trials 05–10 succeeded in 2/3 sessions for each revision. Trial 05's
recovery activation reported `loginwindow` with
`activation_only_foreground_mismatch`; its requested record remained absent.
The next coordinator activation succeeded. Its cause is unresolved and is not
attributed to one fork change. Trial 07 logged a catalog/network timeout, made
no AUV calls, and supplied no completion usage. The timeout remains in the
planned results. Trial 11 was registered as a supplemental measurement before
the final planned pair finished; it does not silently replace trial 07.

The baseline planned total is 1,116,582 recorded tokens, including the failed
attempt. The optimized planned total and tokens per successful task are unknown.

## Setup correction and exploratory results

Trials 01–04 raised the cover's AX window without confirming application
activation. That did not guarantee the intended starting focus, so all four
were retained as exploratory runs and excluded from the controlled aggregate
before trial 05 started. The correction excludes both favorable and reversed
comparisons, not just unfavorable outcomes.

| Trial | Revision | Task | Verified | Recorded tokens |
| --- | --- | --- | --- | ---: |
| 01 | Baseline | Nearby | Yes | 299,859 |
| 02 | Optimized | Nearby | Yes | 140,023 |
| 03 | Optimized | Deep | Yes | 148,337 |
| 04 | Baseline | Deep | Yes | 136,454 |

The exploratory deep pair used fewer baseline tokens. Model choices, discovery,
batching, and initial desktop state matter; savings are not universal.

## Evidence and limits

[Metrics and binary/prompt hashes](evidence/scroll-session-benchmark/metrics.json)
retain every trial's usage, outcome, operation counts, stop reasons, and setup
classification. Example paired captures: [nearby baseline](evidence/scroll-session-benchmark/trial-09.png),
[nearby optimized](evidence/scroll-session-benchmark/trial-10.png),
[deep baseline](evidence/scroll-session-benchmark/trial-08.png), and
[deep optimized](evidence/scroll-session-benchmark/trial-11.png).
The failed trial's [final capture](evidence/scroll-session-benchmark/trial-05.png)
shows the initial viewport. These images contain fictional records only.

Full local prompts, traces, stdout/stderr, protocol amendments, binaries, and
the audit harness are in ignored `docs/notes/session-benchmark/`. Exported data
omits unrelated window-list metadata. Built-in image-view calls are not
enumerated in CLI JSONL, so their count and model-side image inspection cannot
be audited from that event stream; success labels rely on independent native
verification. Provider usage is authoritative for reported session totals;
`o200k_base` command-output counts are only a secondary text proxy.

This is a small controlled CLI pilot with debug binaries and one model setting.
It does not isolate individual commit contributions, establish release latency,
measure warm-session reuse, or demonstrate Chromium/background/MCP behavior.
Service delay is included in wall time. Much of the observed token difference
is reduced context replay from fewer model/tool rounds, including cached input.

Candidate next slice: reduce redundant discovery for known app/window targets.
Completed optimized sessions still spent three shell calls on discovery before
the search; the exploratory baseline deep session avoided window-list discovery.
This benchmark does not approve implementation of a new discovery API.
