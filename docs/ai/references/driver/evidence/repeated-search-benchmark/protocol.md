# Repeated visual record search: registered protocol

Registered 2026-10-07 before model trial 01. Classification: test-only benchmark
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

## Execution note

After native trial 01, the coordinator started a saved-image Vision audit while
AUV trial 02 was launching. The audit completed during that measured session.
This may contend for CPU/OCR resources and affects interpretation of trial 02
wall time; retain the trial and disclose the overlap rather than silently
replacing it. All subsequent saved-image audits run after measured sessions.

Trial 03 returned six records and unresolved AX target failures. Immediately
following model completion, independent CUA final verification reported: the
Mac is locked; automatic unlock is paused because physical input was detected.
The exact lock onset is not measured. This run is retained as externally
confounded, not evidence of an intrinsic driver failure. Trial 04 and later
are pending manual unlock. No automatic unlock or permission change is attempted.

Manual unlock confirmed by the user before trial 04. Fresh CUA state showed the
fixture window and top viewport; it was reset before the resumed case B. The
pause between trials is outside each subject process timing. The binary, skill,
fixture and native adapter remain unchanged across the pause.

Trial 05 stopped after app activation reported a foreground mismatch (Safari).
The source of the mismatch was not measured; retain the failed attempt without
assigning it to physical activity or an intrinsic driver bug. Trials 04 and 06
completed with all eight records correct and screenshot-backed. All six attempts
are now complete and retained; the registered two-case tokens-and-speed win
criterion was not met. No replacement trials were introduced.

Native screenshot bytes were JPEG despite legacy `image/png` relay metadata and
`.png` local names. Published copies correct the filename extension only and
retain identical bytes/hashes. No image conversion or measured rerun occurred.
