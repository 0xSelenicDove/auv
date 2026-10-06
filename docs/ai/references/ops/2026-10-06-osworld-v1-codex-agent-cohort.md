# OSWorld V1 exploratory Codex-agent cohort declaration

Status: **predeclared, not executed**. Freeze this file in the draft PR before
booting either cohort VM. This is an attended, selected two-task exploration of
the AUV agent path. It is not an official OSWorld completion rate, a model
comparison, or evidence of a tool-isolated agent.

Cohort ID: `osw-v1-codex-agent-chrome-vlc-1006`. Both episode configs use this
exact batch/cohort ID; each gets a different episode ID from the table below.

## Fixed membership and order

| Order / episode | Official V1 task | Task JSON SHA256 |
| --- | --- | --- |
| 1 / `v1-agent-chrome-favorites` | Chrome `2ad9387a-65d8-4e33-ad5b-7580065a27ca` — create `Favorites` bookmark folder | `4ddb526e5f3b9efa72a01e3ccae86ee4d698f480e4a526f9dfde85fd9499559c` |
| 2 / `v1-agent-vlc-play-exit` | VLC `5ac2891a-eacd-4954-b339-98abba077adb` — disable `play-and-exit` | `4e038a7bb4c3770186209d68402e678ff723238cb31684fe452f0b0c6f4665da` |

Both task files and their evaluator methods come from OSWorld V1 revision
`b138d348256078fa634fc3b73567a7337c793e6b`. The allowlist and hashes
are also pinned independently in `evals/osworld/k8s_phase_adapter.py` and
`evals/osworld/v1_evaluator.py`; their config preflight must pass before each
boot. Both episodes use **paired-remote** control, one fresh writable qcow2
overlay and one newly paired Device ID per task, with AUV as the only desktop
input/capture route. The read-only base image, runtime image, AUV source and
binary, and action binary digests must be recorded from each reviewed config
and actual install receipt before the first action. Never silently substitute
a different task, image, or binary after a failure.

## Same agent policy for both tasks

- Use a fresh, no-history Codex sub-agent for **each** task, with the same
  configured model/effort and the same instruction template. Do not carry
  screenshots, solution steps, evaluator feedback, or reasoning from task 1
  into task 2. Record the task/thread ID and the displayed model/effort if
  available; the underlying model build/version cannot presently be verified.
- Show each sub-agent only its official task instruction and AUV checkpoint
  images/receipts from its own Run. Do not disclose task JSON internals,
  evaluator implementation, prior successful scripts, or the other episode's
  trace. The operator may inspect these only outside the agent context for
  setup/evaluation auditing.
- Instruct the agent to propose only the ordered JSONL operations accepted by
  `agent_action_gateway.py`: first `capture`, then typed `action` grounded in
  the latest hash-verified AUV checkpoint, more `capture` as needed, and
  terminal `finish` or `abort`. The operator forwards proposals unchanged via
  `agent_action_relay.py`; no task-specific action substitution. No CUA,
  CUA_REPL, VNC input, PyAutoGUI, xdotool, CDP, direct X11 input, or OSWorld
  `/execute`/`/setup/execute` as a GUI relay. Setup/evaluator are separate
  non-GUI phases.
- Per episode, declare `--max-actions 32 --max-captures 32`. The action phase
  has an operator-enforced 600-second wall limit from relay launch. The relay
  independently limits the proposal window to 540 seconds after `ready`,
  proposal idle to 180 seconds, and Rust limits its one Run to 570 seconds. Boot, install,
  setup, evaluate, and reset have their own reviewed phase deadlines and do
  not extend the action window. Stop at the first terminal event or deadline;
  do not grant extra actions/time after seeing a partial result.

Use this same prompt template for both fresh agents, replacing only the
bracketed official instruction and per-episode AUV receipt/image content:

> You are attempting one OSWorld V1 desktop task: `[official task instruction]`.
> Your only permitted desktop observation and input is the installed AUV
> session exposed through the operator's JSONL relay. Propose the first
> `capture`, then choose each typed `action` from the latest AUV screenshot;
> cite that screenshot's Run ID, filename, and SHA256 as `based_on`. You may
> request another `capture`, then end with `finish` or `abort`. At most 32
> actions and 32 captures are available, and the action window is at most
> 600 seconds. Do not use any other tool to inspect or manipulate the guest,
> including CUA/CUA_REPL, VNC, browser automation, shell GUI tools, CDP, or
> OSWorld execution endpoints. Do not ask for evaluator or task-file details.

The operator must relay actual gateway responses, not describe an action as
successful from visual guesswork. The template constrains requested behavior
but does not technically revoke any Codex tool.

These restrictions are currently **prompt-only** for Codex: the sub-agent's
other tools are not technically removed, and its exact model version is not
pinned. Operator observation of a compliant trace does not prove absence of
other tool use. Report those two limitations with every cohort result. A
separate tool-restricted model connector and verifiable model identity are
required before a formal agent benchmark claim.

## Denominator, evidence, and execution gate

The declared denominator is **2**, fixed before the first VM boot. Preserve
both rows even if boot, install, setup, action, evaluation, or cleanup fails;
an unattempted second task remains scheduled/not-run and still occupies one
row. Do not replace a failed row with a retry. A retry is a separately named
cohort with a new freeze, not a correction to this denominator. Keep raw
evaluator scores only when the pinned evaluator ran; missing scores are
`not_evaluated`, never invented `0.0`. Distinguish setup, infrastructure,
relay, agent action, evaluator, and cleanup failures. Report each raw score
and execution status, plus a descriptive selected-task count of exact
evaluator `1.0` results out of 2; never label that fraction an OSWorld-wide
or tool-isolated agent rate.

Before boot, commit this declaration, record its commit and SHA256, and
prepare two distinct episode configs with one shared cohort ID, unique
DNS-safe Kubernetes names and local ports. Audit each config against the task
table and archive its SHA256. For each episode in order: boot → install/pair →
official setup → one attended relay session → pinned evaluation (when safe
after a stopped action) → UID-safe reset. Save the phase UTC times/statuses,
Pod/Service UIDs, overlay/base-image identity, actual AUV/action binary
digests, paired Device ID, agent task/model display, relay `agent_decisions`,
AUV Run ID and sidecars, checkpoint PNG hashes, raw evaluator output, and
cleanup report. A failure to verify remote quiescence or ownership blocks the
next boot; it does not remove either denominator row.

`batch_runner.py` already defines the six-phase ledger and denominator model,
but it starts action commands with `stdin=DEVNULL`. The interactive relay
requires a live proposal stream, so **this document is not a runnable
`batch_runner.py` manifest**. Until a reviewed foreground action-phase model
adapter exists, keep an attended two-row cohort record using the same
scheduled/phase/failure-layer distinctions and do not claim an unattended
batch ledger. The [runbook](2026-10-05-osworld-kubernetes-runbook.md) covers
the paired relay and UID-safe phase steps. The earlier selected Chrome and
VLC agent canaries and the scripted 2/2 batch are historical evidence, not
members of this new cohort.
