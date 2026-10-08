# Post-fix repeated visual search benchmark

Classification: test-only benchmark and documentation. Evidence level: six
registered model sessions on the synthetic macOS canvas fixture, exact oracle
checks, saved-image inspection and fresh final-state screenshots.

**The focus failure did not recur, but performance has not returned to the
successful pre-merge case B baseline.** All six sessions completed eight verified
records. The skill used fewer total tokens than native in both cases in this
rerun, but was slower on case B. It used more total tokens than AUV without the
skill across the two cases. The registered requirement of equal success, fewer
tokens and less time in both cases was not met.

## All registered outcomes

| Case | Method | Verified records | Total tokens | Uncached input + output | Seconds |
| --- | --- | ---: | ---: | ---: | ---: |
| A | Pure computer use | 8/8 | 449,174 | 94,742 | 128.5 |
| A | AUV without skill | 8/8 | 323,722 | 42,890 | 111.5 |
| A | AUV with skill | 8/8 | 287,330 | 60,002 | 76.4 |
| B | Pure computer use | 8/8 | 589,832 | 67,848 | 79.8 |
| B | AUV without skill | 8/8 | 337,433 | 36,249 | 123.3 |
| B | AUV with skill | 8/8 | 455,518 | 80,350 | 129.0 |

Total tokens include all provider input and output, including cached input,
discovery, skill reading, recovery and verification. Reasoning is already part
of output, not added again. Wall time includes model-process startup and
subject-owned daemon startup through process exit. Coordinator setup, audits
and teardown are outside measurement. No model session was replaced.

Across the two cases, native used 1,039,006 tokens and 208.3 seconds; AUV without
skill used 661,155 and 234.8 seconds; AUV with skill used 742,848 and 205.4 seconds.
The skill used **12.4% more tokens but 12.5% less aggregate time** than no skill.
These aggregates do not establish a paired speed win: skill case B was slower
than both controls. These are token counts, not monetary cost estimates.

## Before merge, after merge, after fix

| Skill case B | Total tokens | Seconds | Verified records |
| --- | ---: | ---: | ---: |
| Pre-merge | 111,219 | 56.1 | 8/8 |
| Post-merge | 532,505 | 189.4 | 8/8 |
| Current fixed version | 455,518 | 129.0 | 8/8 |

Current skill case B used 14.5% fewer tokens and 31.9% less time than the
[post-merge run](2026-10-08-post-sync-benchmark.md), but still **4.1× the tokens
and 2.3× the time** of the successful
[pre-merge case B](2026-10-07-repeated-search-benchmark.md).
Pre-merge skill case A verified only six records and post-merge skill case A
verified none; their costs cannot represent successful-task performance.
Current case A completed all eight with the repaired focus check.

This is a historical comparison of model sessions, not an isolated causal test
of upstream changes or the fix. The current native sessions also used much more
tokens and time than the previous native controls. Model strategies, cache
patterns, host load and relay scheduling varied. Two tasks per arm cannot
establish stable typical performance or general application support.

## Execution and audit details

Frozen revision `1f1e7a41` retains upstream through `28aa7bf6` and v0.0.31, plus
the [sharing-badge focus fix](2026-10-07-focus-badge-fix.md) and updated reuse
guidance. The benchmark binary SHA-256 is
`f837b61cf2d6000df7007262a7ee5463fefb53578768e5e3ee3b42ce546cfe5b`.
Production code and frozen skill were unchanged during all six sessions.
The same fixture, target sets, order, model gpt-5.6-sol with low reasoning,
Codex CLI 0.150.1, fresh isolated homes and 900-second limit were retained.
Registration was October 7 locally, October 8 UTC; the inherited protocol header
uses UTC. Prompts differ only in frozen binary/skill and task scratch paths.

Skill case A chose direct CLI execution; case B started a private persistent
Runner. Both no-skill cases also started private Runners. Thus a performance
improvement cannot be attributed simply to Runner reuse being exclusive to the
skill. All startup, discovery and commands remain counted. Native used nine
calls and fifteen calls respectively, through the unchanged validated CUA
adapter. Approximate queue waits totaled 42.4 seconds in A and 12.3 seconds in B,
included in measured wall time. These coordinator relay waits materially limit
native speed comparisons; no latency correction was subtracted from results.

AUV saved evidence was 900×682 WebP; native evidence was 1800×1364 JPEG.
AUV capture metadata still reports native 1800×1364 source pixels at scale 2.
This test does not isolate underlying capture/OCR costs or first-call stalls.
The coordinator observed the top viewport and requested Raise between sessions,
but did not independently record global foreground identity at every start.
No independent image audit overlapped a measured session.

Exact returned triples were checked against the withheld oracle, and saved
images were checked for the full name/code/status row. Final CUA captures
confirmed each last target. Vision OCR auditing shares Apple's engine with AUV;
it is independent of command JSON, not an independent OCR-engine comparison.
Subjects claimed image inspection; exported model logs do not reliably enumerate
built-in image-view calls, so their absence cannot be inferred.

Two audit exceptions were retained and resolved without rerunning subjects:
trial 03's original images used `/private/tmp`, an alias of `/tmp`; canonical
path validation confirmed all eight images within the owned trial root. Trial
06's automated concatenated OCR check missed Rowan audit; direct inspection of
the original referenced JPEG clearly confirmed `Rowan audit | JP-9286 | Approved`.
Initial automated audit results and the manual finding are included in the pack.

The owned fixture was closed. No benchmark model or task-owned daemon remained,
and all six temporary credential copies were removed. Other user apps and
daemons were left running. Only synthetic screenshots are published; raw model
logs and stores remain local.

## Evidence and next candidate

- [Protocol](evidence/post-fix-benchmark/protocol.md), [manifest](evidence/post-fix-benchmark/manifest.json), [all six metrics and checks](evidence/post-fix-benchmark/metrics.json), and [checksums](evidence/post-fix-benchmark/SHA256SUMS).
- [Fixture](evidence/post-fix-benchmark/fixture.swift), [targets and oracle](evidence/post-fix-benchmark/targets.json), [native adapter](evidence/post-fix-benchmark/native-adapter.py), [image audit source](evidence/post-fix-benchmark/audit-images.swift), and [frozen skill](evidence/post-fix-benchmark/skill/SKILL.md).
- [Skill A final evidence](evidence/post-fix-benchmark/trial-03-record-08.webp), [skill B final evidence](evidence/post-fix-benchmark/trial-04-record-08.webp), and [native Rowan audit image](evidence/post-fix-benchmark/trial-06-record-07.jpg).

Next candidate: compare fixed operation sequences with stage timings and bounded
CLI/Runner responses to separate capture/OCR latency from model context growth.
No further production optimization was implemented in this benchmark.
