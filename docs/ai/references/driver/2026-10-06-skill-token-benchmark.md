# AUV skill startup and total-token benchmark

Evidence level: four actual model sessions with independent native receiver
verification, 2026-10-06. Scope: installed skill/docs change; the AUV binary and
production execution code are unchanged.

## Change and result

The installed `auv-computer-control` entrypoint now contains 570 words rather
than 1,240, a 54.0% reduction including frontmatter. Detailed field-editing and
popup instructions moved into its optional operation reference. The short
entrypoint keeps target/coordinate rules, verification, permission boundaries,
complete failure evidence and bounded recovery.

A supplied executable is used directly. Version and doctor checks are
conditional on compatibility/readiness needs or relevant failures. Help is
restricted to needed operations and can be batched. Known app/title targets
can go straight to capture; missing or ambiguous selectors use filtered
discovery where supported. Text-search navigation points to bounded
`input.scrollUntil`, with screenshot verification. Existing `--compact-json`
is preferred when installed help supports it; it preserves the complete
result rather than dropping diagnostics. No output wrapper or new runtime API
was added.

In the matched two-task pilot, the revised skill used **29.8% fewer total
input-plus-output tokens including cached input**. Both versions completed
both tasks. Uncached input-plus-output increased **1.4%**; this does not
establish lower monetary cost or account-quota consumption.

| Metric, two sessions per skill | Before | After |
| --- | ---: | ---: |
| Verified successes | 2/2 | 2/2 |
| Tokens including cached input | 476,794 | 334,476 |
| Uncached input + output | 47,098 | 47,756 |
| Shell calls | 24 | 16 |
| Shell calls containing help | 10 | 7 |
| Shell calls containing version/doctor probes | 4 | 0 |
| Shell calls using compact JSON | 0 | 7 |

The savings coincided with fewer model/tool round trips, no routine startup
probes, and compact invoke responses. This small experiment cannot separate
the contribution of shorter guidance from changes in the selected workflow.

## All registered trials

| Trial | Skill | Task | Total tokens including cache | Uncached input + output | Seconds | Shell calls |
| --- | --- | --- | ---: | ---: | ---: | ---: |
| 01 | Before | Nearby | 230,442 | 15,914 | 77.29 | 12 |
| 02 | After | Nearby | 146,274 | 22,754 | 51.94 | 9 |
| 03 | After | Deep | 188,202 | 25,002 | 87.37 | 7 |
| 04 | Before | Deep | 246,352 | 31,184 | 73.35 | 12 |

Both paired tasks used fewer total tokens after the change. The nearby after
run used more uncached tokens, while the deep after run used fewer. All four
subjects returned the correct code/status and left the requested record
visible. The nearby before run used a single manual scroll; the other three
used `scrollUntil`. The deep after run directly targeted the supplied app/title
without `window.list`; both after runs skipped version/doctor. Independent
help reads were not consistently batched, so that guidance remains a preference,
not a measured guarantee.

## Method and limits

The [protocol](evidence/skill-token-benchmark/protocol.md) was recorded before
trial 01: before/after nearby, then after/before deep. All outcomes are retained.
Fresh ephemeral Codex CLI `0.150.1` sessions used `gpt-5.6-sol`, low reasoning
and a 600-second deadline. The same immutable `fc9b64c1` AUV binary included
the upstream sync and fork changes in both arms. Goal and evidence instructions
were unchanged from the corrected three-arm benchmark; only trial-specific
paths differed between paired prompts. Each model explicitly read its full
frozen skill file; complete read output was checked against that snapshot.

Plugins, skill search, automatic skill instructions and project docs were
disabled. Each trial used a fresh local `CODEX_HOME`; inherited `CODEX_*`
app/thread environment was removed. No other skill reads or MCP calls appeared.
The coordinator reset the synthetic canvas fixture to the top and verified
foreground activation of the inert cover before each run, then independently
checked final native screenshots. These setup/verification actions are outside
subject usage. Model discovery, skill loading, errors, navigation, images and
answers are inside actual provider `turn.completed` usage.

Input includes cached input; uncached input + output subtracts that cached
subset. Reasoning output is already included in output and is not added again.
CLI JSONL does not enumerate built-in image-view calls, so their count remains
unknown. All AUV images use the same normal capture representation. Differing
scroll policies and settling choices mean elapsed seconds do not isolate
backend performance. Two tasks per version do not establish general savings,
platform support, or reliability beyond these verified receiver runs.

The revised total of 334,476 is 19.7% below the earlier AUV-without-skill total
of 416,504, but 9.2% above the earlier pure-computer-use total of 306,175.
Those are contextual comparisons to a separate experiment, not new matched
controls. This pass demonstrates improvement over the old skill; it does not
establish a general advantage over either other mode. See the
[three-arm report](2026-10-06-three-arm-session-benchmark.md).

## Evidence and validation

[Metrics](evidence/skill-token-benchmark/metrics.json) retain all four provider
records, binary/skill/reference/prompt hashes, full-read checks, setup receipts,
independent outcomes, operation counts and byte-for-byte artifact hashes.
Frozen [before](evidence/skill-token-benchmark/before/SKILL.md) and
[after](evidence/skill-token-benchmark/after/SKILL.md) skill snapshots include
their references so the changed instructions remain reviewable in the fork.
The after snapshot matches the installed skill. Original reference inspection
links remain pinned to their historical revision; current installed help takes
precedence.

Final artifacts:
[01](evidence/skill-token-benchmark/trial-01.webp),
[02](evidence/skill-token-benchmark/trial-02.webp),
[03](evidence/skill-token-benchmark/trial-03.webp),
[04](evidence/skill-token-benchmark/trial-04.webp).

Validated skill frontmatter with `quick_validate.py`, full skill reads,
isolation checks, actual usage arithmetic, correct receiver state, verified
setup, frozen/copied hashes, removal of temporary auth copies and
`git diff --check`. Raw logs and unrelated desktop metadata stay in ignored
local notes. No Rust code changed, so Cargo checks were not rerun.
