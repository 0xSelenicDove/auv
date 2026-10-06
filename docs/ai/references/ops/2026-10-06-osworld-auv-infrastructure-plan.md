# OSWorld on AUV: infrastructure plan and TODO

Date: 2026-10-06

Status: In progress. This plan covers computer-use infrastructure before any
claim about agent or harness task success. The implementation branch is
`codex/linux-x11-compatibility` (draft PR #233). Re-check the current `main`
and this branch before taking each item; the plan is a work queue, not evidence
that a task has passed.

## Goal and evidence boundary

Run OSWorld V1 and OSWorld-V2.1 on the ihome Kubernetes cluster with all GUI
observation and input delivered through installed AUV. Exercise both a paired
remote client and a guest-local client sharing the daemon's Unix socket. The
official benchmark's setup and evaluator endpoints may create/reset a guest,
install AUV, and retrieve results; they must not deliver GUI actions. Do not
use CUA/CUA REPL, VNC input, PyAutoGUI, `xdotool`, or OSWorld `/execute` for GUI
input.

The current [evidence](2026-10-05-osworld-kubernetes-x11-evidence.md) includes
an evaluated GIMP task on both official images, separate action-level baselines,
and one completed V1 capture-only Kubernetes episode. It does not prove an
OSWorld completion rate. The
[runbook](2026-10-05-osworld-kubernetes-runbook.md) records the existing KVM Pod
and guest installation route.

## Current state that changes the work order

- PR #233 contains the X11 driver, `input.scrollPoint`, pointer-position read,
  and disjoint X11 key holds. The baseline exposed F13 rejection; a later
  direct-Xorg receiver test verified the fix. The 2026-10-06 official V1/V2.1
  guest rerun also observed F13 through installed AUV.
- Current `main` adds a shared scroll contract: positive `delta_y` moves the
  viewport down and deltas are logical pixels, plus window-targeted
  `input.scroll`/`ScrollWindowPoint`. See
  `docs/ai/references/driver/2026-10-06-scroll-delta-contract.md` at `main`.
  X11 screen-point scrolling must keep that contract and be tested against an
  observed scroll position. A wheel event alone is weaker evidence.
- Current `main` changes listener authentication and pairing. `listen` is a
  `serve --listen URI` option, not a separate command. The 2026-10-06 paired
  Mac→V2.1 test used the owner Unix socket to create a token and the current
  authenticated HTTP route.
- The initial merge-tree check predicted two content conflicts
  (`crates/auv-driver/Cargo.toml` and `proto/auv/api/driver/v1/input.proto`).
  Both were resolved in merge commit `875e259e`; use the actual merged source
  and its tests for current behavior, not that earlier forecast.
- On 2026-10-06 the retained direct-Xorg Pod `auv-osworld-x11` is **Evicted**
  on `neko-gpu-1` because its node crossed the ephemeral-storage threshold;
  both containers are `ContainerStatusUnknown`. The node itself is Ready and
  the retained PVCs remain Bound. This Pod is not a live receiver and must not
  be used to claim a current X11 result. Recreate a task-owned fixture with
  measured storage requests or use an isolated Linux/Xvfb receiver for future
  probes; preserve the existing PVCs. The official-guest rerun used separate
  task-owned Pods, which were cleaned after evidence capture.

## Ordered TODO and acceptance gates

### 1. Align PR #233 with current `main` — code and local gates complete

Merge or rebase the current base into the draft branch while preserving the
X11-only screen-point scroll and the newly added window-targeted scroll as
distinct contracts. Resolve both source conflicts by meaning, regenerate or
check Protobuf bindings, update the runbook's old `--no-discovery` examples to
the current `--no-register`/owner-socket contract, and update PR API disclosure.
Verify `cargo fmt
--check`, targeted Rust tests, `buf lint`/breaking checks, and a Linux Xvfb
receiver test. Do not infer that a clean merge means the two scroll routes have
equivalent units or delivery behavior.

Completed in merge commit `875e259e`: the two APIs remain distinct, X11
screen-point deltas now use logical pixels, and the runbook uses the current
listener syntax. Targeted Rust/Buf/SDK checks, X11 unit tests (17/17), and an
Xvfb integration test (1/1) passed. The Tk receiver's `yview` increased after
`+120`, proving downward motion but not an exact 120 px displacement. The
  existing window-scroll live evidence belongs to `main`; this merge did not
  repeat it. The PR disclosure and official-guest screen-point rerun were
  subsequently completed, as recorded in the evidence note.

### 2. Build a typed OSWorld action adapter — local full-action gate passed

Make one harness-side consumer that maps the official structured action set to
existing AUV Driver/Runner capabilities. Reuse one persistent Runner/session
for independent `KEY_DOWN`/`KEY_UP` and `MOUSE_DOWN`/`MOUSE_UP`; do not add
one-shot CLI commands for stateful ownership. Map no-coordinate click and
current-cursor drag through pointer position, define a deterministic rounding
policy for float coordinates, and convert OSWorld vertical-scroll sign and
step units to AUV logical pixels. Keep V2 `EXECUTE` in the benchmark control
plane; do not expose arbitrary shell execution as AUV GUI input. Assert the
observable AUV calls and native receiver state for each action family, plus
error/cleanup behavior for unknown actions and interrupted holds.
In particular, test interleaved modifier holds, held-key plus `HOTKEY`, and
mouse down/move/up sequences. If the current driver cannot preserve an
officially valid combination, report that exact combination as unsupported
instead of synthesizing a different input sequence.
The existing Runner bounds one held key to 30 seconds. If an OSWorld action
sequence requires a longer uninterrupted hold, report that case as unsupported
and capture the sequence; do not silently release/repress or claim equivalent
behavior.

The protocol and Runner handler already had `KeyDown`/`KeyUp`, but the public
Rust `InputClient` initially exposed `hold_keys` only. The narrow typed client
calls needed by this adapter must map requests and results without making the
harness reach into generated gRPC stubs or invent a second hold-result schema.

The typed-client subtask is implemented but does not complete item 2:
`InputClient::key_down` returns a hold ID and typed action result, and
`key_up` releases by ID. A real gRPC fixture checks request mapping and
Noop/NotFound result semantics. The adapter must still own these IDs and
explicitly release them; the client does not provide an automatic release
guard.

The parser subtask is also implemented under `evals/osworld/`. It accepts the
pinned V1/V2.1 structured actions in flat or nested form, validates defaults
and bounds, and rejects free-form code and V2 `EXECUTE`. It retains raw signed
wheel steps. Upstream `ACTION_SPACE` labels both scroll axes required, but its
`execute_action` implementation permits either axis alone; the parser follows
the executable behavior and normalizes the omitted axis to zero. Passing this
parser is not evidence that an AUV/X11 backend can deliver every upstream key
name.

The persistent `ActionExecutor` now maps the validated GUI action variants
through one public `auv::Client` Runner, owns cross-call key and mouse holds,
converts scroll steps using the calibrated X11 mapping, and releases known
holds on an error or explicit finish. It returns AUV delivery results with
semantic verification unset. Simultaneous mouse-button chords are explicitly
unsupported because the current MouseCoordinator permits one held button per
desktop. The executor rejects key spellings without an exact X11 mapping;
holds are bounded by the Runner's 30-second limit rather than silently
extended. `cargo test -p auv-osworld-evals --lib` passed 8 unit tests. The
ignored public-Runner Xorg integration test was later run explicitly against
the installed AUV daemon in each official guest and passed 1/1 in both, with
independent receiver evidence. It covers a stateful subset; separate AUV CLI
probes confirmed additional primitives, but those do not prove that every
`ActionExecutor` variant passed end-to-end. This is not a benchmark score.

A later test-only Xorg action matrix submitted every supported structured GUI
action family through `ActionExecutor` on an isolated Docker/Xvfb desktop. The
independent Tk receiver checked mouse/key/text/cleanup; a raw `xev` window
checked both scroll axes and delivery order. The final matrix passed 1/1 after
adding an exact no-coordinate-click receiver assertion. See the
[evidence note](2026-10-05-osworld-kubernetes-x11-evidence.md) for image,
binary, and test hashes. This completes the local adapter-delivery gate while
leaving the official guest full-matrix rerun and benchmark evaluator separate.

Placement proposal for review: use a separate `evals/osworld/` harness area.
`evals/auv-base/` currently documents Python-free, application-receiver
evaluations and reserves its Linux platform directory for those tasks. The
official OSWorld environment/evaluator is Python-based and owns QEMU reset and
task assets, so putting its adapter in the X11 driver or the existing base
receiver suite would conflate responsibilities. Confirm this boundary when the
first adapter slice is implemented; the path is provisional, not a new core
runtime crate.

Before implementation, inspect the exact official V1/V2 action parser and
current AUV APIs. The adapter should live with benchmark harness code, not in
the X11 driver or generic CLI. If the official action format contains
free-form PyAutoGUI source, accept only a deliberately enumerated subset or
use the upstream structured form; never evaluate source text.

The pinned official V1
[`actions.py`](https://github.com/xlang-ai/OSWorld/blob/b138d348256078fa634fc3b73567a7337c793e6b/desktop_env/actions.py)
and
[`execute_action`](https://github.com/xlang-ai/OSWorld/blob/b138d348256078fa634fc3b73567a7337c793e6b/desktop_env/controllers/python.py)
confirm both nested `parameters` and flattened action dictionaries. V2 adds
`EXECUTE` to the same GUI list. The official `DesktopEnv.step` dispatches
structured `computer_13` actions back to the guest's PyAutoGUI controller;
the AUV harness must therefore own step/history/observation wiring rather
than call that dispatch path for GUI input. Official `SCROLL` passes integer
`dx`/`dy` to PyAutoGUI's horizontal/vertical wheel calls, while AUV's new
contract is logical pixels. The task-owned Xvfb calibration at AUV head
`20c43090` found one AUV 120-pixel step produced one matching X11 wheel-button
event; PyAutoGUI's X11 backend emits one such event per integer step. The
event-equivalent adapter mapping is `auv_dx = 120 * osworld_dx` and
`auv_dy = -120 * osworld_dy`. Preserve the official horizontal-before-vertical
order when sending separate calls. This does not establish exact displacement
in a particular application's viewport; the later official-guest receiver
rerun is recorded under item 3.

### 3. Re-run capability gates in both official guests — guest-local action matrix passed

Build an Ubuntu 22.04-compatible AUV from the aligned branch and install it
into fresh V1 and V2.1 overlays. In each image, test the typed action adapter
against an independent receiver: screenshot, all mouse buttons and counts,
move/drag, both scroll axes with measured scroll position, ASCII/Unicode
typing, ordinary/special keys including F13, overlapping modifier holds, and
cleanup after failure. Record binary/image revisions, run IDs, receiver logs,
and screenshot digests. Run both guest-local shared-socket and paired remote
topologies with the current `serve --listen` authentication flow. A returned
delivery result is not enough without receiver or semantic observation.

The 2026-10-06 rerun passed the public `ActionExecutor` stateful episode in
both installed official Xorg guests through their owner Unix sockets. Tk and
`xev` independently confirmed the stateful subset and separate AUV CLI probes
for both wheel axes, mouse buttons/counts, held drag, Unicode text, F13, and
overlapping modifiers. A paired Mac→V2.1 route also passed authenticated
remote AUV display capture and a click received by guest Tk. Exact binary
hashes, image digest, Run IDs, receiver-log hashes, and limits are in the
[evidence note](2026-10-05-osworld-kubernetes-x11-evidence.md). The extra
primitive probes were not all submitted through `ActionExecutor`; the paired
remote route did not run the full persistent episode; V1 paired remote was not
rerun. Those are coverage gaps, not hidden passes. No evaluator score or
GPU/DRA behavior follows from this baseline.

The later complete `ActionExecutor` matrix also passed 1/1 through installed
AUV and independent Tk/`xev` receivers in **each** official guest (V1 2.65 s,
V2.1 2.71 s). Exact test executable, binary, and result hashes are in the
evidence note. This closes the guest-local adapter-delivery gate, not a full
paired-remote episode or a task-evaluator gate.

### 4. Batch scheduler and evaluator pilot — two-task transport batch passed; task-solving batch pending

Only after action gates pass, run a small, reproducible official-task batch on
fresh overlays. Each episode needs a known image revision, task assets and
mocked services, reset policy, timeout, AUV run IDs, final artifact, and exact
upstream evaluator output. The initial batch should include multiple action
families, not only the existing GIMP setting task. Report pass/fail/blocked
and denominator explicitly; distinguish environment failure, adapter failure,
agent failure, and evaluator failure. Confirm OSWorld-V2 dataset access before
promising a full 108-task run.

For the first two-episode V1 pilot, fix the **action phase** at 10 minutes per
task. Give boot, installation, setup, and evaluation their own bounded
deadlines; they do not consume the action budget. Enforce deadlines with a
monotonic clock and record UTC start/end times for review. Start each task
from a fresh writable overlay backed by the same read-only pinned qcow2.
Do not extend a running episode after seeing partial progress. This is an
exploratory pilot policy, not a claim that the official benchmark uses the
same timeout.

The batch ledger must keep an evaluator score separate from execution status:

| Field group | Required evidence |
| --- | --- |
| Identity | batch/episode ID, pinned benchmark/task revision and task hash, topology, runtime image and qcow2 identity, installed AUV source and binary SHA |
| Phases | boot/install/setup/action/evaluate/reset UTC boundaries, action budget and terminal reason |
| AUV | device/Runner identity, action Run IDs, final screenshot artifact and digest |
| Outcome | raw evaluator output and score **only if evaluation ran**; otherwise a named failure layer and no invented `0.0` |
| Cleanup | exact task-owned resources removed, retained PVCs verified, cleanup failure if any |

Count all two scheduled tasks in the pilot denominator, including blocked and
timed-out episodes; report their status individually. An evaluator-returned
`0.0`, an absent answer file, a setup failure, and an unavailable evaluator
are four different results. A fixed action script can test scheduler plumbing,
but only a blinded agent attempt measures agent behavior. The old scratch
`/tmp/auv-osworld-batch-20261005/harness.py` and `slot.py` are not a compliant
implementation: they launch AUV GUI commands through OSWorld `/setup/execute`
and use stale listener flags. Preserve them as historical scratch only.

`evals/osworld/batch_runner.py` now supplies the benchmark-local process and
ledger gate for an operator-audited, predeclared manifest. It records all
scheduled episodes before execution, bounds each phase independently, stops
the action process group at its deadline, records AUV Run IDs and final
screenshot digest from an action sidecar, and keeps an absent score distinct
from evaluator-returned zero. The local tests cover timeout, interruption,
evaluator failure, cleanup layering, and both target identities. This is not
yet a cluster batch: the runner does not verify that manifest commands use
AUV-only GUI delivery, stop detached or remote processes, pin a Pod UID, or
prove a fresh qcow2 overlay and task-owned reset. Those belong to the next
audited Kubernetes phase adapter before any unattended batch claim.

An experimental V1 Chrome paired-remote adapter now fixes the six phase
commands around a capture-only negative control. Its local tests cover pinned
task/binary configuration, guest and overlay identity checks, AUV-only pixel
observation, task-owned port-forward lifetimes, and UID-preconditioned reset.
The V1 hot qcow2 digest and proxy image were measured and audited for the
first live gate, which stopped in boot before the AUV action phase; a
create-to-ownership-journal crash gap still requires manual recovery. A
capture-only `0.0` would be a scheduler negative control, not an agent attempt
or AUV GUI-input capability result.

The first live control stopped in `boot`: the pinned runtime launched QEMU
with `-hda /boot.qcow2`, not the adapter's assumed `-snapshot /System.qcow2`.
No installation, action, or evaluation occurred. Reset's 30-second Pod
disappearance check also timed out; the task-owned proxy vanished shortly
afterward, and the remaining Pod and Service were removed with UID
preconditions. All task resources are absent and the hot PVC/PV remained
Bound. The reset observation policy has since been widened with a regression
test, but not yet rechecked live. Before retrying, verify the image's actual
disposable-disk lifecycle. Retain the failed episode and denominator in the
[evidence note](2026-10-05-osworld-kubernetes-x11-evidence.md).

A second, newly named episode passed the corrected live backing-file overlay
audit and UID-stable boot. It stopped in install because the adapter parsed
the pinned upstream `/setup/launch` success text as JSON. A third episode
passed boot and the corrected launch parser, then stopped on an invalid
pairing-token output shape. The pinned `/setup/execute` reports HTTP 200 even
for a nonzero guest command; the old adapter ignored its exit code, so the
third gate cannot establish whether the command failed or returned malformed
stdout. Test-first changes now require zero exit and report only token-safe
metadata on failure. Reset succeeded in both episodes and removed all task
resources without changing the hot PVC/PV. All three failed episodes remain
in the evidence note; none contributes a benchmark score. The revised
pairing diagnostics still need a fresh live gate.

The fourth live gate used those diagnostics and stopped at the installed
binary's `--version`, before daemon launch or token generation: child exit
127 despite HTTP 200/`status=success`. The stderr SHA256 and byte count
exactly match the missing-`libtesseract.so.4` dynamic-loader message for the
measured ELF. It again passed boot and UID-safe reset, leaving no AUV Run or
score. Next, make this guest library prerequisite explicit and verify it in a
fresh episode before claiming installation or evaluator success.

The adapter now installs only the three packages previously validated in the
runbook, using the public V1 image sudo password, before `auv --version`.
That bounded install call and its ordering have local regression coverage.

The fifth gate passed guest installation, pairing, and official Chrome
setup. Action failed before AUV capture because the adapter supplied the
canonical ID to CLI `--device`, whose contract is name selection. The
unchanged desktop's evaluator-only score was `0.0`; no AUV Run or artifact
exists, so this is not a valid capture negative control. Reset again removed
the task-owned resources and preserved the hot PVC/PV. A test-first change
switched the adapter to `--device-id`; the six-phase path was not yet
verified live at that point.

The sixth fresh episode completed the full six-phase path with verified AUV
Run/PNG evidence and raw pinned Chrome evaluator score `0.0`. This closes the
single-task capture-only infrastructure gate. It does not test agent GUI
actions, task-solving performance, a multi-task predeclared batch, V2.1, or
the full upstream provider. Those remain separate next slices.

The following bounded sequence is in progress:

1. Add the pinned V1 VLC `5ac2891a-eacd-4954-b339-98abba077adb`
   setup/getter/metric method chain to the benchmark-local evaluator bridge.
   Assert its no-action score, initial `play-and-exit` state, and that this
   fixed upstream path sends no GUI input through PyAutoGUI. Keep the bridge
   explicitly limited to the reviewed Chrome and VLC tasks.
2. Give the action phase a foreground, benchmark-local client of the existing
   typed `ActionExecutor` and one persistent AUV Runner. Predeclare structured
   actions and terminal policy; record every AUV result, a final screenshot,
   and all Run IDs. Test independent native event receipt and cleanup on
   failure, interrupt, and deadline. This is scripted automation evidence,
   not a blinded agent claim.
3. Predeclare exactly two independent episodes (Chrome and VLC), each with a
   fresh verified overlay, pinned task/assets, separate 10-minute action
   budget, evaluator, and UID-safe reset. First prove the VLC no-action
   evaluator on a fresh guest; then run the fixed two-task AUV-action batch
   without changing its denominator or scripts after seeing an outcome.
   Report phase failures separately from raw scores and preserve the final
   image/PVC identities. Only after that gate should an agent policy be
   frozen and evaluated as a distinct trial.

The first of these slices now has a local pinned-source implementation:
`v1_evaluator.py` allows the reviewed VLC setup/getter/metric chain and checks
the guest config file's opposite initial value before scoring. Boundary tests
reject unreviewed guest commands, misleading HTTP 200 responses, wrong file
paths, and changed upstream/task bytes. The K8s phase adapter now accepts
either fixed task while keeping its action capture-only and UID-safe cleanup
unchanged. The pinned Python suite passed locally. A fresh VLC guest then
completed the six-phase capture-only control with verified Run/PNG and raw
evaluator `0.0`; setup checked the guest's opposite initial config value.
This closes the VLC negative gate but does not substitute for the two-task
AUV-action batch or an agent attempt.

The second slice now has a local foreground `auv-osworld-action --plan`
entry. It validates a predeclared typed action sequence before connecting,
selects an explicit paired or guest-local AUV context, runs the sequence on
one persistent Runner, and records its Run ID, original per-step
`InputActionResult` values, and same-Runner final PNG/SHA256. The sidecar is
written as soon as the Run ID is observed; terminal stdout mirrors it. Local
Rust tests pass, but its live Xorg cancellation/hold-release gates remain
ignored and it is **not wired to the K8s phase adapter**. Thus this is an
implementation milestone, not proof of live GUI-action delivery or OSWorld
task completion. A subsequent independent Chrome guest did complete all six
phases using this entry through the paired-remote topology. It recorded a
successful foreground-system-events click attempt and a same-Runner PNG, but
the typed result has `verified=false` and the pinned evaluator returned
`0.0`; the click was not a solution. Its UID-safe reset preserved the hot
PVC/PV. This closes one paired typed-action delivery gate, not guest-local
entry coverage or a multi-task batch. The next gate is to freeze the two-task
batch scripts and denominator before execution.

A separate fail-closed paired-remote adapter then froze a two-episode
Chrome/VLC batch with SHA-pinned `MOVE_TO → DONE` templates. Both fresh
guests completed all six phases, produced verified AUV Run/PNG/typed delivery
records, and reset without changing the hot PVC/PV. Each raw evaluator score
was `0.0`, deliberately expected from a non-solving pointer move. This
closes the **two-task infrastructure transport** gate only. The remaining
baseline work is to validate real task-directed action scripts and guest-local
entry/cancellation, then evaluate a frozen agent policy on a predeclared
denominator. Do not divide successful infra phases by task count and call it
an OSWorld completion rate.

The first frozen task-directed Chrome replay then reproduced the UI through
the `New folder` dialog and entered `Favorites`, but its final same-Runner
capture still showed Save unconfirmed; the pinned evaluator returned `0.0`
despite five successful-but-`verified=false` input delivery records. The
earlier manual positive path inspected intermediate screenshots, whereas the
new entry runs actions immediately and captures immediately after the final
click. A narrowly bounded post-action settle/observation gate is the next
diagnostic slice; do not label this discrepancy an X11 click failure or
replay the same episode with changed coordinates.

A fresh comparison repeated the byte-identical Chrome action array with
`final_settle_ms=2000`. All delivery attempts and cleanup succeeded, but
the dialog still showed Save after the delay and the evaluator still scored
`0.0`. Waiting only after the full sequence is not enough. The next narrow
capability should let the harness observe a screenshot **between** actions
inside one persistent AUV Run, then decide when to send the next typed input.
Do not infer that a fixed inter-action sleep will solve the race without
observing the relevant dialog state first.

A bounded benchmark-local `--interactive --context` JSONL entry now keeps
typed actions and intermediate AUV captures in one persistent Runner. A fresh
attended Chrome V1 episode used it to inspect the `Favorites` dialog before
Save, click the visible button through AUV, and confirm the folder in the
post-click AUV checkpoint. The pinned evaluator bridge returned raw `1.0`,
and UID-safe reset completed. This closes one same-Runner **attended**
task-directed gate. It does not prove why the static replays failed: adding
observations also added time between actions. The entry is not wired into
`batch_runner.py`, whose action stdin remains closed; no unattended agent
policy or predeclared task-solving denominator has been evaluated. At that
point, guest-local current-head entry/cancellation remained untested. See the
[evidence note](2026-10-05-osworld-kubernetes-x11-evidence.md).

The guest-local current-head V1 gate has now passed on a separate fresh
overlay. Exact Ubuntu 22.04 ELF hashes were verified in the guest; ephemeral
key-only SSH, not the OSWorld setup API, launched the AUV owner-socket daemon
and action tests. A same-Runner action/capture test, EOF cancellation, and
SIGTERM followed by a second Run's input reacquisition passed. A persistent
guest-local Run retained checkpoint/PNG/typed-result sidecars. One initial
SIGTERM test failure was test-only: its 1001-action plan exceeded the 1000
limit and created no Run; `980c1ec9` corrected it before the passing rerun.
All task-owned resources were cleaned without changing retained PVC/PV UIDs.
This closes the current-head **guest-local transport/lifecycle** gate, not a
task evaluator or independent X11 ButtonRelease receiver gate. The next
unclosed boundary is an audited controller for a frozen task-solving batch;
V2.1 asset/setup/evaluator coverage remains separate.

A fail-closed scripted visual controller then passed a **fresh one-task Chrome
V1 batch** on `liet-gpu-1`. The five-action policy and Tesseract build/model
were SHA-pinned; four spatial screenshot gates took seven AUV observations
inside one Run. All six phases completed, the pinned Chrome method-body
evaluator returned raw `1.0`, no failure layers were recorded, and UID-safe
cleanup preserved the hot PVC/PV. This closes one deterministic task-solving
baseline and proves the controller can drive the real six-phase path; it is
not an autonomous-agent rate or a two-task task-solving batch. The next slice
is a separately audited VLC visual policy or another predeclared task with a
stable observed transition, followed by a frozen multi-task denominator.
V2.1 task setup/evaluator and asset coverage remain separate. See the
[evidence note](2026-10-05-osworld-kubernetes-x11-evidence.md) and
[runbook](2026-10-05-osworld-kubernetes-runbook.md).

A subsequent **attended** fresh V1 VLC episode reached Advanced Preferences,
visually identified the `Play and exit` checkbox as already unchecked, and
saved through paired AUV. The pinned evaluator returned raw `1.0` and
UID-safe cleanup preserved the hot disk. Setup had written
`play-and-exit=1` after VLC launched, so the in-memory unchecked default
needed a Save, not a blind toggle. This gives an observed path for a second
policy but does not yet freeze its visual predicates: full-frame OCR missed
one Advanced title and text recognition does not classify the checkbox.
The next narrow slice is to validate an explicit checked/unchecked image
predicate against positive and negative controls, then replay the exact
VLC path through a fail-closed controller on a fresh guest. Only after that
may Chrome and VLC form a predeclared two-task **scripted** denominator.
The immediate fresh ON/OFF-control attempt safely aborted: after `All`,
Advanced appeared in the title but Simple content persisted for more than
seven seconds. It produced no evaluator score or checked-target sample.
Before another automation attempt, diagnose or bound that redraw state with
right-pane evidence; a title-only gate is demonstrably insufficient.
A read-only spatial OCR classifier then distinguished Simple, three archived
stale Advanced frames, full Advanced, and target Playlist frames (6/6) in
about 1.8 seconds. A fresh single-guest AUV loop reproduced two stale cycles
followed by one full Advanced cycle, each about 12 seconds; Escape returned
to VLC main every time. This is a live red/green symptom signal, not a
root-cause diagnosis or proof that retrying will always recover. It can
support a bounded fail-closed policy only after the target checkbox-state
predicate and fresh replay are validated.
An exact-source audit of three alternate V1 task candidates did not yield a
drop-in second AUV-only baseline: terminal
`13584542-872b-42d8-b299-866967b5c3ef` uses PyAutoGUI in setup and
evaluator; VS Code `0512bb38-d531-4acf-9e7e-0add90816068` uses `wmctrl`
window activation, needs an external VSIX, and has an evaluator command-line
shape that must be verified; Writer
`0810415c-bde4-4443-9047-d5f70165a697` uses `wmctrl`/PyAutoGUI and
external document assets. This is a three-task sample, not a claim that no
other V1 task is eligible. Do not silently adapt upstream setup/evaluator
behavior or count an adapted task as unmodified official execution.

The access preflight on 2026-10-06 authenticated `hf` as `nekomeowww`.
Initially the task dataset returned `Access denied. This repository requires
approval.` After the owner approved access, read-only dry-runs at revision
`osworld-v2.1` enumerated 147 task-repository files and 1084 gated asset files.
The 108 release-matched task classes were subsequently downloaded to a
task-owned scratch directory, and every file matched the SHA256 in the pinned
`osworld-v2.1.task_hashes.json` (manifest SHA256
`c54d428329be5ca72742a6becd49ee83cf739df5dea429bba182e1f3f21badfb`).
The 1084-file gated asset snapshot has **not** been downloaded or verified
locally; required assets and task setup/evaluator behavior must still be
checked before official V2.1 task evaluation. See the
[official V2.1 guide](https://github.com/xlang-ai/OSWorld-V2/blob/osworld-v2.1/docs/PUBLIC_EVALUATION_GUIDELINE_v2.1.md#21-download-v2-task-classes-and-assets).
The pinned gated inventory is 1,084 files / 4,920,057,674 bytes; a narrow
follow-up downloaded and hash-verified only Task099's image and Task044's
5,789,382-byte Shotcut video. Task044's reachable setup/evaluator code has
no PyAutoGUI GUI input, CDP, or mocked-site dependency, but Shotcut/codec
behavior in the guest needed live validation. A separate fresh V2.1 guest
preflight has since verified the asset bytes, Shotcut launch and AUV preview,
plus CPU decoding; it did **not** crop/export/evaluate the task or establish
Shotcut export performance. See the
[asset and environment preflight](2026-10-06-osworld-v2-assets-preflight.md).
Broader batch
coverage is the trigger to sync and verify the full pinned asset snapshot.

Task setup and evaluator code need their own input audit before batch execution.
At the pinned V1 revision, the known
[GIMP theme task](https://github.com/xlang-ai/OSWorld/blob/b138d348256078fa634fc3b73567a7337c793e6b/evaluation_examples/examples/gimp/7767eef2-56a3-4cea-8c9f-48c070c7d65b.json)
uses a PyAutoGUI Ctrl+Q in evaluator `postconfig`, while the
[volume task](https://github.com/xlang-ai/OSWorld/blob/b138d348256078fa634fc3b73567a7337c793e6b/evaluation_examples/examples/os/28cc3b7e-b194-4bc9-8353-d04c0f4d56d2.json)
uses a PyAutoGUI click during setup. Running either unmodified would violate
the AUV-only GUI-input rule even if the agent itself uses AUV. The pilot must
either reproduce those preparatory GUI actions through AUV at the same phase
and invoke the release-matched evaluator without the original GUI delivery,
or select tasks whose setup and evaluator are input-free. Record every such
deviation from upstream execution; do not report it as an unmodified official
benchmark score.

Two candidate V1 pilot tasks at the pinned checkout avoid explicit PyAutoGUI
commands in both setup and evaluator:

- [Chrome bookmark-bar folder](https://github.com/xlang-ai/OSWorld/blob/b138d348256078fa634fc3b73567a7337c793e6b/evaluation_examples/examples/chrome/2ad9387a-65d8-4e33-ad5b-7580065a27ca.json):
  setup launches Chrome and `socat`; evaluation restarts Chrome and checks its
  bookmark data for a folder named `Favorites`.
- [VLC play-and-exit setting](https://github.com/xlang-ai/OSWorld/blob/b138d348256078fa634fc3b73567a7337c793e6b/evaluation_examples/examples/vlc/5ac2891a-eacd-4954-b339-98abba077adb.json):
  setup launches VLC and writes `play-and-exit=1`; evaluation reads `vlcrc`
  and expects `0`.

The two candidates were audited and run on 2026-10-06 in separate official V1
overlays. The VLC guest-local shared-socket episode exhausted its 10-minute
action window and the pinned metric returned `0`; the Chrome paired-remote
episode completed inside its window and the pinned metric returned `1.0`
after official postconfig. The denominator is **2**; the arithmetic mean
`0.5` describes only this selected two-task exploratory slice. The runs used
release-matched getter-equivalent file retrieval and exact upstream metric
functions, **not** the full `DesktopEnv.evaluate()` runner. They therefore do
not establish an official benchmark score or general agent completion rate.
See the [evidence note](2026-10-05-osworld-kubernetes-x11-evidence.md) for
task hashes, AUV artifacts, timing, and cleanup. A durable batch scheduler,
full-runner integration, and larger predeclared sample remain pending.

A later Chrome-only evaluator boundary now runs selected pinned upstream
`DesktopEnv.evaluate()`/setup/getter/metric **method bodies** against an
externally managed VM, without constructing the Docker provider or calling
`step()`. Fresh official V1 guest negative and AUV-only positive controls
returned `0.0` and `1.0` respectively. This improves on metric-only
extraction but still is **not** full upstream-module/provider execution or a
general batch runner. The bridge is allowlisted to the Chrome task/hash; VLC
and V2.1 need separate audits. See the evidence note and runbook.

For V2.1, a static audit of all 108 hash-verified task classes selected
`Task099` as the first strict AUV-only pilot. Its setup only downloads
`task_099/my_image.png` to the guest Desktop; its evaluator only retrieves
`position.txt` and scores coordinates. The required image was downloaded from
the pinned gated asset revision into task-owned scratch storage (SHA256
`6c99998e7275e2132c6e27d01f212eaabfbf1c10b0e9405d11aa125eb94a16c1`).
The guest needed Google Maps access, and the later blind agent received
only the task instruction and AUV observations, **not** task source or
evaluator ground truth. `Task100` is a better mouse-heavy second pilot, but
requires a self-hosted SlidePuzzle backend; official setup uses CDP to open
its initial page and its evaluator rejects agent CDP use. A separate Task099
infrastructure negative control later ran exact pinned `setup()`/`evaluate()`
methods through a minimal file-transport adapter on a fresh official V2.1
guest. The image arrived byte-for-byte, Google Maps loaded through AUV input,
and the evaluator returned the expected `0.0` with no answer file. That run
used an older AUV 0.0.27 binary of unproven source identity, not the current
PR head; no agent attempted the task in that negative control. See the
evidence note. A later fresh guest on an exact-current-head AUV 0.0.28 build
supported one blinded AUV-only agent attempt. The agent navigated the image
and Google Maps but was stopped at a disclosed ad-hoc limit before writing
`position.txt`; exact pinned evaluation returned `0.0` because the answer
file was absent. This is an incomplete single attempt, not a rate or a
completed batch. A separate selected V2.1 Task044 Shotcut blind attempt later
produced both output files and returned `0.6000000000000001` through its
exact pinned `Task044.evaluate()` method plus official file getter in a
minimal transport adapter. The crop top was 90 px, outside the metric's
75–85 px range. Its action stop was acknowledged five seconds after the
fixed ten-minute deadline, though the final recorded AUV capture began four
seconds before it; the evaluator environment also used a different OpenCV
package/version than the pinned project. See the
[Task044 episode evidence](2026-10-06-osworld-v2-task044-episode.md). These
two independently selected V2.1 attempts do not form a predeclared batch or
completion-rate denominator. Full upstream runner integration remains pending.

### 5. Agent/harness evaluation — two selected V2.1 attempts, no rate

Once the infrastructure and batch pilot are repeatable, connect an agent that
observes through AUV and selects typed adapter actions. Score it with the
official evaluator on a pinned task set. Keep agent policy results separate
from driver capability evidence. Full-suite or comparative claims require the
matching task assets, mocked sites, reset image, and recorded denominator.

## Operating rules

- Work through these items in order; each item may be delegated to one bounded
  sub-agent at a time. The primary agent checks its diff, evidence, and scope
  before starting the next item.
- Keep runbooks and durable evidence in `docs/ai/references/ops/`; keep raw
  local scratch logs under `docs/notes/<owner>/` unless explicitly approved
  for commit.
- Do not delete retained V1/V2.1 PVCs or the existing paired Xorg fixture as
  incidental cleanup. Clean only task-created disposable Pods, overlays,
  port-forwards, and daemons after resolving their exact identities.
- Update this TODO status only from verified code and live evidence. If an
  item reveals a missing AUV primitive, document the concrete consumer and
  propose the smallest contract slice before expanding the driver API.
