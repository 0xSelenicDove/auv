# Compressed AUV skill experiments

Evidence level: eight actual model sessions with independent synthetic native
receiver checks on 2026-10-06. Classification: docs/skill-only; no production
AUV code changes. **Both candidates rejected; prior installed guidance restored.**

The task was to reduce the remaining skill/context overhead. The incumbent is
the 618-word skill from the [covered-app activation pilot](2026-10-06-skill-visibility-benchmark.md).
The first candidate condensed it to 402 words. Its operation routing, targeting,
focus restrictions, coordinate conversion, complete result retention, verification
and bounded recovery were retained, but the explicit warning against root help,
full catalogs and setup probes was omitted. After observing broader discovery,
a second candidate restored that warning and explicitly excluded unused operation
help, bringing it to 421 words. Reference guidance and UI metadata were unchanged.

## Result and decision

Each pilot registered four fresh sessions before its first run. The revision
was designed after inspecting the first pilot, so these are two sequential
experiments, not an eight-session confirmatory comparison. Each has its own
fresh control using exactly the same incumbent. No runs are pooled.

| Pilot | Variant | Successes | Input + output including cache | Uncached input + output | Shell calls | Help-containing calls |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| Compression | Incumbent | 2/2 | 347,643 | 33,531 | 17 | 7 |
| Compression | Candidate | 2/2 | 419,338 | 41,738 | 22 | 12 |
| Compression + discovery restriction | Incumbent | 2/2 | 406,648 | 38,392 | 19 | 6 |
| Compression + discovery restriction | Candidate | 1/2 | 804,902 | 47,398 | 28 | 11 |

The first candidate used **20.6% more total tokens** and **24.5% more uncached
input + output**. Both tasks succeeded, but both candidate runs read root help
and the invoke catalog. Nearby also read unused scroll/click operation help.
The control read the invoke catalog once, never root help. Shorter instructions
did not compensate for broader discovery.

The revised candidate used **97.9% more total tokens** and **23.5% more uncached
input + output**, with only **one of two tasks successful**. Nearby still read
root help. Deep initially followed targeted help discovery, then its search
stopped after eight steps with `end_by_no_visual_progress` (budget 20). It tried
unsupported `input.click --help`, discovered `input.clickPoint`, clicked at the
reset-button location, then made six separate scroll/capture pairs. It reported
`found=false`; the coordinator saw rows 034–042 rather than Kestrel handoff.
Its receipts contain eight captures, one search, one click and six individual
scrolls. These calls and the failure remain in the token totals.

The revised-pilot deep control also encountered a focus-confirmation failure,
then reactivated and completed a second bounded search. Its recovery costs are
included. Neither candidate met the registered retention criterion: lower total
tokens with both tasks successful. The incumbent was restored byte-for-byte.
Both frozen candidates remain experimental evidence, not active skill guidance.

This does not prove compression itself causes failure, or that the omitted
sentence alone caused catalog reads. The model made different discovery, budget,
policy and recovery choices. It does show that these particular shorter
entrypoints did not improve the measured full sessions.

## Every registered session

| Pilot | Trial | Variant | Task | Success | Total tokens | Uncached + output | Seconds |
| --- | --- | --- | --- | --- | ---: | ---: | ---: |
| First | 01 | before | Nearby | Yes | 160,367 | 12,271 | 54.03 |
| First | 02 | after | Nearby | Yes | 216,607 | 27,295 | 47.68 |
| First | 03 | after | Deep | Yes | 202,731 | 14,443 | 62.65 |
| First | 04 | before | Deep | Yes | 187,276 | 21,260 | 55.38 |
| Revised | 01 | before | Nearby | Yes | 162,824 | 20,232 | 34.89 |
| Revised | 02 | after | Nearby | Yes | 179,115 | 13,483 | 63.77 |
| Revised | 03 | after | Deep | No | 625,787 | 33,915 | 123.98 |
| Revised | 04 | before | Deep | Yes | 243,824 | 18,160 | 83.66 |

## Method and limits

Each pilot ran before/candidate nearby, then candidate/before deep. All eight
used immutable AUV revision `fc9b64c1`, Codex CLI `0.150.1`, `gpt-5.6-sol`, low
reasoning and a 600-second deadline. No binary rebuild or execution change was
introduced. Paired common prompts match after normalizing trial paths; no new
strategy hint was added to the task prompt. Every full skill read matches its
frozen variant. All provider `turn.completed` usage records are complete.

Trial-specific homes held only the frozen skill and a local auth copy removed
after completion. Automatic skill instructions, project documents, plugins and
skill search were disabled, with inherited `CODEX_*` context stripped. No other
skill reads or MCP calls occurred. Subjects used only the AUV CLI for fixture
control. Raw logs and unrelated desktop metadata remain in ignored local notes.

The native coordinator reset the synthetic canvas to the top, checked the inert
cover's `verified_foreground` receipt before each run, and inspected each final
native receiver screenshot. Nearby oracle was `AR-1012 / Stored`; deep was
`VK-7392 / Ready for review`. Seven sessions left the correct record visible;
the failed revised deep session did not. Oracle values were withheld from
subjects. Coordinator actions are outside subject token totals; subjects' skill
reads, help, errors, input, images and answers are included.

Primary usage is input + output including cached input. The secondary measure
subtracts cached input; reasoning output is already included in output and is
not added again. Built-in image-view counts are unknown in JSONL. Operation
receipts are deduplicated by run ID and command ID. Final images, including the
failed trial's final image, are copied byte-for-byte and hashed. Small sample
size, adaptive revision, varying recovery and cache patterns limit causal and
general conclusions; these are not billing, platform-support or latency claims.
Editing, remote and background-only workflows were not behaviorally tested.

Validated all eight retained results, normalized prompts, complete reads,
isolation, provider arithmetic, setup/outcome checks, binary/snapshot/artifact
hashes, auth cleanup, restored installed skill, skill validation and
`git diff --check`. No Rust changed; Cargo checks were unnecessary.

## Evidence and follow-up

First pilot: [protocol](evidence/skill-compact-benchmark/protocol.md), [metrics](evidence/skill-compact-benchmark/metrics.json), [incumbent](evidence/skill-compact-benchmark/before/SKILL.md), [candidate](evidence/skill-compact-benchmark/after/SKILL.md); final images [01](evidence/skill-compact-benchmark/trial-01.webp), [02](evidence/skill-compact-benchmark/trial-02.webp), [03](evidence/skill-compact-benchmark/trial-03.webp), [04](evidence/skill-compact-benchmark/trial-04.webp).

Revised pilot: [protocol](evidence/skill-compact-routing-benchmark/protocol.md), [metrics](evidence/skill-compact-routing-benchmark/metrics.json), [incumbent](evidence/skill-compact-routing-benchmark/before/SKILL.md), [candidate](evidence/skill-compact-routing-benchmark/after/SKILL.md); final images [01](evidence/skill-compact-routing-benchmark/trial-01.webp), [02](evidence/skill-compact-routing-benchmark/trial-02.webp), [03](evidence/skill-compact-routing-benchmark/trial-03.webp), [04](evidence/skill-compact-routing-benchmark/trial-04.webp).

Candidate next slice: reduce operation-help response overhead at its owning
CLI boundary while preserving the arguments, defaults and failure contract.
This would reduce the cost of actual discovery calls without relying on another
shorter instruction. No CLI-help feature or driver recovery changes were added
in these experiments; they need their own scoped implementation and validation.
