# OSWorld-V2.1 Task099: exploratory Codex agent attempt

Date: 2026-10-07. The owner selected a current Codex sub-agent for one
exploratory AUV-only blind action phase. This was a **selected single task**,
not a predeclared V2.1 cohort or official agent completion rate. The agent
did not receive the Task099 source, evaluator, ground truth, local image
asset, or earlier captures. The relay restricted accepted proposals to AUV
capture and typed GUI actions, but the Codex process itself was not placed
behind an enforced AUV-only model tool boundary. This is a procedural blind
test, not proof of model tool isolation.

## Fixed environment and action boundary

The fresh task-owned V2.1 QEMU/KVM guest ran on `liet-gpu-1` with runtime Pod
UID `72dbddd0-e7ea-44a6-8412-260735343d60`, proxy UID
`d684ef2a-14dd-40a5-95c5-e17279bee756`, and the same pinned base qcow2
SHA256 `28b617987f3edf14edd835069cdde6a6e4708be8e4ae4eb957200d5da335186a`.
Both Pods were Ready with zero restarts during the attempt. The guest AUV
ELF was the Ubuntu 22.04 build from source `1148f382` (SHA256
`327afaf09f11dd5d8926ee5a16f58ac03e96818afe62e165971c046e45f6bd8e`).
The paired Mac AUV and action binary SHA256 values were
`7d2ffc8545dfe1eb6f12ed0f623a94f34e7246ede8328f54999f43e1ca0c496a`
and `1b8e338380f6d79fd6b7840aff6b154f4810671c9c8e41141f5d7562d193e19e`.
The sealed config SHA256 was
`e366456aaf62eab93d33dfcb966d17071c44715a2c0285c2e006e75f55990eaa`.
The primary agent ran the V2 adapter's boot, install, and pinned Task099
setup before handing over the action phase; setup verified the image bytes
after upload. The Codex sub-agent used `agent_action_relay.py --adapter
v2-task099` with a 540-second session limit, 180-second proposal-idle limit,
and at most 32 AUV actions/32 captures. No OSWorld `/execute` GUI relay,
VNC input, CUA, PyAutoGUI, xdotool, or CDP agent control was authorized.

## Observed result

The attended relay recorded AUV Run
`820e27c1d7f59b5a95e620a50178ead0`, 13 typed actions, and 14 verified
capture receipts. The last checkpoint was `checkpoint-0014.png`, SHA256
`291a743cfdafd6531d64746a7d44ab0fb46516f093d350db8d91c63be4732768`.
The sub-agent reported recognizing the Petronas Twin Towers and using the
guest's Google Maps to inspect the KLCC area, but not confirming the exact
camera location. Independent inspection of the last AUV checkpoint showed
Image Viewer with the task image and Chrome displaying Google Maps Street
View. It does not establish a correct location or completed output file.

The 540-second relay deadline expired before `finish` or `abort`. The
decision trace status is `failed-before-terminal` (SHA256
`e691928185a7b43c8603db9845e30b72e1616c9763c4feb4443fc8d619da6d`);
the action sidecar retained the Run ID but no final artifact (SHA256
`713fecdd9f2e0fdeaee0cfca0ab7412587be178a06f355d722823db95bc7da1d`).
No `position.txt` was saved. After the relay stopped, the primary agent
invoked the unchanged pinned Task099 evaluator on the same Pod UID. It
returned raw `score: 0.0` and distance partial `0.0`. This is an incomplete
attempt with a measured zero, not evidence that AUV could not deliver the
needed GUI actions or that the task was impossible within a different policy.

The primary agent then ran the V2 adapter's UID-safe reset. It removed
runtime Pod UID `72dbddd0-e7ea-44a6-8412-260735343d60`, proxy UID
`d684ef2a-14dd-40a5-95c5-e17279bee756`, and Service UID
`4df7a210-ffad-4175-be3d-fa81cf2a5e20`. A read-only follow-up found no
Pods or Services in `auv-x11-hami-test`; the five retained PVCs were Bound,
including V2 hot PVC UID `540bbdd1-794f-46b5-ad72-2ee553f21ad8`.
Uncommitted raw local traces and checkpoints remain under
`/tmp/auv-v2t099-blind-20261007-d1/v2t099-blind-d1/`.

## Interpretation and next gate

This run validates that an attended Codex agent can maintain a paired AUV
capture/action sequence on the official V2.1 guest and navigate to Google
Maps. It did not produce an answer or terminal Run receipt. The next
benchmark-quality gate remains a predeclared multi-task V2.1 cohort with a
verifiable agent identity, enforced tool boundary, fixed per-task budget,
complete setup/evaluator coverage, and a denominator that includes every
scheduled task. A repeat of Task099 alone must remain labeled exploratory.
