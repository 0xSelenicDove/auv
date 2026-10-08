# Skill correction: reduce repeated visual-search orchestration

Classification: docs-only skill correction, validated by a test-only model
benchmark. Evidence level: four fresh macOS synthetic-canvas model sessions,
all eight-record tasks independently verified. The revised skill used **78.9%
fewer total tokens** and **52.5% less elapsed time** in this pilot.

## Corrected behavior

The preceding [stall-fix benchmark](2026-10-08-stall-speed-token-benchmark.md)
showed extra resets, captures and model turns. This slice corrects the decisions
in the bundled `auv-computer-control` skill:

- Reuse an available connected Runner; a bounded sequential CLI batch can stay
  direct when no daemon exists. Start a Runner for required reference workflows
  or measured benefits rather than assuming startup saves tokens.
- Use supplied targets and observed foreground state. An operation returning
  usable current evidence can provide the first observation; initial activation
  or a separate capture is conditional on an actual need.
- Preserve the observed viewport for related searches. Reset when navigation or
  recovery requires it, rather than routinely starting each search at the top.
- Inspect existing final-search artifacts. Correct a clipped row locally and
  repeat the same bounded search for fresh evidence; avoid restarting traversal.
- Wait on yielded command sessions through the host completion tool instead of
  polling artifact directories or issuing sleep/probe commands.

Selection, permissions, input policy, full result/failure retention and semantic
screenshot verification remain required. The clipped-row guidance uses the
existing scan loop's initial capture/text check; no new API was introduced.
No runtime code or binary changed. Bundled and installed skill copies match.

## Registered comparison

Both arms used the same `f9abd0e5` release binary, SHA-256
`79f55ecee5b167cc89719f08cb9e36d0e069852bcc54c105e34badec636a4344`.
Current skill was frozen from `dfa4abee`; candidate hashes were registered before
trial 01 and rechecked after completion. Order: current/A, revised/A, revised/B,
current/B. Same task prompts as the preceding pilot except binary/skill/scratch
paths. Subjects received no additional hints about the desired workflow.

Same two sets of eight named records on a 180-row read-only AppKit canvas, top
initial viewport, foreground uncovered window, gpt-5.6-sol low, Codex CLI 0.150.1,
900-second timeout and fresh isolated sessions. No prestarted daemon, automatic
skills, inherited project context, plugins, network searches, extra agents or
other-app access. Trials ran sequentially; no builds or independent OCR audits
ran during measurement. No runtime, skill or prompt edits between trials.

## Results

Total tokens are provider input plus output, including cached input exactly
once. Time includes model launch, skill/help reading, any daemon startup,
recovery, screenshot inspection and final answer. Reset and independent audits
are outside timing. All registered outcomes are retained.

| Task | Skill | Verified records | Total tokens | Seconds |
| --- | --- | ---: | ---: | ---: |
| A | Current | 8/8 | 536,922 | 133.8 |
| A | Revised | 8/8 | 118,398 | 66.2 |
| B | Revised | 8/8 | 142,411 | 65.0 |
| B | Current | 8/8 | 701,243 | 142.7 |

Current totaled **1,238,165 tokens and 276.5 seconds**; revised totaled
**260,809 tokens and 131.3 seconds**, with 16/16 verified records for each skill.
Both matched cases used fewer tokens and less time with the revised skill.
Uncached input plus output also fell from 123,413 to 58,185 (**52.9% fewer**).
These totals are separate from earlier experiments; no historical pooling.

Each revised subject used four completed shell calls, no daemon, eight scroll
searches, and the original returned images. Neither issued standalone window
captures, activation calls or manual scrolls. Each control used 15 completed
shell calls and a private Runner. Control A produced 15 operation results,
including three standalone captures and one manual scroll; control B produced
12, including two standalone captures and one manual scroll.

Control A encountered one foreground-input `System Events ... Application isn't
running (-600)` error and recovered after activation. That failure and recovery
are counted; they confound part of its difference. Control B and both revised
subjects reported zero failed operations. No capture timeout was observed.
The unchanged runtime retains the competing-capture ownership guard.

## Verification and limits

All 32 reported triples matched the withheld oracle and appeared as full rows
on original saved images, ignoring OCR separator punctuation. Every original
artifact hash matched. All four independent final-window images contained the
last requested target; the coordinator visually inspected both revised final
states. Subject image inspection is a claim: the CLI JSON event stream does not
independently expose image-view calls.

Both revised tasks passed, and both bundled/installed skill validators passed.
The registered binary and both frozen skills were unchanged during measurement.
Installed skill files match the tested candidate. Evidence JSON, original image
receipts and pack checksums were validated; `git diff --check` passed. No Rust
code changed, so workspace tests were not rerun for this docs-only slice.

Two tasks per skill form a small synthetic pilot. Model choices, route, image
sizes and OS state vary. The revised sessions demonstrate lower observed costs
with equal verified outcomes; the percentages are not a general desktop speed
or token guarantee. The local clipping-recovery branch was not needed in the
revised attempts. No native or without-skill arm was rerun.

The owned fixture was closed, private subject daemons exited and temporary
authentication copies were removed. No permissions or unrelated settings changed.

## Evidence

[Metrics](evidence/skill-token-fix-benchmark/metrics.json),
[manifest](evidence/skill-token-fix-benchmark/manifest.json),
[protocol](evidence/skill-token-fix-benchmark/protocol.md),
[original images and audits](evidence/skill-token-fix-benchmark/README.md),
[checksums](evidence/skill-token-fix-benchmark/SHA256SUMS).
Raw model transcripts remain local under `docs/notes/skill-token-fix-benchmark/`.
