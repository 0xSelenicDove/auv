# Three-arm computer-use session benchmark

Evidence level: measured model sessions with independent native receiver
verification on a synthetic AppKit canvas ledger. Date: 2026-10-06.
Scope: benchmark/docs only; production AUV is unchanged.

## Result

Pure computer-use used the fewest recorded input-plus-output tokens, including
cached input. AUV without the skill used the fewest uncached input-plus-output
tokens and completed both tasks. The skill did not improve this pilot: it used
more tokens than AUV without the skill and failed the nearby task.

| Arm, two sessions each | Verified successes | Tokens including cached input | Uncached input + output | Shell / native MCP calls |
| --- | ---: | ---: | ---: | ---: |
| Pure computer-use | 2/2 | 306,175 | 69,247 | 0 / 11 |
| AUV without skill | 2/2 | 416,504 | 32,760 | 18 / 0 |
| AUV with skill | 1/2 | 498,589 | 65,949 | 23 / 0 |

Compared with pure computer-use, AUV without the skill used **52.7% fewer
uncached input-plus-output tokens**, but **36.0% more tokens including cache**.
Compared with AUV without the skill, the skill arm used **19.7% more tokens
including cache** and **101.3% more uncached input-plus-output tokens**, with
one failure retained. These are two tasks per arm, not a general performance,
billing, quota, or reliability claim.

## Corrected registered trials

| Trial | Arm | Task | Success | Tokens including cache | Uncached input + output | Seconds |
| --- | --- | --- | --- | ---: | ---: | ---: |
| 07 | Pure computer-use | Nearby | Yes | 69,913 | 18,841 | 49.30 |
| 08 | AUV without skill | Nearby | Yes | 232,179 | 15,475 | 77.49 |
| 09 | AUV with skill | Nearby | No | 226,847 | 34,335 | 44.54 |
| 10 | AUV with skill | Deep | Yes | 271,742 | 31,614 | 63.73 |
| 11 | AUV without skill | Deep | Yes | 184,325 | 17,285 | 75.40 |
| 12 | Pure computer-use | Deep | Yes | 236,262 | 50,406 | 136.50 |

Nearby pure computer-use used observe, one page scroll, and observe. Nearby AUV
without the skill used filtered discovery, captures and scrolling. Nearby AUV
with the skill tried `input.key pagedown`, which failed with
`confirm_input_focus`: activation/window focus was not confirmed. A second
keyboard attempt left the viewport at the top; the subject returned `found:
false`. The coordinator confirmed record 012 remained absent. Its short duration
is therefore not an advantage.

Both deep AUV subjects naturally discovered `input.scrollUntil` and verified the
final artifact. Neither was instructed to use that operation. The skill subject
used nine shell calls, while the subject without it used eight. Pure computer-use
used eight native calls: five observations and three down-scrolls of six, six,
and four pages. No code/status was supplied to the measured subjects.

## Method and controls

Registered order: native / AUV without / AUV with for nearby, then AUV with /
AUV without / native for deep. Six fresh Codex CLI `0.150.1` sessions used
`gpt-5.6-sol`, low reasoning and a 600-second deadline. Both AUV arms used the
same immutable `fc9b64c1` binary, containing the upstream sync and fork changes.
The with-skill arm explicitly read a frozen copy of `auv-computer-control`;
its successful full `cat` read was audited in both sessions. No skill-file reads
occurred in the other corrected arms.

Each session had a fresh `CODEX_HOME`, plugins and skill search disabled,
`skills.include_instructions=false`, and `project_doc_max_bytes=0`. Inherited
`CODEX_*` app/thread environment variables were removed before setting the
trial-specific `CODEX_HOME`. Strict config validation accepted these controls.
The skill arm's single explicit guidance file was an exception to the shared
help/output-only reading restriction. The CLI's
[skills configuration schema](https://raw.githubusercontent.com/openai/codex/main/codex-rs/core/config.schema.json)
describes the automatic-instructions control; this report relies on the
installed CLI validation and observed reads, not solely on that live schema.

Pure computer-use had only a small `native_ui` MCP adapter and no shell tool.
The measured model selected actions; the coordinator mechanically relayed them
through native CUA. The adapter returned fresh accessibility state and unmodified
native screenshots, with no OCR or AUV calls. AUV subjects used CLI help, complete
invoke JSON and ordinary image inspection; no native MCP calls occurred.
The tool/explicit-skill audit passed for all six corrected sessions.

Before every session, the coordinator reset the fixture to the top through
native UI and required verified foreground activation of the inert cover.
After every session, fresh native screenshots independently confirmed the
receiver state. Successful nearby answers were `AR-1012 / Stored`; deep answers
were `VK-7392 / Ready for review`. Setup and verification are outside measured
subject token usage. Subject help, guidance, errors, navigation, image inspection
and final answers are inside usage.

Usage comes from actual `turn.completed` records. Input includes cached input;
uncached input + output subtracts cached input. Reasoning output is a subset of
output and is not counted twice. All corrected trials, including the failure,
are retained with complete usage.

## Limits and preliminary runs

This is a cold-session comparison of the exposed interfaces and chosen
strategies. The native adapter is smaller than the complete general CUA surface.
Native screenshots were 1800 × 1364 PNG; AUV artifacts used its normal
900 × 682 WebP capture. Those representation differences affect image and
context costs, so the totals do not isolate the input backend's contribution.
Built-in AUV image-view calls are absent from CLI JSONL and remain unknown.
Native relay scheduling is included in seconds; durations are not a fair
backend-speed comparison.

The native deep subject requested six pages twice despite the MCP schema's
advertised five-page maximum. The relay did not enforce that schema bound and
executed the finite requested amounts unchanged. The full action sequence and
these violations are retained in metrics. This limits interpreting its number
of steps as a result under a strict five-page cap.

Five initial sessions were completed before the isolation audit caught a
problem: a separate `CODEX_HOME` did not suppress global skill guidance by
itself. Trial 05 read a general computer-use skill, and trials 03–04 did not
show successful explicit full reads of the intended frozen skill. Automatic
loading in those runs cannot be confirmed from command JSONL. Their outcomes
remain below and in metrics, but are not pooled with the corrected comparison.
Unstarted trial 06 was cancelled before the new six-trial protocol was registered.

| Preliminary trial | Arm | Task | Success | Tokens including cache | Uncached input + output |
| --- | --- | --- | --- | ---: | ---: |
| 01 | Pure computer-use | Nearby | Yes | 132,197 | 33,253 |
| 02 | AUV without skill | Nearby | Yes | 295,230 | 33,726 |
| 03 | AUV with skill | Nearby | Yes | 321,341 | 32,829 |
| 04 | AUV with skill | Deep | Yes | 1,268,564 | 81,492 |
| 05 | AUV without skill | Deep | Yes | 1,074,502 | 73,158 |

## Evidence and validation

[Sanitized metrics](evidence/three-arm-session-benchmark/metrics.json) contain
all completed trials, provider usage, binary/skill/prompt hashes, verified setup,
receiver outcomes, operation counts, native actions and artifact hashes.
Both [initial protocol](evidence/three-arm-session-benchmark/protocol.md) and
[corrected protocol](evidence/three-arm-session-benchmark/protocol-isolated.md)
were recorded locally before their respective first trials. Final artifacts
were copied byte-for-byte, including the failed nearby trial's top-of-list image:
[07](evidence/three-arm-session-benchmark/trial-07.png),
[08](evidence/three-arm-session-benchmark/trial-08.webp),
[09](evidence/three-arm-session-benchmark/trial-09.webp),
[10](evidence/three-arm-session-benchmark/trial-10.webp),
[11](evidence/three-arm-session-benchmark/trial-11.webp),
[12](evidence/three-arm-session-benchmark/trial-12.png).

Validated completion records, usage arithmetic, isolation/read audit, setup
receipts, copied artifact hashes, frozen skill hashes, removal of trial auth
copies and `git diff --check`. Raw logs and unrelated desktop metadata remain
in ignored local notes. No production edits or upstream PR were made.

Candidate next slice: reduce repeated readiness/help work in the skill and
clarify keyboard-focus recovery using this observed failure. This benchmark
does not implement or authorize that follow-up.
