# Scroll routing: three-method model-session pilot

Classification: test-only evidence and docs. Evidence level: six independently
verified local synthetic AppKit tasks, one nearby and one deep task per method.
All six succeeded. AUV with the installed skill used 17.1% fewer total tokens
than AUV without it in this pilot. This is an observation about the complete
current workflows, not a causal measurement of the individual driver fix.

## Results

Provider input plus output is the primary total, including cached input.
Reasoning tokens are already included in output and are not added again. The
secondary total subtracts cached input once; it is not a price or quota estimate.

| Method | Verified tasks | Total tokens | Uncached input + output | Seconds |
| --- | ---: | ---: | ---: | ---: |
| Pure computer-use | 2/2 | 968,392 | 132,552 | 267.9 |
| AUV without skill | 2/2 | 370,103 | 60,983 | 125.8 |
| AUV with skill | 2/2 | 306,986 | 24,362 | 163.2 |

| Trial | Method | Task | Total tokens | Uncached input + output | Seconds |
| --- | --- | --- | ---: | ---: | ---: |
| 02 | Pure computer-use | Nearby | 159,469 | 30,573 | 51.1 |
| 03 | AUV without skill | Nearby | 163,105 | 35,233 | 55.6 |
| 04 | AUV with skill | Nearby | 125,985 | 10,785 | 63.1 |
| 05 | AUV with skill | Deep | 181,001 | 13,577 | 100.1 |
| 06 | AUV without skill | Deep | 206,998 | 25,750 | 70.1 |
| 07 | Pure computer-use | Deep | 808,923 | 101,979 | 216.8 |

The skill reduced total tokens on both tasks, while taking longer than the AUV
sessions without it. It used three help calls per session versus six, avoided
window discovery for the supplied covered app, and used `input.scrollUntil` for
both searches. The nearby session without the skill used `input.scroll`; the
deep session independently discovered `input.scrollUntil`. These observed paths
explain possible savings, but this sample cannot isolate their contribution.

The native aggregate is especially sensitive to trial 07. Both native sessions
first called scroll without an element and received a coordinate error. Trial 02
recovered with a page-button click. Trial 07 requested six pages despite the
schema maximum of five; the relay did not enforce that maximum, although the
missing element caused the call to fail before scrolling. It then made 21
scrollbar value changes and 23 screenshot observations before finding the deep
record. Those attempts remain included. The schema ambiguity, weak validation,
and native recovery behavior prevent treating the observed 68.3% reduction
against native as a general advantage of AUV.

## Protocol and isolation

The [registered protocol](evidence/scroll-routing-benchmark/protocol.md) and
[setup amendment](evidence/scroll-routing-benchmark/protocol-amendment.md)
preceded the controlled sessions. Trial 01 had duplicate fixture instances and
inconsistent coordinator snapshots. It was designated exploratory before trial
02 started, retained separately with 70,215 total tokens, and excluded from the
aggregate. The corrected cohort required exactly one receiver and one cover,
a fresh top viewport, and a verified foreground cover before each session.

Each session used gpt-5.6-sol with low reasoning, Codex CLI 0.150.1, a fresh
isolated configuration, and a 600-second limit. Automatic skill instructions,
project docs, plugins and skill search were disabled; inherited Codex context
was removed. Native subjects had shell disabled and only a CUA-backed tool bound
to the synthetic receiver. AUV subjects used only the frozen CLI. The with-skill
subjects read the complete frozen 618-word installed skill; the other subjects
read no skill. The audit matched usage to provider turn-completion events and
checked explicit skill reads, backend separation, artifact hashes and setup
window counts. Local temporary authentication copies were removed afterward.

The frozen debug binary contains the
[WindowServer readiness fix](2026-10-06-scroll-focus-ordering.md), commit
`cd37e69875b966c1c9b90ee9d2c000619d4a29da`, on top of `7ca7db53`.
The [manifest](evidence/scroll-routing-benchmark/manifest.json) records binary,
skill and patch hashes. No skill change was tested in this round.

Subjects read fictional records drawn on a canvas, absent from accessibility
text. Nearby required `AR-1012 / Stored`; deep required
`VK-7392 / Ready for review`. Each subject returned a fresh screenshot, followed
by a separate coordinator screenshot verifying the answer and final visibility.
Input-delivery success and OCR matches were not the outcome oracle.

## Limits and evidence

This is one session per task and method, on one model and one synthetic macOS
app, with varying cache reuse. Native captures were 1800×1364 PNG; AUV captures
were 900×682 WebP. The native tool wrapper and coordinator relay differ from a
direct production computer-use tool, and relay time is included in wall time.
AUV built-in image-view call counts are unavailable in the JSONL export and
remain unknown. There is no pre-fix control, repeatability estimate, cross-app
support claim, or pooling with earlier benchmarks.

All outcomes, including the exploratory setup and native error/recovery costs,
are in [metrics.json](evidence/scroll-routing-benchmark/metrics.json). The evidence
folder contains subject screenshots, controlled coordinator screenshots,
prompts, frozen guidance, implementation patch and checksums. Raw model logs and
fixture executables stay local. Fixture launches no longer activate themselves;
the user authorized foreground benchmark sessions. Both test windows were
closed after the final verification, with zero remaining fixture windows checked
through `window.list`.

Candidate next slice: require an observed element for native scroll calls and
enforce conditional schema bounds in the benchmark relay, then repeat the
native comparison. That would remove a known harness confound before drawing
stronger conclusions; it is not implemented in this slice.
