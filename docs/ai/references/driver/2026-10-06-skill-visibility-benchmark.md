# Known covered-app activation skill pilot

Evidence level: four measured model sessions with independent synthetic native
receiver verification on 2026-10-06. Classification: docs/skill-only. No Rust,
CLI, driver or selector behavior changed.

The installed skill now says to activate a known covered app once before
observation when the task permits foreground activation. It preserves
background-only/no-focus restrictions, supported-help discovery, permission
boundaries and independent outcome verification. The entrypoint grows from
570 to 618 words; operation references are unchanged.

## Result and decision

| Metric, two sessions per variant | Before | Candidate |
| --- | ---: | ---: |
| Verified successes | 2/2 | 2/2 |
| Input + output including cached input | 321,012 | 312,056 |
| Uncached input + output | 27,124 | 49,016 |
| Shell calls | 14 | 13 |
| Shell calls containing help | 6 | 4 |
| Seconds | 100.42 | 115.71 |
| Failed capture/list observations | 0 | 0 |
| Activation before observation | 0/2 | 2/2 |

The candidate used **2.8% fewer total tokens**, **80.7% more uncached tokens**,
and took longer. It meets the successful-completion and total-token criterion
for retaining the candidate locally. This is a small pilot result, not evidence
of lower billing, faster execution or general token savings.

Crucially, neither baseline reproduced the visible-window failure seen in the
[previous experiment](2026-10-06-skill-reuse-benchmark.md). Both captured the
allowed fixture successfully despite verified foreground cover setup. Thus this
pilot establishes instruction adoption but **does not establish avoided lookup
failures or recovery savings**. No `window.list` calls occurred in either arm.
The candidate added one activation per task. Both variants captured twice and
searched once per task. Candidate nearby also batched three help reads into one
call, while the other runs did not; batching was already in both skill variants.
Its narrower startup help path and batching confound attribution to activation.

| Trial | Variant | Task | Total tokens | Uncached + output | Seconds | Shell calls |
| --- | --- | --- | ---: | ---: | ---: | ---: |
| 01 | Before | Nearby | 168,514 | 14,018 | 50.21 | 7 |
| 02 | Candidate | Nearby | 148,570 | 35,930 | 53.53 | 5 |
| 03 | Candidate | Deep | 163,486 | 13,086 | 62.18 | 8 |
| 04 | Before | Deep | 152,498 | 13,106 | 50.20 | 7 |

Nearby improved, deep worsened. All registered results were retained; none was
replaced or excluded. The candidate is the installed variant, with its frozen
snapshot committed for review. The rejected final-frame/help-batching expansion
from the previous experiment was not reinstated.

## Protocol and validation

The [protocol](evidence/skill-visibility-benchmark/protocol.md) was recorded
before trial 01: before/candidate nearby, then candidate/before deep. Each fresh
`gpt-5.6-sol` low-reasoning session had a 600-second deadline and used Codex CLI
`0.150.1` and the same immutable `fc9b64c1` AUV binary. Paired prompts are identical
after normalizing trial paths. No new activation hint was added to common prompts.
The baseline exactly matches the previously installed shorter skill.

The synthetic canvas app starts at the top, with an inert cover activated and
its `verified_foreground` receipt checked before each session. Subjects may
control only the fixture. They must use bounded navigation, one corrective
recovery after ineffective input and fresh screenshot verification. The native
coordinator independently inspected each final receiver screenshot: nearby
`AR-1012 / Stored`, deep `VK-7392 / Ready for review`, both left visible. Coordinator
setup and verification are outside subject token totals.

Fresh trial-specific `CODEX_HOME` directories contain the frozen variant and a
local auth copy deleted after completion. Automatic skill instructions, project
documents, plugins and skill search are disabled; inherited `CODEX_*` context is
removed. Full successful skill reads match frozen files. No other skills or MCP
calls occurred. All four provider `turn.completed` usage records are complete.
Subject skill reads, help, observations, errors and final answers are included.

Input includes cached input; the secondary metric subtracts that subset.
Reasoning output is already part of output and is not added twice. Built-in
image-view counts remain unknown because JSONL does not expose them. Operation
receipts are deduplicated by run ID and command ID before counting. Complete
result envelopes are retained locally. Final artifacts are copied byte-for-byte
and hashed. Raw logs and unrelated desktop metadata stay in ignored local notes.

Validated normalized prompts, isolation, full skill reads, usage arithmetic,
setup receipts, outcomes, snapshot/artifact/binary hashes, auth cleanup, installed
candidate identity, skill validation and `git diff --check`. No Rust changed;
Cargo checks were not needed for this skill/docs-only slice. Two sessions per
variant and different cache/strategy choices limit causal conclusions. Earlier
benchmarks are not pooled or used as controls here. Foreground restrictions are
preserved in guidance; this pilot does not test background-only behavior.

## Evidence and next candidate

[Metrics](evidence/skill-visibility-benchmark/metrics.json), frozen
[before](evidence/skill-visibility-benchmark/before/SKILL.md) and
[candidate](evidence/skill-visibility-benchmark/after/SKILL.md), and final images
[01](evidence/skill-visibility-benchmark/trial-01.webp),
[02](evidence/skill-visibility-benchmark/trial-02.webp),
[03](evidence/skill-visibility-benchmark/trial-03.webp),
[04](evidence/skill-visibility-benchmark/trial-04.webp).

Candidate next slice: make the covered-window failure reproducible in a
controlled fixture before tuning recovery further. It matters because these
sessions did not exercise the original wasted-lookup path. No fixture or
production changes were made in this slice.
