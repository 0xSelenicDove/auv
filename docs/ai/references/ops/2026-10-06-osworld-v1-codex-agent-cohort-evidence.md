# Selected V1 Codex-agent cohort: two incomplete AUV Runs

Date: 2026-10-06. Evidence level: **exploratory, prompt-restricted agent
attempts on fresh official V1 guests**, not a tool-isolated model benchmark or
an OSWorld-wide completion rate. The two-row membership, order, prompt,
budgets, and denominator were [committed before either VM
boot](2026-10-06-osworld-v1-codex-agent-cohort.md) at
`7df823937b503d312c3712f1599636d97e9eb47d`, declaration SHA256
`4565163ababbf84283020b8b17a9e1adf88347bd5cef4c004a7898e227f6f61f`.
Both rows ran serially on `liet-gpu-1`, each on a fresh V1 qcow2 overlay with
its own Pod/Service/proxy names and paired Device ID. All GUI input/capture
went through installed AUV; the operator used the OSWorld setup/evaluator
plane only for the official non-agent phases. No CUA/CUA_REPL, VNC input,
xdotool, PyAutoGUI input, CDP agent control, or OSWorld execute GUI relay was
used by the operator. The Codex agents' other tools were restricted by prompt
only, not technically removed; their exact model build identity is unknown.
The `/private/tmp` evidence links below identify local operator artifacts and
are not portable PR attachments; the SHA256 values preserve their identity.

| Frozen row | AUV Run and delivered work | Terminal / failure layer | Pinned raw evaluator |
| --- | --- | --- | ---: |
| Chrome Favorites, task `2ad9387a-65d8-4e33-ad5b-7580065a27ca` | `8f91d0de1c74ee1b295ce4fae269ac60`; 4 captures, 3 typed actions | `failed-before-terminal`; 540-second attended relay session deadline expired before proposed seq 8 was forwarded | `0.0` |
| VLC Play and exit, task `5ac2891a-eacd-4954-b339-98abba077adb` | `95f9b62088024d93edd6e0dc422f9475`; 4 captures, 4 typed clicks | `failed-before-terminal`; gateway rejected proposed seq 9 before delivery because a new capture was required after seq 8 | `0.0` |

The declared denominator remains **2**, including both incomplete rows. The
two raw `0.0` values reflect the partial guest states when the pinned V1
evaluator ran; they are not evidence that the agent, X11 driver, or AUV would
fail either task with a complete session. Neither Run emitted a successful
`finish` or final PNG; both terminal AUV sidecars have `final_artifact=null`.
All delivered `InputActionResult` entries reported successful
`foreground_system_events` attempts and `verified=false`, so delivery alone
does not prove semantic success. The operator did not send either agent the
task JSON, evaluator result, prior scripted policy, or other row's images/
trace; prompt-only restriction cannot prove the agent lacked other access.

## Per-row observations

Chrome's pinned task JSON SHA256 was
`4ddb526e5f3b9efa72a01e3ccae86ee4d698f480e4a526f9dfde85fd9499559c`.
The first fresh agent selected `HOTKEY ctrl+shift+b`, `RIGHT_CLICK(260,127)`,
and `CLICK(353,532)` from hash-verified AUV checkpoints. Its fourth PNG
showed the new-folder dialog; the agent then proposed typing `Favorites`,
but the relay had already exited. The context file timestamp was 22:06:57
China time, checkpoint 0004 was 22:14:33, and the terminal gateway trace was
22:15:57: exactly 540 seconds from context creation and only 84 seconds
after the last checkpoint. This supports the total session deadline, not a
180-second post-capture idle. The operator/sub-agent message-and-image
round trips consumed much of the action window; the agent reported sending
the next proposal promptly. The [Chrome gateway trace](/private/tmp/auv-osworld-agent-cohort-1006.ZQUHYj/v1-agent-chrome-favorites/agent_decisions.json)
SHA256 was `ae6214d69e545700539171c7cabf50ad956551bc31a2c4016c1581662f521b14`;
its [AUV terminal sidecar](/private/tmp/auv-osworld-agent-cohort-1006.ZQUHYj/v1-agent-chrome-favorites/action_evidence.json)
SHA256 was `7f3956d9bd581bbfb745cfe7fb79137c8adb64b77e885970999b55bb760d65fa`.
Proposed seq 8 was not recorded as a forwarded receipt.

VLC's pinned task JSON SHA256 was
`4e038a7bb4c3770186209d68402e678ff723238cb31684fe452f0b0c6f4665da`.
The separate fresh agent reached Advanced Preferences, observed the search
field, and clicked it. It next proposed typing `play and exit` against the
same checkpoint. The gateway intentionally consumes checkpoint provenance
after one action and rejected this next action before AUV delivery. The
prompt and operator protocol explanation said actions must cite the latest
screenshot but did **not** state that every action needs a new capture; the
agent reported that this rule was unclear. No preference was changed or
saved. This is a protocol/tool-description failure, not a demonstrated
model-policy or driver failure. The [VLC gateway trace](/private/tmp/auv-osworld-agent-cohort-1006.ZQUHYj/v1-agent-vlc-play-exit/agent_decisions.json)
SHA256 was `4036c0ec29471fc37504745f497d1564b758bbde74bb9f695513600f5cf4979f`;
its [AUV terminal sidecar](/private/tmp/auv-osworld-agent-cohort-1006.ZQUHYj/v1-agent-vlc-play-exit/action_evidence.json)
SHA256 was `83e0f63f2395cd83cc78bc2a36da8f2cce86d4317e583f27e7a16a7d38bce13b`.

## Environment, cleanup, and follow-up

The installed Ubuntu guest AUV was 0.0.28, SHA256
`2a8e53eecfef1dcd8fa8368fa480d6df36e254527c7e60be3ac82802e7073427`
from pinned older source `25e2320570a72d3b9580451ea2917a9e03fa6b95`;
the host action ELF was SHA256
`861fdd93c069b40354f26fd02910d7417cb4a4d0a12a19ad79caa03207297100`.
Each fresh guest initially had `packagekitd` holding the apt lists lock,
preventing the install phase. A temporary `systemctl stop packagekit` in
each disposable guest released it; the same VM then passed the original
install phase. VLC's apt install later exceeded the setup endpoint's
30-second HTTP timeout while apt continued. The operator did not immediately
retry: a process check first found apt had exited, `dpkg-query` verified all
required packages, and `/home/user/auv --version` succeeded; only then was
the idempotent install phase rerun. These are environment/phase-adapter
failures, not agent actions. The full [local operator
log](/private/tmp/auv-osworld-agent-cohort-1006.ZQUHYj/cohort-operator-log.md)
records config hashes, resource UIDs, and both recovery steps.

Both pinned evaluators ran after their failed action processes had stopped.
UID-preconditioned reset removed only each task's runtime/proxy Pods and
Service. An independent Kubernetes postcheck found all six absent. The V1
hot PVC/PV UIDs stayed
`34143535-4ac2-42f2-a443-08db3f6b49ff` /
`8e8e46f1-0670-4a55-b06a-8caebb4d5a73` and V1/V2 hot PVCs remained Bound.

The next attempt needs a separately frozen cohort. First make the relay
usable for model I/O: eliminate unnecessary operator-message latency or
raise a predeclared wall limit within the Rust Run constraints, and make the
one-action-per-checkpoint rule explicit (or deliberately design a typed
compound action with separately reviewed semantics). Do not retroactively
grant this cohort more time or a retry. A formal benchmark additionally
requires an enforced AUV-only model tool boundary and verifiable model
identity; prompt compliance and these two raw evaluator values cannot prove
either.
