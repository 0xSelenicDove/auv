# Runner screenshot evidence: matched model-session benchmark

Classification: test-only benchmark and evidence documentation. Evidence level:
four fresh model sessions on the synthetic macOS canvas fixture. The outcome is
**inconclusive**, not a token or speed improvement claim. Each version completed
one of two tasks, and neither task pair contains two successful attempts.

Compared `75260c50` before the fix with `3fe5b264` after the Runner final-observation
recording fix and its skill guidance. The pre-fix binary was the frozen release
from `1f1e7a41`; the production diff through `75260c50` is empty. Both matching
skills and binaries were frozen before trial 01. No runtime or skill edits were
made during measurement. The installed/Homebrew binary was not changed.

## Results

Total tokens include cached input exactly once plus output. Wall time includes
model startup, help/skill reading, daemon startup, recovery and subject screenshot
verification. Setup/reset and independent auditing are outside measurement.

| Task | Version | Verified records | Complete | Total tokens | Seconds |
| --- | --- | ---: | --- | ---: | ---: |
| A | Before | 8/8 | Yes | 569,275 | 216.5 |
| A | After | 1/8 | No | 403,111 | 187.9 |
| B | After | 8/8 | Yes | 391,550 | 201.2 |
| B | Before | 4/8 | No | 330,400 | 219.9 |

The after/A attempt encountered a persistent window-capture timeout and stopped
following a direct-path recovery that reported ScreenCaptureKit capture denial
and failed xcap fallback. After/B recovered from persistent capture stalls using
direct calls, then completed. Before/B also encountered capture timeouts and
failed recovery. These failures were retained; no favorable replacement run was
substituted. Similar capture failure occurs without the new recording RPC, so
this pilot does not identify that RPC as the cause.

Across both attempts, before used 899,675 total tokens and 436.4 seconds, with
12 verified records; after used 794,661 tokens and 389.0 seconds, with 9 verified
records. These aggregate totals mix different amounts of completed work and
cannot establish savings. Uncached input plus output was 110,043 before and
125,861 after. Neither aggregate is a successful-task efficiency estimate.

The successful after/B path used direct AUV calls, which already returned final
screenshots before this fix. Its success therefore does not isolate the new
Runner evidence path. The unit/live regressions in
[the implementation evidence](2026-10-07-runner-scroll-evidence.md) establish that
path's behavior, but this full-session pilot does not establish its performance.
The next measurement candidate is the existing capture timeout/first-call stall,
with the route held fixed; no further production change is part of this report.

## Method and limits

Same 180-row fixture, two sets of eight names, top initial viewport, foreground
uncovered window, model `gpt-5.6-sol`, low reasoning, Codex CLI 0.150.1 and
900-second timeout. Order: before/A, after/A, after/B, before/B. Each session
received its complete matching skill and could choose local calls, a private
Runner daemon or built-in MCP. This compares the runtime and guidance together;
route choice, model variability and capture stalls confound attribution.
Native and without-skill arms were not rerun. No pooling with earlier pilots.

Auditing compared every reported code/status with the withheld oracle, checked
that each retained image contained the complete exact row, and checked an
independent final-window screenshot. All 21 reported rows passed these checks.
Final targets were visible only for the two complete attempts. The coordinator
also visually inspected the final evidence from both complete tasks. Subject
inspection is retained as a claim in the audit; the CLI JSON event stream does
not independently log its image-view tool calls.

All task-owned daemon processes and temporary credential copies were removed,
and the test window was closed. CUA refused access to Codex when restoring that
app; no alternate UI method was used to bypass that restriction. No permissions
were changed.

## Evidence

[Metrics and individual evidence](evidence/runner-evidence-benchmark/metrics.json),
[registered manifest](evidence/runner-evidence-benchmark/manifest.json),
[protocol](evidence/runner-evidence-benchmark/protocol.md),
[failed operation responses](evidence/runner-evidence-benchmark/failed-operation-responses.json),
[checksums](evidence/runner-evidence-benchmark/SHA256SUMS).

The pack retains all four prompts, audits, final-state images, matching skills,
21 record screenshots, usage totals and failure outcomes. Raw model transcripts
remain local under `docs/notes/runner-evidence-benchmark/`; temporary credentials
are excluded from the evidence pack.
