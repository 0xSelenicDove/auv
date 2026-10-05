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

The current [evidence](2026-10-05-osworld-kubernetes-x11-evidence.md) proves
one evaluated GIMP task on both official images, plus separate action-level
baselines. It does not prove an OSWorld completion rate. The
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

### 3. Re-run capability gates in both official guests — baseline passed, coverage remains

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

### 4. Batch scheduler and evaluator pilot — pending

Only after action gates pass, run a small, reproducible official-task batch on
fresh overlays. Each episode needs a known image revision, task assets and
mocked services, reset policy, timeout, AUV run IDs, final artifact, and exact
upstream evaluator output. The initial batch should include multiple action
families, not only the existing GIMP setting task. Report pass/fail/blocked
and denominator explicitly; distinguish environment failure, adapter failure,
agent failure, and evaluator failure. Confirm OSWorld-V2 dataset access before
promising a full 108-task run.

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

### 5. Agent/harness evaluation — pending

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
