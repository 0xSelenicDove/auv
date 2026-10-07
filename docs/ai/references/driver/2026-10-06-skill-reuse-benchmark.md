# Final-frame reuse and help-batching skill experiment

Evidence level: four measured model sessions with independent synthetic native
receiver verification, 2026-10-06. Scope: skill/docs experiment, no AUV execution
code changes. **Candidate not promoted; the previous installed skill was restored.**

## Result and decision

The candidate made returned `scrollUntil` final-frame reuse explicit, with a
fresh-capture fallback for missing artifacts or changed/uncertain views. It also
made batching already-known independent help pages into one shell/tool call more
explicit. Both variants completed both tasks, but the candidate used **2.0% more
total input-plus-output tokens including cached input**, and **85.1% more uncached
input-plus-output tokens**. It did not meet the total-token optimization goal.

| Metric, two sessions per variant | Before | Candidate |
| --- | ---: | ---: |
| Verified successes | 2/2 | 2/2 |
| Tokens including cached input | 566,436 | 577,557 |
| Uncached input + output | 45,732 | 84,629 |
| Shell calls | 28 | 25 |
| Shell calls containing help | 10 | 7 |
| Capture invokes after a scroll search | 3 | 2 |

Help batching was adopted in both candidate sessions: three commands in one
shell call for nearby, and a three-operation loop for deep. Neither baseline
session batched help. The metrics distinguish commands containing multiple
literal help flags from the loop case; that flag count alone is not a complete
batch detector.

The candidate nearby run returned the search's final-frame path and did not
recapture after the search. The candidate deep run inspected that frame but
interpreted the common fresh-verification requirement as a reason to capture
again. Its first final capture could not resolve the now-hidden application;
reactivation and another capture succeeded. The baseline deep run also naturally
returned a search artifact after recovery and a second bounded search. Thus
**both arms used search evidence for one of their two final answers**. More
explicit guidance did not produce consistent additional reuse.

All trials encountered visible-target resolution failures before activation or
during recapture. The candidate nearby run made two failed initial captures,
then resolved/activated the allowed fixture and succeeded. The candidate deep
run also encountered zsh's read-only `status` variable during bookkeeping,
requiring another read to expose the saved result. Failures, recovery, help,
continuations and final answers remain in the measured totals. No result was
excluded or replaced.

The earlier shorter-skill improvement remains installed. The candidate's frozen
snapshot is retained as experimental evidence, not as active guidance. The
negative pilot does not establish that reusing a current frame is unsafe or
that batching cannot help; it shows that this added instruction text did not
reduce full-session usage in these registered runs.

## All registered trials

| Trial | Variant | Task | Total including cache | Uncached input + output | Seconds | Shell calls | Final search artifact used |
| --- | --- | --- | ---: | ---: | ---: | ---: | --- |
| 01 | Before | Nearby | 263,602 | 27,570 | 58.10 | 12 | No |
| 02 | Candidate | Nearby | 247,075 | 37,155 | 48.65 | 11 | Yes |
| 03 | Candidate | Deep | 330,482 | 47,474 | 81.79 | 14 | No |
| 04 | Before | Deep | 302,834 | 18,162 | 107.62 | 16 | Yes |

## Method and limits

The [protocol](evidence/skill-reuse-benchmark/protocol.md) was recorded before
trial 01: before/candidate nearby, then candidate/before deep. The baseline
snapshot exactly matches the installed shorter skill from the
[previous experiment](2026-10-06-skill-token-benchmark.md). Both variants used
the same immutable `fc9b64c1` AUV binary, Codex CLI `0.150.1`, `gpt-5.6-sol`,
low reasoning and a 600-second deadline. Common task, bounded recovery,
complete-JSON and screenshot-verification instructions were unchanged; paired
prompts differ only in trial-specific paths. Artifact reuse and help batching
were not added as extra hints to the common prompt.

Fresh trial-specific `CODEX_HOME` directories held only the variant skill and
an existing local auth copy removed after completion. Plugins, skill search,
automatic skill instructions and project documents were disabled. Inherited
`CODEX_*` app/thread environment was stripped. Complete successful skill reads
were checked against each frozen file; no other skill reads or MCP calls
occurred. All four provider usage records are complete.

For every trial the coordinator reset the synthetic canvas fixture to the top,
verified foreground activation of the inert cover, and independently checked
the final native receiver screenshot. Nearby code/status was `AR-1012 / Stored`;
deep was `VK-7392 / Ready for review`. These coordinator actions are outside
subject token counts; subject help, skill loading, errors, images, input and
answers are inside actual `turn.completed` usage.

Input includes cached input. Uncached input + output subtracts that cached
subset; reasoning output is already included in output. CLI JSONL does not
expose built-in image-view call counts, which remain unknown. A reused final
answer path is checked against the exact `auv.scan.scroll_until_final_capture`
receipt. All screenshots are copied byte-for-byte with hashes. Two sessions per
variant, differing recovery/settling strategies and cache-hit patterns limit
causal conclusions and do not establish billing, quota, backend latency or
platform support. Earlier experiment totals are not pooled or used as this
comparison's control.

## Evidence and follow-up

[Metrics](evidence/skill-reuse-benchmark/metrics.json) retain provider usage,
setup/outcome checks, hashes, failure messages, operation counts, help-batch
adoption, post-search captures, final artifact reuse and the decision to restore
the incumbent. Frozen [before](evidence/skill-reuse-benchmark/before/SKILL.md)
and [candidate](evidence/skill-reuse-benchmark/after/SKILL.md) snapshots include
their unchanged operation references. Final images:
[01](evidence/skill-reuse-benchmark/trial-01.webp),
[02](evidence/skill-reuse-benchmark/trial-02.webp),
[03](evidence/skill-reuse-benchmark/trial-03.webp),
[04](evidence/skill-reuse-benchmark/trial-04.webp).

Validated full reads, matching normalized prompts, isolation, provider arithmetic,
receiver state, verified setup, copied/frozen hashes, restored installed content,
auth-copy cleanup, skill validation and `git diff --check`. Raw logs and unrelated
desktop metadata remain in ignored local notes. No Rust code changed.

Candidate next slice: avoid repeated visible-window lookup failures for a known
covered app by making the permitted activation/observation sequence clear. This
needs a task/policy boundary so observation alone does not unnecessarily steal
focus. It was not implemented in this experiment. Zsh-safe bookkeeping is
another observed follow-up; neither finding expands this slice into driver or
frontend changes.
