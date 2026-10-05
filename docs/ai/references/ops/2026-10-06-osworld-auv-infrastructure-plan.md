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
  direct-Xorg receiver test verified the fix. These repairs have not yet been
  rerun in the official V1/V2.1 guest images.
- Current `main` adds a shared scroll contract: positive `delta_y` moves the
  viewport down and deltas are logical pixels, plus window-targeted
  `input.scroll`/`ScrollWindowPoint`. See
  `docs/ai/references/driver/2026-10-06-scroll-delta-contract.md` at `main`.
  X11 screen-point scrolling must keep that contract and be tested against an
  observed scroll position. A wheel event alone is weaker evidence.
- Current `main` changes listener authentication and pairing. `listen` is a
  `serve --listen URI` option, not a separate command. The next paired test
  must use the new registration/authentication flow instead of relying on a
  daemon created from the older PR head.
- A read-only merge-tree check between PR #233 and current `main` reports two
  content conflicts (`crates/auv-driver/Cargo.toml` and
  `proto/auv/api/driver/v1/input.proto`). This is a forecast, not a completed
  merge or validation.

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
repeat it. The PR disclosure and current guest revalidation are separate
follow-ups.

### 2. Build a typed OSWorld action adapter — pending

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
contract is logical pixels. Measure the mapping with a receiver before fixing
a step-to-pixel factor; sign conversion alone is not sufficient.

### 3. Re-run capability gates in both official guests — pending

Build an Ubuntu 22.04-compatible AUV from the aligned branch and install it
into fresh V1 and V2.1 overlays. In each image, test the typed action adapter
against an independent receiver: screenshot, all mouse buttons and counts,
move/drag, both scroll axes with measured scroll position, ASCII/Unicode
typing, ordinary/special keys including F13, overlapping modifier holds, and
cleanup after failure. Record binary/image revisions, run IDs, receiver logs,
and screenshot digests. Run both guest-local shared-socket and paired remote
topologies with the current `serve --listen` authentication flow. A returned
delivery result is not enough without receiver or semantic observation.

### 4. Batch scheduler and evaluator pilot — pending

Only after action gates pass, run a small, reproducible official-task batch on
fresh overlays. Each episode needs a known image revision, task assets and
mocked services, reset policy, timeout, AUV run IDs, final artifact, and exact
upstream evaluator output. The initial batch should include multiple action
families, not only the existing GIMP setting task. Report pass/fail/blocked
and denominator explicitly; distinguish environment failure, adapter failure,
agent failure, and evaluator failure. Confirm OSWorld-V2 dataset access before
promising a full 108-task run.

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
