# Model-session benchmark for filtered window discovery

Evidence level: six measured model sessions with independently verified
synthetic AppKit receiver state. Scope: benchmark/docs only, 2026-10-06.

## Result

Compared `25d8f5fa` before discovery filters with `fc9b64c1` after them.
All six searches succeeded, with complete usage. The filtered revision used
**13.0% fewer uncached input-plus-output tokens**, but **8.7% more recorded
input-plus-output tokens including cached input**. The small pilot is mixed;
it does not establish a reduction in every token metric or monetary cost.

| Metric, three sessions per revision | Before filters | After filters |
| --- | ---: | ---: |
| Independently verified successes | 3/3 | 3/3 |
| Input + output, including cached input | 549,840 | 597,945 |
| Input minus cached input + output | 57,168 | 49,721 |
| Shell calls | 15 | 20 |
| Total session seconds | 168.96 | 132.29 |
| Windows returned per discovery | 22, 21, 21 | 1, 1, 1 |

All three after-filter agents naturally selected app/title filtering after
reading command help. They retained complete matching records. The before
agents all listed windows too, sometimes batched with other commands, so shell
call counts are not operation counts. The larger cached total and differing
discovery paths show why response-size savings cannot be treated as a fixed
full-session saving.

## All registered trials

| Trial | Revision | Task | Tokens including cache | Uncached input + output | Seconds | Shell calls |
| --- | --- | --- | ---: | ---: | ---: | ---: |
| 18 | Before | Nearby | 139,757 | 16,749 | 59.25 | 4 |
| 19 | After | Nearby | 242,135 | 20,311 | 52.06 | 10 |
| 20 | After | Deep | 221,994 | 20,650 | 56.98 | 6 |
| 21 | Before | Deep | 291,744 | 23,840 | 80.12 | 8 |
| 22 | Before | Nearby | 118,339 | 16,579 | 29.59 | 3 |
| 23 | After | Nearby | 133,816 | 8,760 | 23.25 | 4 |

No results were excluded or replaced. Trial 19 read more help than its matched
before trial. Both deep trials reached their first 20-step budget and continued
for two more steps; their entire usage remains included. Trial 21 also encountered
zsh's read-only `status` variable during shell bookkeeping, then continued and
activated the app. These trajectories limit attributing the aggregate token
difference exclusively to filtered response size. The first nearby pair used
more uncached tokens after filtering; the other two pairs used fewer.

## Method

The protocol was registered before trial 18: B/F nearby, F/B deep, B/F nearby.
Matched prompts were byte-identical and unchanged from the earlier benchmark,
so filter adoption was discovered through actual help, not forced on one arm.
Fresh ephemeral Codex CLI `0.150.1` sessions used `gpt-5.6-sol`, low reasoning,
a 300-second limit, and the same runtime executable path. Immutable debug
binaries differed only in the discovery-filter patch. The before binary was
built with the three touched Rust files restored to `25d8f5fa`; the checkout
and normal binary were then restored to `fc9b64c1` before trials began.

For every trial the coordinator reset the fixture through native UI and required
`verified_foreground` activation of the disposable cover. Models used only AUV
core operations on the synthetic canvas fixture, foreground-preferred bounded
scroll searches, complete JSON envelopes, and screenshot evidence. Fixture
source, other trials and oracle answers were unavailable to measured models.
Discovery, continuation, reasoning, final answers and images were included in
actual completion usage. A fresh native coordinator screenshot independently
confirmed the final visible code/status after each run: `AR-1012`, `Stored` for
nearby, and `VK-7392`, `Ready for review` for deep.

Metrics use `turn.completed`: input + output, with cached input a subset of
input. The uncached metric subtracts cached input; reasoning output is not
counted twice. CLI JSONL does not enumerate built-in image-view calls, so their
counts remain unknown. Text tokenizer proxies in the export do not replace
actual session usage. These are not billing, account-quota, or production
latency estimates. Backend enumeration is unchanged.

## Evidence and validation

[Sanitized metrics](evidence/scroll-session-filter-benchmark/metrics.json)
include binary hashes, prompt hashes, setup verification, operation counts,
returned-window counts, usage and evidence hashes. Unrelated desktop metadata
and raw logs remain in ignored local notes. Final artifacts are preserved
byte-for-byte: [18](evidence/scroll-session-filter-benchmark/trial-18.webp),
[19](evidence/scroll-session-filter-benchmark/trial-19.webp),
[20](evidence/scroll-session-filter-benchmark/trial-20.webp),
[21](evidence/scroll-session-filter-benchmark/trial-21.webp),
[22](evidence/scroll-session-filter-benchmark/trial-22.webp),
[23](evidence/scroll-session-filter-benchmark/trial-23.webp).

Validated all six completion records, usage arithmetic, matching prompt hashes,
verified setup, receiver state and copied artifact hashes, plus
`git diff --check`. Builds succeeded and the current checkout was restored.
No production changes were made by the benchmark.

See [filter behavior and response-size measurement](../invoke-cli/2026-10-06-window-list-filters.md)
and the [upstream-sync benchmark](2026-10-06-scroll-session-sync-benchmark.md).
Further work should measure reuse of help and already grounded targets;
this pilot does not authorize new caching or session APIs.
