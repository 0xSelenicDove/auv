# OSWorld agent model I/O: proposed AUV-only adapter slice

Date: 2026-10-06. Status: **design/TODO, not implemented or live-validated**.
Scope: one model-driven action phase on an already prepared V1/V2.1 episode,
using either existing paired-remote or guest-local AUV context. This note does
not authorize an API purchase, a new scored cohort, or a Kubernetes mutation.

## Why this slice

The [frozen V1 Codex cohort](2026-10-06-osworld-v1-codex-agent-cohort-evidence.md)
had two incomplete Runs: Chrome exhausted the attended relay's 540-second
session window amid operator/model message transfers; VLC's agent proposed a
second action against an already consumed screenshot. The relay now advertises
the single-use rule, but it still depends on an operator to transfer every
proposal and image. It cannot prove the Codex sub-agent had only AUV tools or
identify its exact model build.

Existing code gives the connector a narrow seam:

- `agent_action_gateway.py` accepts sequential `capture`, `action`, `finish`,
  and `abort` proposals, verifies PNG bytes/index and AUV sidecars, and refuses
  uncertain forwards. It has no model or tool policy.
- `agent_action_transport.py` owns exactly one foreground Rust child and one
  request/response exchange at a time. It never restarts a child after an
  ambiguous exchange.
- `agent_action_relay.py` handles reviewed context/preflight for paired and
  guest-local topologies and advertises 180-second proposal-idle and
  540-second total-session limits. Its stdin is attended JSONL.
- `batch_runner.py` starts an action phase with `stdin=DEVNULL`; the attended
  relay is therefore not an unattended action command. The runner's process
  group and reset evidence are useful, but it does not establish model tool
  isolation or K8s guest quiescence.

The proposed adapter should own model session events and call correlation,
then invoke the *same* gateway/transport path. It should not add GUI-control
code, expose OSWorld `/execute` to the model, or use a model-hosted computer-use
tool. Rust remains the authority for typed action parameter semantics and
actual AUV delivery.

## Candidate provider contract and its limits

The [Agents API architecture](https://developers.openai.com/api/docs/guides/agents-api/architecture)
documents `environment.type: "none"`: no built-in Bash/apply-patch, workspace
files, or executor MCP, while the application receives and handles configured
function calls. The [function guide](https://developers.openai.com/api/docs/guides/agents-api/tools/functions)
documents function definitions in `agent.tools`, pending calls in
`session.required_actions`, and result submission with the original `turn_id`
and `call_id`. It says a function result may be a supported content array and
requires durable results keyed by session/turn/call for side-effect recovery.
The [turn-item reference](https://developers.openai.com/api/reference/resources/beta/subresources/agents/subresources/sessions/subresources/turns/subresources/items/methods/list)
describes function-call output as a string or `InputContent[]`; an
`input_image` may carry a base64 data URL. The function guide warns that a
tool result must be below 4 MiB including room for metadata.

These are provider documentation facts, **not** evidence that this repository
has used the API. The exact SDK object shape, image acceptance in a live tool
result, model identifier returned by the service, turn completion behavior,
and any ability to attest an immutable model build need a credentialed
contract probe. A configured model name alone pins a requested model ID, not
necessarily its underlying build. Do not label an attempt “tool-isolated” or
“exact-build-pinned” until the session's effective environment/tool list and
provider-returned identity can be inspected and retained. The current Codex
sub-agent cannot be retroactively treated as such a session.

## One proposed function surface

Create one fresh model session per fresh task episode with
`environment.type: "none"`, no remote MCP, plugins, web/search, hosted computer
use, shell, files, or extra functions. The agent instructions contain only the
official task instruction and a pinned, versioned AUV protocol prompt; no
task JSON, evaluator, or setup channels are passed to the model. A session
override must **replace** the whole tool list with exactly the four functions
below; the adapter records the effective agent/session configuration before
any GUI call. The [configuration guide](https://developers.openai.com/api/docs/guides/agents-api/configuration)
states that session-supplied objects/arrays replace saved values and that
changing tools requires a new session.

The proposed JSON Schemas are intentionally small. The connector adds `seq`
itself. `auv_action.action_json` must parse to one JSON object; the gateway
rejects a non-typed or forbidden action and Rust `parse_action` remains the
single parameter validator. The connector must not use the schema as its sole
security check. `based_on` is copied from a prior capture result, not filled
from hidden current state on the model's behalf.

```json
[
  {"type":"function","name":"auv_capture","description":"Capture one AUV screenshot. Each verified action consumes one capture; capture again before another action.","parameters":{"type":"object","properties":{},"required":[],"additionalProperties":false}},
  {"type":"function","name":"auv_action","description":"Send exactly one typed GUI action through AUV, based on the latest AUV capture. No shell, evaluator, setup, or direct OSWorld control.","parameters":{"type":"object","properties":{"action_json":{"type":"string","description":"One JSON object using the existing auv-osworld-action typed action schema."},"based_on":{"type":"object","properties":{"run_id":{"type":"string"},"path":{"type":"string"},"sha256":{"type":"string"}},"required":["run_id","path","sha256"],"additionalProperties":false}},"required":["action_json","based_on"],"additionalProperties":false}},
  {"type":"function","name":"auv_finish","description":"Finish this AUV Run and capture its final screenshot; this does not claim task success.","parameters":{"type":"object","properties":{},"required":[],"additionalProperties":false}},
  {"type":"function","name":"auv_abort","description":"Cancel this AUV Run when the task cannot be continued.","parameters":{"type":"object","properties":{},"required":[],"additionalProperties":false}}
]
```

Only one pending function call may execute at a time. A multi-call or unknown
function batch is a protocol failure; do not reorder calls to make it work.
`auv_capture` maps to gateway `{op:"capture",seq}` and returns a content
array containing `input_text` with `run_id`, checkpoint filename, SHA256,
sequence, delivery rules, and remaining budgets, followed by `input_image`
with `data:image/png;base64,...` from those exact hash-verified PNG bytes.
Do not provide a local path alone: a model without files cannot open it.
`auv_action` maps to `{op:"action",seq,action,based_on}` and returns a text
receipt with the gateway/Rust delivery outcome and an explicit reminder that
another action needs a fresh capture. `auv_finish`/`auv_abort` map to their
terminal gateway operations. A normal agent prose answer is not a successful
action phase unless the gateway has a verified `finish` terminal receipt.

Before sending an image, compute the complete serialized result size. If it
would approach the provider's 4 MiB limit, **fail the action phase without
silently resizing or swapping the image**: a transformed image would have a
different digest and needs an explicit, reviewed observation contract. The
original PNG, checkpoint index, gateway trace, and AUV sidecar remain in the
episode directory. Tool output need not duplicate the base64 in the durable
ledger; record its SHA256/byte length and the PNG path/digest instead.

## Identity, ordering, and failure evidence

Before the model receives the task, write an episode-local, append-only model
I/O record with: adapter source revision and binary digests; protocol prompt
SHA256; requested agent/model/reasoning/settings; effective session settings
as returned by the provider; session ID; OSWorld task/revision/asset hashes;
topology/Device ID; and AUV Run ID once `ready` arrives. Redact credentials,
authorization headers, and paired profile bearer tokens. The model provider
may not expose an immutable build identifier; record that as `unknown`, not
as an inferred hash.

For each pending function, durably record `(session_id, turn_id, call_id,
function_name, arguments_sha256, proposed_seq)` **before** gateway submission.
After it returns, record gateway receipt, AUV Run ID, checkpoint/action
evidence digests, and exact tool-result digest before sending that result to
the provider. A repeated pending `call_id` returns the saved result only;
never call `gateway.submit` again. Resume after disconnect by fetching
`required_actions`, not by replaying arbitrary historical function-call
items. If gateway submission might have occurred but no validated result was
saved, close the episode as ambiguous and do not repeat GUI input. This
follows both the gateway's existing `pending` rule and the provider's
[side-effect recovery advice](https://developers.openai.com/api/docs/guides/agents-api/tools/functions).

The adapter has one absolute action deadline no later than the existing
Rust 570-second hard limit; the present 540-second relay budget is the initial
candidate, not a promise that direct I/O makes all tasks fit. Retain the
180-second no-proposal bound, transport's 20-second per-exchange response
bound, 32-action/32-capture ceiling, and provider request deadlines within
the remaining wall time. Provider throttling, unavailable API, oversized
image, unexpected tool, duplicate call with changed arguments, session
settings drift, unfinished turn, or terminal AUV mismatch must yield an
explicit failure layer. Only a verified gateway `finish` permits the action
phase to exit zero. An agent failure must not skip the official evaluator or
UID-safe reset once the action process and guest input are known stopped.

## Ordered TODO and acceptance gates

1. **Credential/contract gate (no live call yet):** obtain owner approval for
   separately billed API use and an appropriately scoped credential. Probe a
   no-guest function call with `environment.type:none`; retain the returned
   effective model/environment/tools, `required_actions`, image-content
   acceptance, size behavior, and result replay evidence. If the provider
   cannot expose an immutable build, state the weaker model-ID claim.
2. **Offline adapter slice:** implement one benchmark-local direct model-I/O
   entry that reuses `AgentActionGateway` and `ForegroundActionTransport`,
   plus existing topology preflight. No new GUI backend or duplicate action
   parser. Inject a fake provider for tests of schema allowlist, screenshot
   bytes/digest, single-use capture, ordered calls, `call_id` replay, crash
   between gateway and result, 4 MiB refusal, deadlines, and terminal truth.
   The adapter should be runnable as a foreground action command with no
   stdin so `batch_runner.py` can supervise it; batch manifest and K8s UID
   ownership need their own review before unattended remote claims.
3. **Fresh non-solving live control:** after approval, run one fresh V1 guest
   `capture → action → capture → finish`, inspect provider session items,
   gateway trace, Rust sidecars, evaluator output, and UID-safe cleanup.
   Repeat for guest-local shared socket separately. These controls establish
   transport/evidence, not an OSWorld task score.
4. **Only then freeze a new blind cohort** before boot: task list/order,
   prompt/model ID/settings, tools/environment, budgets, image policy, hashes,
   denominator, failure accounting, and both topologies. Never revise the
   already frozen Chrome→VLC cohort or turn its two incomplete raw `0.0`
   evaluator values into an AUV or agent success rate.

The tool allowlist constrains the *configured provider session*. It does not
by itself prove there is no model-side hidden tool, provider behavior change,
out-of-band operator access, or guest setup leak. The accepted evidence level
must say exactly which of those were observed, prohibited by configuration,
or unverified.
