# OSWorld benchmark-local execution

`batch_runner.py` is a local process/ledger gate for a predeclared OSWorld
task set. It does **not** provision Kubernetes, certify that a task uses only
AUV for GUI input, or produce a benchmark-wide completion rate. The manifest
is trusted, operator-audited input. Audit every phase command and its reachable
setup/evaluator code before a live run. In particular, never use OSWorld
`/execute` or `/setup/execute` to dispatch AUV GUI input; the old
`/tmp/auv-osworld-batch-20261005/harness.py` does exactly that and is not an
adapter for this runner.

The module interface is:

```text
python3 evals/osworld/batch_runner.py --manifest MANIFEST.json --output-dir NEW_DIRECTORY
```

The output directory must not exist. All episodes are written to `ledger.json`
as `scheduled` before the first command starts, fixing the denominator. The
ledger is atomically replaced and fsynced after every phase and evidence
transition. The CLI exits 1 if any predeclared episode is not `completed`;
an evaluator score of zero alone is not an execution failure. A benchmark
episode may contain many AUV Runs; the full AUV Run
IDs belong in the action evidence, not in the episode ID.

Manifest shape (all values shown are placeholders, **not** a runnable cluster
configuration):

```json
{
  "trust": "operator-audited",
  "batch_id": "predeclared-batch-id",
  "episodes": [{
    "episode_id": "task-001",
    "identity": {
      "benchmark": "OSWorld-V1",
      "benchmark_revision": "pinned-git-sha",
      "task_id": "official-task-id",
      "task_sha256": "verified-task-json-sha256",
      "topology": "guest-local-shared-socket",
      "runtime_image": "pinned-runtime-digest",
      "qcow2": "pinned-base-image-digest",
      "auv_source": "pinned-auv-git-sha",
      "auv_binary_sha256": "installed-binary-sha256",
      "auv_target": "owner Unix socket or paired Device ID",
      "runner_identity": "actual-runner-or-session-identity"
    },
    "phases": {
      "boot": { "argv": ["audited-boot-program"], "timeout_seconds": 900 },
      "install": { "argv": ["audited-install-program"], "timeout_seconds": 300 },
      "setup": { "argv": ["audited-setup-program"], "timeout_seconds": 180 },
      "action": { "argv": ["audited-auv-agent-program"], "timeout_seconds": 600 },
      "evaluate": { "argv": ["audited-evaluator-program"], "timeout_seconds": 180 },
      "reset": { "argv": ["audited-task-owned-cleanup-program"], "timeout_seconds": 180 }
    }
  }]
}
```

Each command is an argv array, run without a shell in its own process group.
The runner enforces each phase's wall budget with a monotonic deadline, sends
TERM then KILL to the group, reaps the direct child, and checks that no
non-zombie member remains in that process group. The action
command must remain foreground and must not detach descendants into a new
process group. Host process-group termination cannot itself prove that a remote
Pod or guest has stopped changing state, nor can it detect a descendant that
detached into a different group. A real cluster adapter must own Pod
UID checks, fresh qcow2 overlays, and task-owned resource reset before this
gate can be called an unattended Kubernetes batch.

The action process receives `AUV_OSWORLD_EPISODE_DIR` and
`AUV_OSWORLD_ACTION_EVIDENCE`. It must atomically update the latter JSON file
as AUV Runs are observed. A successful action exits 0 and prints an identical
JSON object on its final stdout line:

```json
{
  "run_ids": ["full-auv-run-id-1", "full-auv-run-id-2"],
  "final_artifact": { "path": "final-screenshot.png", "sha256": "actual-file-sha256" }
}
```

The screenshot file must be inside the episode directory. The runner verifies
its bytes against the digest and compares terminal stdout with the sidecar.
An action timeout is still a failed action phase, but if setup succeeded and
the process group stopped, the evaluator runs against the deadline state.
The evaluator must print an upstream result JSON object with a finite numeric
`score` as its final stdout line. Its exact stdout/stderr bytes, paths, and
SHA256s remain in the episode directory/ledger. If evaluation did not run or
failed, `score` and `evaluator_output` are absent, never a fabricated zero. The reset command always
runs, even after an earlier failure, and must end stdout with JSON arrays
`removed_resources` and `retained_pvcs_verified`. Cleanup failure is appended
as a separate layer without replacing the original failure.

Local process, phase-adapter, and pinned evaluator boundary tests run with:

```bash
OSWORLD_V1_CHECKOUT=/path/to/clean/pinned/OSWorld PYTHONDONTWRITEBYTECODE=1 \
  python3 -m unittest discover -s evals/osworld/tests -p 'test_*.py' -v
```

One operator-audited Kubernetes episode has also completed all six phases
on a fresh V1 overlay. Its fixed action was a paired-AUV `display.capture`
only, followed by the pinned Chrome evaluator's expected raw `0.0`. The
evidence [record](../../docs/ai/references/ops/2026-10-05-osworld-kubernetes-x11-evidence.md)
links its ledger with the AUV Run ID, verified PNG digest, and UID-safe reset. This is a
single-task infrastructure negative control, not a task-solving attempt or
multi-task batch. The configured guest AUV binary is tied to a pinned older
source commit; do not treat this as a current-PR-head guest binary test.

The evaluator bridge also has a locally tested allowlist for the pinned V1
VLC `play-and-exit` task. The K8s phase adapter accepts this fixed task ID
for a capture-only control, with independent pinned task bytes and reset
checks. VLC has not yet passed a fresh live guest control; the completed
Kubernetes episode above remains Chrome-only.

The repository [infrastructure plan](../../docs/ai/references/ops/2026-10-06-osworld-auv-infrastructure-plan.md)
and [Kubernetes runbook](../../docs/ai/references/ops/2026-10-05-osworld-kubernetes-runbook.md)
describe the separate live-environment prerequisites and evidence limits.
