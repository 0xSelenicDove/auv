# Compact command help with full guidance retained

Evidence level: regression-tested help output across every registered command,
three measured before/after help pages, and four actual model sessions with
independent native receiver checks, 2026-10-06. Classification: narrow refactor
of CLI presentation. The change is retained for its smaller contract-preserving
help layout. The full-session savings criterion was not met because a control
failed; the observed session totals are reported separately.

## Change and direct measurement

`auv-cli-invoke::render_command_help` previously always rendered Clap's expanded
layout. It now uses Clap's compact layout while promoting long about/help text
and examples into the compact fields. Long metadata is cleared on this temporary
presentation command, eliminating the misleading hint to request another help
view. Existing argument parsing, registry metadata, targets, defaults, possible
values, input delivery, JSON results and the installed skill are unchanged.
Both `-h` and `--help` continue to use this shared renderer. The catalog is not
shortened in this slice.

The renderer relies on Clap rather than postprocessing generated help with
whitespace/line heuristics. Arguments and descriptions share a row when Clap
can fit them; longer options can remain on separate lines. Defaults and possible
values no longer require their own expanded blocks. No new flags, dependencies
or execution abstractions were added.

A test was added first and failed against the old renderer. After the change,
it verifies every registered command has fewer help lines and exactly the same
sequence of contract words as expanded help, excluding only Clap's generated
short/long-help hint. Existing registry tests also retain inline examples. This
protects platform limitations and activation/verification boundaries along with
the ordinary syntax and defaults.

| Help page | Before bytes | After bytes | Before lines | After lines |
| --- | ---: | ---: | ---: | ---: |
| `app.activate` | 947 | 906 | 34 | 17 |
| `window.capture` | 1,370 | 1,324 | 40 | 21 |
| `input.scrollUntil` | 3,648 | 3,414 | 99 | 59 |

Combined: **5,965 → 5,644 bytes (5.4% smaller)** and **173 → 97 lines**.
These are output-size measurements, not tokenizer counts or billing savings.
All operation guidance words remain; the generated hint accounts for the word
count differences. Frozen help output and the source patch are stored as JSON strings to preserve
their exact whitespace without introducing trailing whitespace in evidence files.

## Model-session pilot

| Metric, two sessions per binary | Expanded help | Compact help |
| --- | ---: | ---: |
| Verified successes | 1/2 | 2/2 |
| Input + output including cached input | 406,322 | 349,275 |
| Uncached input + output | 85,298 | 37,979 |
| Shell calls | 19 | 19 |
| Help-containing shell calls | 8 | 7 |

Observed totals were **14.0% lower**, and uncached input + output **55.5% lower**,
with compact help. Both compact-help tasks succeeded, while the nearby control
failed. The baseline search reported no visual progress after two steps; one
Page Down recovery and a capture left the fixture at the top. It correctly
reported `found=false`. That failed session's help and recovery remain counted.
The compact nearby run itself required reactivation and a third capture.

The deep pair both succeeded, but compact help used **more** total tokens there
(203,195 versus 179,861). Thus the pilot does not show a consistent improvement
across tasks or isolate a causal effect of help layout. Different discovery,
input/focus strategies and cache patterns affect the whole-session totals. The
change is kept for the deterministic smaller help output and passing contract
checks, not a proven general 14% reduction. The registered full-session criterion
and its failed status remain visible in metrics; no trial was replaced.

| Trial | Help | Task | Success | Total tokens | Uncached + output | Seconds |
| --- | --- | --- | --- | ---: | ---: | ---: |
| 01 | before | Nearby | No | 226,461 | 42,909 | 63.20 |
| 02 | after | Nearby | Yes | 146,080 | 13,472 | 42.75 |
| 03 | after | Deep | Yes | 203,195 | 24,507 | 66.60 |
| 04 | before | Deep | Yes | 179,861 | 42,389 | 90.78 |

## Protocol, validation and limits

The [protocol](evidence/help-layout-benchmark/protocol.md) was registered before
trial 01: before/after nearby, then after/before deep. Both binaries were built
in the same debug profile from `14a3b9ea`; only the after binary includes the
[help-layout patch](evidence/help-layout-benchmark/help-layout.patch.json). Binary
hashes are recorded, with an immutable executable copied into each trial. Skill
snapshots are identical, matching the installed 618-word guidance. Paired prompts
match after normalizing trial paths; no additional strategy hint was introduced.
Earlier pilots are not pooled or used as these runs' controls.

Subjects used Codex CLI `0.150.1`, `gpt-5.6-sol`, low reasoning and a 600-second
deadline. Fresh trial homes contained the frozen skill and a local existing auth
copy deleted afterward. Automatic skill instructions, project documents, plugins
and skill search were disabled, inherited `CODEX_*` context stripped. Full skill
reads match frozen files; no other skill reads or MCP calls occurred. All four
provider `turn.completed` usage records are complete.

The synthetic canvas began at the top under an inert cover, whose
`verified_foreground` setup receipt was checked before each subject. Subjects
were limited to the fixture, finite navigation and one corrective recovery,
complete result envelopes and fresh screenshot verification. The native
coordinator independently verified the final receiver for every trial: three
correct visible records, one unchanged top viewport. Nearby oracle was
`AR-1012 / Stored`; deep was `VK-7392 / Ready for review`, withheld from subjects.
Coordinator setup/inspection is outside subject tokens; skill loading, help,
errors, images, input, recovery and final answers are included.

Input includes cached input. The secondary metric subtracts its cached subset;
reasoning output is already included in output. Built-in image-call counts remain
unknown in JSONL. Receipts are deduplicated by run ID/command ID. Final artifacts,
including the failed trial, are copied byte-for-byte and hashed. Raw logs and
unrelated desktop metadata remain in ignored local notes. Four sessions, one
failed control and varying cache/recovery patterns limit generalization. This
is not a billing, quota, latency or new platform-support claim.

Validation passed: `cargo fmt --check`, `cargo check`, `cargo test`,
`cargo test -p auv-cli-invoke`, `cargo build -p auv-cli --bin auv`,
`cargo run --quiet -- invoke --help`, and `git diff --check`. The focused crate
had 120 passing tests and one ignored; configured default test targets had 109
passing and one ignored. Also checked frozen binary/skill/help/artifact hashes,
all usage arithmetic, normalized prompts, setup/outcomes and auth cleanup.

[Metrics](evidence/help-layout-benchmark/metrics.json) retain all sessions, direct
help sizes and the distinction between keeping the layout and failing the
full-session improvement criterion. Frozen skill [before](evidence/help-layout-benchmark/before/SKILL.md)
and [after](evidence/help-layout-benchmark/after/SKILL.md) are identical. Final images:
[01](evidence/help-layout-benchmark/trial-01.webp),
[02](evidence/help-layout-benchmark/trial-02.webp),
[03](evidence/help-layout-benchmark/trial-03.webp),
[04](evidence/help-layout-benchmark/trial-04.webp).

Candidate next slice: isolate the recurring foreground/no-motion failure before
changing recovery policy. It matters because a failed navigation path can dwarf
the help-size saving. No driver or recovery behavior was changed in this slice.
