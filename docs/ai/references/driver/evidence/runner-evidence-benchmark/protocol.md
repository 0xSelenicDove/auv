# Repeated visual record search: registered protocol

Registered 2026-10-08 before model trial 01. Classification: test-only benchmark
and docs; no production runtime or skill changes. Hypothesis: repeated visual
search with bounded internal navigation may reduce total model tokens and elapsed
time relative to native computer use. A result is not predetermined.

Reuse the synthetic AppKit canvas ledger, with 180 rows and randomly generated
codes/statuses. Eight named landmarks per case, distributed through the document.
Two cases, A and B; each arm receives identical targets and initial top viewport
within a case. The target app is foreground, no cover. Names are supplied but row
positions, codes, status and fixture source are withheld. Each task returns all
eight exact name/code/status triples with screenshot evidence, and leaves the
last requested target visible. No accessibility text exposes canvas row content.

Six fresh isolated sessions: native/A, AUV without skill/A, AUV with skill/A,
AUV with skill/B, AUV without skill/B, native/B. Same model gpt-5.6-sol, low,
Codex CLI 0.150.1; 900-second timeout. Fixed release binary and current bundled
skill, hashes in manifest. No automatic skills, project docs, plugins, skill
search, inherited Codex context, other trials, source/answer-file reads, other
apps, installations, permission changes or extra agents. Trial credentials are
local temporary copies and removed on completion; never published.

Native uses CUA-backed MCP bound to the exact test app. Shell is disabled, but
arbitrary deterministic action batches are allowed (up to 40 actions per call).
Fresh AX state is returned after each action; observe also returns an unmodified
window screenshot. Scroll requires an observed element and validates page bounds
0.1..5 before any action. Entire batches are validated first. Scrollbar value
changes and keys are allowed. Regression checks reject the two previously
observed malformed scroll requests. Native preflight with valid scroll succeeded;
reset to top precedes every measured session. No deliberate native disadvantage
from missing target arguments or unvalidated page limits is retained.

AUV subjects can batch CLI commands/scripts and choose direct execution, a private
daemon or the binary's built-in MCP. No daemon is supplied; startup/discovery is
inside measurement. Daemons may use only task-owned private Unix sockets/stores,
no default registration or network listener, and must be cleaned up. Without-skill
arm discovers syntax from installed help; with-skill arm must read the complete
frozen bundled skill and may read its references. All JSON fields/failures retained.

Primary outcomes: verified task success (8/8 exact records, valid inspected
screenshots, final target visible), provider input+output including cached input,
and wall time from before model-process launch through model-process exit,
including discovery, skill reading, daemon startup, recovery and subject
verification. Secondary tokens subtract cached input exactly once. Teardown after
exit is reported separately if needed. Count failed/timed-out attempts; do not
substitute successful runs. Setup/reset and independent coordinator audit are
outside subject timing/tokens. Audit result against withheld oracle, evidence
provenance and screenshot pixels; final app state is checked independently.

A win requires equal verified success, lower total tokens and lower elapsed time
in both paired cases. Otherwise report the observed tradeoff. Two tasks per arm
remain a small synthetic pilot, not general app support or marketing evidence.
No pooling with prior experiments. Native/AUV image sizes, frontend differences,
CUA relay overhead and per-call CLI transport are recorded as confounds. Relay
polling/service latency is included in native time; no direct-native latency
claim. No stopping early because one method wins. Publish all six outcomes.


Current registration: four fresh sessions before/A, after/A, after/B, before/B. Both arms read their frozen skill. Prior six-way results are historical only; no pooling. Production and skill revisions are 75260c50 and 3fe5b264. Same fixture, targets, model, limits, screenshot audit and foreground/no-cover setup. Primary comparison requires equal success and lower total tokens and wall time in both paired cases. No source edits during trials.
