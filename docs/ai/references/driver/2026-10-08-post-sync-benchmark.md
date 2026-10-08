# Post-sync repeated visual search benchmark

Classification: test-only benchmark and documentation. Evidence level: six
registered model sessions on the synthetic macOS canvas fixture, with exact
oracle checks, saved-image audits and fresh final-state screenshots.

**The merged version did not demonstrate a token or speed advantage over native
computer use.** Native completed both tasks and used fewer total tokens and less
time in each. AUV without the skill completed both. AUV with the skill completed
one and failed the other on foreground-focus confirmation. The earlier skill
token saving did not repeat. This small pilot does not establish which upstream
change, model strategy or environmental condition caused the difference.

## Every registered attempt

| Case | Method | Verified records | Total tokens | Uncached input + output | Seconds |
| --- | --- | ---: | ---: | ---: | ---: |
| A | Pure computer use | 8/8 | 127,454 | 47,582 | 53.3 |
| A | AUV without skill | 8/8 | 355,425 | 42,977 | 135.5 |
| A | AUV with skill | 0/8 | 232,747 | 29,355 | 66.6 |
| B | Pure computer use | 8/8 | 226,484 | 63,412 | 56.7 |
| B | AUV without skill | 8/8 | 450,950 | 47,878 | 149.9 |
| B | AUV with skill | 8/8 | 532,505 | 51,097 | 189.4 |

Totals include all provider input and output, including cached input, skill
reading, discovery, recovery and verification. Reasoning tokens are already
included in output and are not added again. Wall time runs from model-process
launch through exit, including any task-owned daemon startup. Failed attempts
remain counted; their shorter duration is not a successful-task speed saving.
These are token counts, not monetary-cost or quota estimates.

The subsequent [fixed-operation investigation](2026-10-08-post-sync-isolation.md)
reproduced the foreground-focus rejection in both frozen binaries and retained
search timing variance and capture timeouts. It does not establish the cause of
this full-session spike or a token improvement.

In the completed case B, the skill used **135.1% more total tokens** and took
**233.9% longer** than native. It also used more tokens and time than AUV without
the skill. AUV's uncached-input-plus-output totals were lower than native in
both cases, but the requested total-token measure includes cached input.
The registered requirement of equal success, fewer total tokens and less time
in both cases was not met.

## What happened

Trial 02 used direct CLI calls. It guessed an application search interaction,
encountered foreground focus errors, then completed with background-preferred
input and final-capture reuse. These discovery and recovery costs are retained.

Trial 03 read the frozen skill, started a private daemon, selected its Device
and used Runner calls with foreground-preferred input. `input.clickPoint` and
`input.scrollUntil` repeatedly failed with
`macos native confirm_input_focus failed`. It returned no records; the fresh
final screenshot showed the top viewport. The app was observable through CUA,
but CUA's bound AX observation or window Raise is not independent proof of the
OS foreground recipient. No lock was reported during this run. The cause of
the focus failures remains unclassified; this is not a reproduced proof that
the upstream merge introduced a driver bug.

Trial 04 read the skill and used direct CLI calls with background-preferred
input. It retained final scroll captures, but also navigated again and captured
additional evidence while recovering incomplete row visibility. All eight
returned triples and their screenshots passed the audit; the additional work
is included in its time and tokens.

Trial 05 used direct CLI calls, `window.clickText` to reset the exposed button,
scroll searches and additional window captures. It completed all eight records
and left the last target visible.

Native trials used validated batches and exposed scrollbar values, reading
record contents from screenshots. No answer text was exposed through AX. The
adapter allowed up to 40 deterministic actions per call, rather than forcing
one model turn per action. Native relay service timings and screenshot counts
are retained in the metrics.

## Comparison boundaries

The frozen revision is `50aa4972`, including upstream through `28aa7bf6` and
v0.0.31. The bundled skill, fixture and two sets of eight targets are unchanged
from the [previous pilot](2026-10-07-repeated-search-benchmark.md). Both runs used
Codex CLI 0.150.1, gpt-5.6-sol with low reasoning, fresh isolated homes and a
900-second per-session limit. Order remained native/A, no-skill/A, skill/A,
skill/B, no-skill/B, native/B. No sessions were replaced or stopped because
another method won.

Saved AUV evidence was **900×682**, and native screenshots were **1800×1364**,
as in the earlier pilot. Upstream now captures Retina windows at native
resolution; AUV's artifact emitter renders logical-resolution evidence, so the
saved image dimensions do not reveal the underlying capture/OCR cost. Some
returned artifact metadata records 1800×1364 source dimensions and scale 2.
This is not an isolated before/after speed or token test of capture reuse or
resolution. Model choices, recovery, cache patterns, JPEG versus WebP and
frontend/transport framing also differ. The native relay now labels its JPEG
bytes correctly; it does not convert them.

The fixture was observed at the top before each session. Between trials the
coordinator reset the exposed control and requested window Raise. An independent
global foreground identity was not recorded at every trial start; do not assume
that these actions guaranteed the input-delivery focus contract. The fixture's
initial launch had two CUA timeouts before successful observation, outside all
measured sessions. Saved-image auditing ran only after model sessions, with no
CPU/OCR audit overlap. Physical activity was not continuously monitored, so the
focus errors cannot be attributed to either user activity or a driver defect.

The audit checks exact returned values against the withheld synthetic oracle
and complete name/code/status triples in the referenced screenshot pixels.
Fresh CUA final screenshots separately check the last requested target.
Saved-image OCR uses Apple's Vision, also used by AUV, so it is not an independent
OCR-engine comparison. Subjects assert screenshot inspection; the CLI JSONL
export does not enumerate built-in image-view calls, so their count or absence
cannot be inferred from those logs. Provider usage still includes the session.
The small synthetic sample does not establish general app or platform support.

The owned test window was closed and no benchmark model or task-owned daemon
remained. All six temporary credential copies were removed. Safari was observed
after closure; a subsequent Raise was interrupted by user activity, so no
forced app restoration followed. Raw model logs and stores remain local.
Published images contain only the synthetic fixture.

## Next candidate slice

Investigate the foreground-focus failure with a narrow reproduction comparing
direct and Runner input paths before adding more token optimizations. Separately,
measure capture resolution and evidence reuse with fixed operation sequences to
distinguish runtime cost from model strategy. These are follow-up candidates,
not changes implemented by this benchmark.

## Evidence and reproduction

- [Registered protocol](evidence/post-sync-benchmark/protocol.md).
- [Revision, model and frozen artifact hashes](evidence/post-sync-benchmark/manifest.json).
- [All six outcomes, token usage, screenshot checks and native relay timings](evidence/post-sync-benchmark/metrics.json).
- [Targets and withheld oracle](evidence/post-sync-benchmark/targets.json).
- [Fixture](evidence/post-sync-benchmark/fixture.swift), [validated native adapter](evidence/post-sync-benchmark/native-adapter.py), [saved-image audit](evidence/post-sync-benchmark/audit-images.swift), and [frozen skill](evidence/post-sync-benchmark/skill/SKILL.md).
- [Skill case A final viewport](evidence/post-sync-benchmark/trial-03-final.jpg), [skill case B final record](evidence/post-sync-benchmark/trial-04-record-08.webp), and [native case B final record](evidence/post-sync-benchmark/trial-06-record-08.jpg).
- [Checksums](evidence/post-sync-benchmark/SHA256SUMS).

Compile the AppKit fixture with `swiftc`, package it with bundle identifier
`local.auv.RepeatedSearchFixture`, and withhold its source/oracle from subjects.
Freeze the release binary and whole skill directory. Follow the registered
order, isolation and complete-token accounting. The native adapter needs a
coordinator's CUA relay bound to this fixture; its validation self-check runs
with `python3 native-adapter.py --self-check`. Audit after each measured trial,
retain failures, verify the final viewport and clean up owned processes.
