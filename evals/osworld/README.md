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

## Fixed paired-remote typed-action infrastructure trial

`k8s_typed_action_adapter.py` is a separate, fail-closed manifest and action
entry for exactly two OSWorld V1 episodes: pinned Chrome
`2ad9387a-65d8-4e33-ad5b-7580065a27ca` and pinned VLC
`5ac2891a-eacd-4954-b339-98abba077adb`. Its checked-in template files
contain only a typed pointer move followed by `DONE`. They exercise the
delivery/Run/evidence path; they are **not task-solving scripts** and their
scores must not be reported as AUV capability. The capture-only adapter and
its existing manifests remain unchanged negative controls.
The pointer move is deliberately non-activating: this slice has not audited
Chrome/VLC focus and shortcut spelling after setup, so a bookmark-manager or
preferences shortcut would silently assume unproven app state. The trial
establishes input transport and evidence plumbing only.

The batch input has exactly `batch_id`, `episodes` (two distinct absolute
paths to existing per-episode capture-adapter config JSON files), and
`action_binary` (absolute path to the pinned host `auv-osworld-action` binary).
The current host binary SHA256 is
`48eedb94c99f7296060aa88d55a9da0a41e2bac36b6c02bad307aa5d54bcd9e9`;
the two template SHA256s are embedded in the adapter, not selected by config.
Each episode config must explicitly select one of the two task IDs, have the
same batch ID, and use unique episode IDs, Pod/Service/proxy names, and local
ports. The existing config validator verifies upstream task bytes/revision,
base and AUV binary pins, and the manifest interpreter's `requests` import.
The separate adapter also verifies the action binary and template byte hashes.
Each typed phase checks the predeclared config SHA256/task ID before
delegating. Boot, install, setup, action, and evaluate also check the
action-binary SHA256; reset intentionally does not, because it never uses that
binary and must still clean up UID-matched resources if it disappears after a
timeout. The fixed action phase runs for at most 600 seconds;
boot/install/setup, pinned evaluation, and UID-safe reset use the existing
adapter's Episode methods. If config drift precedes reset, cleanup fails
closed and is recorded as a separate failure layer; it cannot safely delete
unknown resources.
Each boot creates a fresh disposable qcow2 overlay.

After inspecting both configs, their binary hashes, and the two static
templates, generate and inspect a manifest locally:

```bash
/path/to/python-with-requests evals/osworld/k8s_typed_action_adapter.py manifest \
  --batch /absolute/path/to/typed-batch.json > /absolute/path/to/typed-manifest.json
```

Only after operator approval, pass that manifest to `batch_runner.py` using
the invocation above. The action wrapper binds the Device ID observed by the
existing install phase to that episode's paired profile path, writes the
fixed plan, then runs the pinned host action binary as a foreground child in
the runner's process group. The action binary owns atomic sidecar updates and
identical final stdout; the runner validates both plus the PNG digest. No
arbitrary action argv, shell/Python GUI source, or OSWorld GUI relay is
accepted. The host must be able to reach the guest AUV daemon through the
existing paired port-forward. A frozen two-task typed-move infrastructure
batch passed; its `0.0` scores were expected and are not task-solving rates.
The manifest records the action-entry implementation commit
`349e5c18812337ac9ee7088418eed9ecce53bde4` separately from the measured
binary SHA256. This identifies reviewed source; it does not prove a
reproducible build from that commit. The persisted plan's action array is
re-read and compared with the audited template before launching the binary.
Paired-RPC held-input release/Run finish on hard timeout remains unproven.
The batch runner now gives the action process group at most eight seconds
after TERM before KILL, records whether TERM alone stopped the group, and
marks a forced kill as an unverified input-release failure layer. A graceful
process-group exit still does not independently prove the remote Run outcome.

The infrastructure templates must not silently be repurposed as benchmark
attempts. Separate fixed Chrome and VLC scripted pilots are described below.

## Local typed-action entry

`auv-osworld-action` is a separate foreground entry for an operator-audited,
predeclared sequence. The fixed paired-remote adapter above invokes this
binary with only its audited template plan. The original Chrome and VLC
capture-only controls remain negative controls. The
entry accepts `--plan /absolute/path/to/plan.json` or the separate attended
`--interactive --context /absolute/path/to/context.json` mode, plus the two
runner environment paths. One paired-context fixed plan is:

```json
{
  "version": 1,
  "context": {
    "kind": "paired",
    "device_id": "full-canonical-device-id",
    "config_profile": "episode-profile",
    "profiles_file": "/absolute/path/to/paired-profiles.json"
  },
  "actions": [
    { "action_type": "CLICK", "x": 420, "y": 300 },
    { "action_type": "TYPING", "text": "example" },
    "DONE"
  ],
  "final_settle_ms": 1000
}
```

For an AUV binary running inside the guest, replace `context` with
`{"kind":"guest-local","device_id":"full-canonical-device-id","daemon_endpoint":"unix:///absolute/path/to/auv.sock"}`.
The context and action plan reject extra fields; actions use `parse_action`'s
enumerated structured schema, not shell, Python, or OSWorld `/execute` GUI
relay. The profile file is a credential store, not copied into the plan or
stdout. `final_settle_ms` is optional (default `0`) and must be an integer
from `0` to `5000`. After the last action, the entry waits that long before
the same Runner's final capture. A TERM/Ctrl+C during this delay cancels the
Run, attempts normal cleanup/finish, exits nonzero, and does not produce a
success screenshot. This does not change the no-op `WAIT` control action or
provide intermediate adaptive observation. An operator can invoke the local
entry with:

```bash
AUV_OSWORLD_EPISODE_DIR=/absolute/episode \
AUV_OSWORLD_ACTION_EVIDENCE=/absolute/episode/action_evidence.json \
  cargo run -p auv-osworld-evals --bin auv-osworld-action -- --plan /absolute/plan.json
```

It creates one new AUV Run/Runner, atomically records its Run ID in
`action_evidence.json` before GUI delivery, executes the typed sequence, and
captures `final-screenshot.png` through that same Runner only on successful
sequence completion. `input-action-results.json` stores an array per step of
the original serialized `InputActionResult` values. Normal termination prints
the final sidecar object as the final stdout line; failure or interruption
returns nonzero, may leave `final_artifact: null`, and still attempts to release
held input and finish the Run. Neither delivery nor screenshot is an OSWorld
task score. The local schema, artifact, and sidecar tests are automated; the
entry's real Runner/cancellation gate is ignored until run on an isolated
Xorg guest with a live AUV daemon.

The attended interactive mode takes a strict context file without an action
array; for example:

```json
{
  "version": 1,
  "context": {
    "kind": "paired",
    "device_id": "full-canonical-device-id",
    "config_profile": "episode-profile",
    "profiles_file": "/absolute/path/to/paired-profiles.json"
  }
}
```

Set the same two absolute episode paths, then run
`auv-osworld-action --interactive --context /absolute/context.json` in the
foreground with a JSONL stdin/stdout pipe. It emits a `ready` line containing
the newly persisted AUV Run ID. Send one newline-terminated JSON object per
request, with consecutive `seq` starting at 1:

```jsonl
{"seq":1,"op":"capture"}
{"seq":2,"op":"action","action":{"action_type":"CLICK","x":1270,"y":638}}
{"seq":3,"op":"capture"}
{"seq":4,"op":"finish"}
```

`capture` writes `checkpoint-0001.png`, then atomically updates
`checkpoints.json` with its relative path and SHA256 before responding with
`{"seq":1,"op":"capture","artifact":...}`. Each `action` uses the same
Runner and responds with its unmodified typed `InputActionResult` delivery;
`input-action-results.json` is updated first. `action-requests.json` retains
each validated request before execution. `finish` captures the final PNG and
ends the Run; its response is the final `action_evidence.json` object, byte-for-
byte identical to the final stdout line. `abort`, EOF, SIGTERM/Ctrl+C, protocol
failure, or timeout attempts release/finish and exits nonzero. The Run ID is
already atomic in the sidecar even when there is no final PNG. The sidecar has
no benchmark score; task evaluation remains a separate phase.

The protocol rejects unknown fields and commands, shell/OSWorld GUI relay,
`EXECUTE`, and action-level `WAIT`/`DONE`/`FAIL`; only `finish`/`abort` control
the session. Limits are 64 KiB per line, 1 MiB of total requests, 1,000
actions, 32 checkpoint captures,
60 seconds idle, and 570 seconds after the Run is ready. This is an attended
local diagnostic interface, not a new arbitrary-action Kubernetes phase:
`batch_runner.py` still closes action stdin and the capture-only controls stay
unchanged. The ignored isolated-Xorg gate checks one Run across before/after
captures and typed input; until it is run, live cleanup and Chrome dialog
readiness were tested in a later attended gate. The interactive entry remains
a transport, not an agent policy. The separate scripted Chrome controller
below supplies a fixed observation policy.

### Scripted Chrome V1 controller

`k8s_task_controller.py` accepts one operator-audited Chrome V1 episode and a
SHA-pinned `auv-osworld-action --interactive` host binary. The manifest
contains exactly one episode, so its fixed denominator is one; this is an
end-to-end pilot, not a multi-task benchmark rate. A local batch input has
exactly these fields:

```json
{
  "batch_id": "unique-chrome-batch",
  "episode": "/absolute/path/to/chrome-episode-config.json",
  "action_binary": "/absolute/path/to/auv-osworld-action",
  "tesseract_binary": "/absolute/path/to/tesseract",
  "eng_traineddata": "/absolute/path/to/eng.traineddata"
}
```

The controller reuses the pinned V1 boot/install/setup/evaluate/reset and
UID-checked resource cleanup. In the action phase it reads only AUV checkpoint
PNGs, uses a pinned Tesseract build/model and 1920×1080 spatial OCR gates,
and sends the SHA-pinned five typed actions for the Chrome `Favorites` folder.
Each gate allows at most three observations; an ambiguous state ends the
action without guessing another GUI input. Rust persists the Run ID, original
`InputActionResult` values, checkpoint hashes, and final sidecar; the
controller persists its OCR decisions. The batch runner binds the decision
trace and checkpoint hashes to the pinned policy and Run. Only the selected
upstream evaluator phase supplies the raw score. This is a deterministic
scripted baseline, not autonomous-agent completion. Local archived screenshot
and process-group tests pass. A fresh one-task Chrome V1 batch subsequently
completed on `liet-gpu-1` with raw pinned evaluator score `1.0`, seven
checkpoint observations, no failure layers, and UID-safe reset. That is a
single scripted task result, not an agent rate or a multi-task benchmark.
See the [evidence note](../../docs/ai/references/ops/2026-10-05-osworld-kubernetes-x11-evidence.md)
for Run and artifact hashes.

### Scripted VLC V1 controller and selected two-task batch

`k8s_vlc_task_controller.py` accepts only the pinned VLC `play-and-exit`
task. Its batch input has the same fields as Chrome's plus
`"ffmpeg_binary": "/absolute/path/to/pinned/ffmpeg"` for the 11×11
checkbox crops. The controller verifies the action/OCR/ffmpeg/policy bytes
before manifest generation. Within one paired AUV Run, its bounded state
machine requires MAIN, SIMPLE, a fully redrawn Advanced right pane, the
search query and Playlist result, and the target label plus same-frame
checked/unchecked pixel controls. A stale or ambiguous redraw can cause a
bounded close/reopen; persistent ambiguity, unexpected checkedness, and
changed 1920×1080 geometry fail closed. For this exact task, setup writes
`play-and-exit=1` after VLC launches while the UI shows unchecked; the
script verifies unchecked and clicks Save. It does not toggle the target.
Only the pinned evaluator supplies semantic score.

One fresh VLC six-phase scripted replay passed with raw `1.0` after a
bounded redraw recovery. A later predeclared Chrome→VLC manifest with two
distinct fresh episode configs completed both tasks with raw `1.0` each,
no failure layers, and UID-safe cleanup. That denominator of two measures
selected deterministic scripts and batch plumbing, **not** an autonomous
agent or benchmark-wide completion rate. A prior scripted VLC toggle
attempt failed closed with raw `0.0`; retain it when interpreting the
positive result. See the [evidence note](../../docs/ai/references/ops/2026-10-05-osworld-kubernetes-x11-evidence.md)
for exact manifests, task hashes, Runs, and artifacts.

### Batch runner outcome

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
checks. A separate fresh VLC guest completed its six-phase capture-only
control with AUV Run/PNG evidence and expected raw evaluator `0.0`. Neither
control sent task-solving GUI input.

The K8s manifest pins the Python interpreter used to invoke
`k8s_phase_adapter.py` into every phase command. Generate the manifest with
an interpreter that can import `requests`; generation now fails before boot if
that exact interpreter cannot import it. On the current operator host,
`/Users/neko/.pixi/envs/pip/bin/python` has `requests` 2.34.2, while the
Homebrew Python 3.14 interpreter does not. For example:

```bash
/Users/neko/.pixi/envs/pip/bin/python evals/osworld/k8s_phase_adapter.py manifest \
  --config /absolute/episode-config.json > /absolute/manifest.json
```

The repository [infrastructure plan](../../docs/ai/references/ops/2026-10-06-osworld-auv-infrastructure-plan.md)
and [Kubernetes runbook](../../docs/ai/references/ops/2026-10-05-osworld-kubernetes-runbook.md)
describe the separate live-environment prerequisites and evidence limits.
