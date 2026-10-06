# OSWorld-V2.1 Codex-agent pilot: frozen protocol

Declared 2026-10-07, **before any episode in this cohort boots**. Status:
planned, no result yet. This is a two-task exploratory AUV-only Codex
sub-agent pilot, not a representative V2.1 score or an official upstream
agent run. Earlier Task099 and Task044 episodes are excluded from this
denominator, including Task044's partial 0.6 and Task099's exploratory zero.

## Membership and identity

The denominator is exactly **2**, in this order: Task099 (Google Maps
Street View coordinate) and Task044 (Shotcut video crop/export). Do not
replace a task after observing progress or a score. Both use the V2.1 source
commit `3d778a3c9a34a079316f70df023b166700445792` on a clean checkout
and the same retained read-only V2 hot qcow2 base SHA256
`28b617987f3edf14edd835069cdde6a6e4708be8e4ae4eb957200d5da335186a`.
Each task gets a fresh writable overlay, its own Pod UID, namespace
`auv-x11-hami-test`, and node `liet-gpu-1` if it remains schedulable.

| Task | Class SHA256 | Gated asset SHA256 | Pinned evaluator |
| --- | --- | --- | --- |
| 099 | `58c460fdfecf518f64714fdc21933b60818d8cf28ec02f9fa15a10e56ef02e32` | `6c99998e7275e2132c6e27d01f212eaabfbf1c10b0e9405d11aa125eb94a16c1` | `v2_task099_evaluator.py` |
| 044 | `3ff702eb197aff0e4987537c7a98c40f50100f8c416371c9f0d424c6a3a860ad` | `987ee02e31537ad83cbe3da15502366d88be944e8b8cec08538adf51185d5108` | `v2_task044_evaluator.py` with `opencv-python==4.8.1.78` |

## Execution policy

- Agent: the **current Codex sub-agent**, one task at a time. Spawn it with
  only the public task instruction, AUV connection/command contract, and
  deadline; do not pass task source, evaluator rubric, output files from old
  attempts, or this protocol's private scoring details. The harness/operator
  handles setup and evaluation separately. The agent's tool access is not
  technically restricted to AUV, so the result must remain labeled
  exploratory even if its action transcript uses AUV only.
- Topology: paired-remote AUV to the guest-installed AUV daemon. No CUA,
  CUA REPL, VNC input, PyAutoGUI, `xdotool`, browser DevTools/CDP, OSWorld
  `/execute` GUI relay, or direct file writes for task-solving. File-only
  setup and read-only evaluation endpoints are allowed to the harness, not
  the agent. A separate guest-local shared-socket cohort is not included.
- Budget: **540 seconds of action time per task**, measured by the harness's
  monotonic clock from the relay's ready receipt, before the first allowed AUV
  observation. Boot, install,
  setup, evaluation, and reset have separate bounded deadlines. No extension
  after partial progress. A timeout terminates the active action relay and
  is reported as such, even if a later acknowledgment arrives.
- Gate: verify source/assets and guest-installed AUV bytes, Pod UID, fresh
  overlay, and setup receipt before giving the instruction. Require AUV
  Run IDs and byte-verified PNG captures. Do not infer semantic success from
  input-delivery receipts, process launch, or a screenshot alone.
- Score: run the pinned original task scorer after action termination on the
  same Pod UID. Record unmodified raw result, evaluator logs, and absence of
  a score separately. An upstream `0.0` is a score; infrastructure or
  evaluator failure is **not** imputed as `0.0`. Report each task's raw score
  and full-completion indicator (`score == 1.0`) with denominator 2; label
  blocked/timeout episodes in that denominator. Do not merge fixed-script
  infrastructure controls into the agent numerator or denominator.
- Reset: remove only task-owned Pod, Service, port-forwards, and paired
  profile with UID/ownership checks. Preserve retained V1/V2 PVCs and verify
  them Bound afterward. Keep raw logs/ledger and hashes outside disposable
  episode resources.

This declaration does not by itself authorize a score claim. The Task044
six-phase UID-bound adapter and both tasks' live prerequisites must pass
before running the pilot. If a prerequisite fails, record the attempted
member as blocked or do not start the cohort; do not silently substitute
another task or change the denominator after an episode starts.
