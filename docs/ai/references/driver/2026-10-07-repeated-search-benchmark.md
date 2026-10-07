# Repeated visual record search: three-method benchmark

Classification: test-only benchmark and documentation. Evidence level: six
registered model sessions on one synthetic macOS AppKit fixture, with returned
values checked against a withheld oracle and saved screenshot pixels.
Production AUV and its skill are unchanged.

**One completed matched case demonstrated 39.5% fewer total tokens with the
current AUV skill than pure computer use, but took 8.9% longer.** Both methods
returned all eight records correctly. The registered goal of lower tokens and
lower elapsed time in both cases was **not met**. An interrupted run and an
activation mismatch also prevent a clean two-case, three-method comparison.

## Completed matched comparison: case B

The task was to find eight named records distributed through a 180-row canvas
list, return their random codes/statuses with inspected screenshot evidence,
and leave the final requested record visible. Canvas row text was absent from
accessibility. Both arms could batch operations; native could use the scrollbar.

| Method | Verified records | Total input + output | Uncached input + output | Seconds, startup included |
| --- | ---: | ---: | ---: | ---: |
| Pure computer use | 8/8 | 183,773 | 76,381 | 51.5 |
| AUV with current skill | 8/8 | 111,219 | 42,483 | 56.1 |

This is one successful matched task per method, not an average over repeated
successful runs. It supports a bounded token-saving example, not a general
speed, pricing, quota or reliability advantage.

The skill subject read the full frozen skill, batched needed help, then batched
eight sequential direct CLI `scrollUntil` calls with compact JSON and 150 ms
settling. It reused each final capture as evidence. Three shell calls completed;
built-in image-view calls are not enumerated by the CLI JSONL export and are
not counted as zero. No daemon was started in this run.

Native used three batched tool calls and 22 screenshot observations, including
a 20-position scrollbar sweep. The extra intermediate images provide a plausible
mechanism for higher context usage, but the pilot does not isolate image size,
model strategy, cache patterns or tool framing as causal effects. Native captures
were 1800×1364; AUV captures were 900×682. Native batching was permitted and used;
its valid scroll/element arguments were enforced rather than left ambiguous.

CUA supplied JPEG screenshot bytes, while the original relay labeled them
`image/png` and stored `.png` paths. Published copies use `.jpg` filenames without
changing bytes; both native runs returned values matching those images. This
metadata quirk remains a harness limitation and is not a measured format effect.

## Every registered attempt

| Trial | Case | Method | Verified records | Total tokens | Seconds | Outcome |
| --- | --- | --- | ---: | ---: | ---: | --- |
| 01 | A | Pure computer use | 8/8 | 178,015 | 58.5 | Complete |
| 02 | A | AUV without skill | 8/8 | 219,255 | 126.7 | Complete; saved-image audit briefly overlapped |
| 03 | A | AUV with skill | 6/8 | 705,195 | 230.2 | Incomplete; lock-confounded |
| 04 | B | AUV with skill | 8/8 | 111,219 | 56.1 | Complete |
| 05 | B | AUV without skill | 0/8 | 260,363 | 62.0 | Stopped after activation mismatch |
| 06 | B | Pure computer use | 8/8 | 183,773 | 51.5 | Complete |

All attempts and their usage remain in [metrics.json](evidence/repeated-search-benchmark/metrics.json).
No failed goal was discarded, replaced, or pooled into a claimed successful-task
saving. Case A without the skill used more total tokens and took longer than
native; its saved-image audit overlap limits timing interpretation.

Trial 03 returned six screenshot-backed records and unresolved AX target
failures. Immediately after completion, independent CUA verification reported
the Mac locked and manual unlock required after physical input. The exact lock
onset is unknown. This cannot establish an intrinsic AUV driver failure. The
six supplied values and their saved screenshots matched the oracle. Following
user-confirmed manual unlock, case B began from a freshly reset viewport.

Trial 05 returned no records. The subject observed the requested fixture but
stopped when `app.activate` reported `activation_only_foreground_mismatch` with
Safari foreground, including after the permitted recovery. The coordinator's
fresh bound screenshot then showed the fixture at the top. The source of the
foreground mismatch was not measured; do not attribute it to user activity or
a driver bug from this evidence alone. Unrelated foreground capture artifacts
and unfiltered discovery output remain private rather than being published.

## Runner evidence gap observed

In trial 03 the skill subject started one private daemon and selected its exact
Device. Runner `scrollUntil` returned text matches without a final invoke
artifact receipt. The subject reset to top for individual searches and then
repeated navigation/captures for screenshot evidence. These strategy and
evidence costs are counted; the interrupted run does not quantify their
individual effect. Trial 04 instead used direct CLI execution and retained its
final artifacts without that extra navigation.

The Runner-artifact gap was already documented in
[scroll-search evidence](2026-10-06-scroll-search-evidence.md). Candidate next
slice: publish the final Runner scroll-search capture through the same invoke
artifact contract as local execution, avoiding another capture solely for
evidence. This needs owner approval and focused regression/benchmark validation;
it is not implemented in this task.

## Method, verification and limits

The [protocol](evidence/repeated-search-benchmark/protocol.md) preceded trial 01;
execution notes retain the audit overlap, lock and subsequent resume. Two target
sets ran in reverse method order. Six fresh isolated Codex CLI 0.150.1 sessions
used `gpt-5.6-sol`, low reasoning, and a 900-second limit. Both AUV arms used the
same frozen release binary; only with-skill sessions read the full frozen skill.
Automatic skill instructions, project docs, plugins and skill search were
suppressed. No source/answer-file reads, other-trial access, external OCR by
subjects, extra agents or permissions changes were authorized.

Primary tokens are provider input plus output including cached input and skill
loading. Reasoning output is already included and is not added again. Secondary
tokens subtract cached input exactly once. Time starts before model-process
launch and ends at process exit, including discovery, daemon startup where used,
operations, recovery and subject verification. The manual-unlock pause between
sessions is outside each task. Coordinator setup and independent audits are
outside subject measurements. Native CUA relay timing is included and recorded;
this is not a direct-native latency measurement.

Returned codes/statuses were checked exactly against the withheld oracle.
Saved screenshots were audited for each complete name/code/status triple, using
case-insensitive text normalization for OCR evidence only. Audit OCR reads saved
images and never controls the desktop; it shares Apple's Vision implementation
with AUV, so it is not an independent OCR-engine comparison. Fresh CUA final
screenshots separately confirmed the last target for every completed task.
Native and AUV image sizes/frontends differ, and small model samples/cache and
strategy variation remain confounds. The fixture is synthetic and fixed; it is
not a support claim for arbitrary apps, platforms or task categories.

The owned fixture was closed, Safari restored, and all six temporary credential
copies removed. Owned-daemon cleanup completed. Raw traces and trial stores stay
local; published screenshots contain only the synthetic fixture.

## Evidence and reproduction

- [Frozen revision and binary/skill/fixture/adapter hashes](evidence/repeated-search-benchmark/manifest.json).
- [Sanitized usage, outcomes, record screenshot links, hashes and native relay timings](evidence/repeated-search-benchmark/metrics.json).
- [Target sets and synthetic oracle](evidence/repeated-search-benchmark/targets.json).
- [Fixture source](evidence/repeated-search-benchmark/fixture.swift), [native adapter with validation self-check](evidence/repeated-search-benchmark/native-adapter.py), [saved-image audit](evidence/repeated-search-benchmark/audit-images.swift), and [frozen skill](evidence/repeated-search-benchmark/skill/SKILL.md).
- Final requested record: [AUV retained capture](evidence/repeated-search-benchmark/trial-04-record-08.webp), [native screenshot](evidence/repeated-search-benchmark/trial-06-record-08.jpg).
- [Evidence checksums](evidence/repeated-search-benchmark/SHA256SUMS).

Compile the fixture with `swiftc` on macOS and package it with bundle identifier
`local.auv.RepeatedSearchFixture`. Open its window in the foreground and reset
before each session. Withhold source/oracle from subjects. Use the registered
isolation, task and reversed order. The native adapter requires the coordinator's
CUA relay to execute validated batches against that exact app; it is not a
standalone replacement for CUA. Its validation check runs with
`python3 native-adapter.py --self-check`. Freeze the release binary and skill,
count startup and all failures, and independently verify screenshots and values.
The original fixture source/adapter hashes match their published copies.
