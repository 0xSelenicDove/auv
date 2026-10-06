# OSWorld-V2.1 two-task Codex-agent pilot: first cohort evidence

Date: 2026-10-07 local. This reports the
[predeclared Task099→Task044 pilot](2026-10-07-osworld-v2-codex-agent-cohort.md)
without replacing either member. It is an **exploratory, prompt-restricted
Codex sub-agent test**, not an official OSWorld-V2.1 pass rate or an enforced
AUV-only model tool sandbox. The denominator stays **2**. Neither task was
fully completed; only Task044 reached the agent action phase.

| Scheduled task | Execution layer | Original evaluator | Full completion |
| --- | --- | --- | --- |
| Task099 | Blocked during install/pairing, before setup or agent action | **No score**; evaluator not run | Not measured for agent |
| Task044 | Agent relay `failed-before-terminal` after 22 actions and 23 AUV captures; no final artifact | Raw float **`0.0`**: exported MP4 missing | No |

Do not interpret the table as “AUV solves 0/2 tasks.” One member has no
agent observation at all. The other has a valid task zero on an incomplete
agent Run. The earlier selected Task099 zero and Task044 partial 0.6 are not
members of this cohort.

## Frozen identity and Task099 infrastructure stop

The two task configs and one-episode manifests were generated before either
cohort guest booted:

| Episode | Config SHA256 | Manifest SHA256 |
| --- | --- | --- |
| `v2agent099-a1` | `8510bcb04a7d8cde26c81e73106be4c80d726a7934d6b13614d9f655988204ac` | `284d48d0056b599284c66aebea499cebd72058175021c1a5ef145e04e2deb788` |
| `v2agent044-a1` | `d7350636ca45eacfe9b0b27b12b97d9a9e292142c585a431ba27152ff80e351a` | `30b98dd258cda4489f10cfa58741729d4c2e434303b3fc89796ed998134fcea4` |

Both used the clean V2.1 revision and task/asset pins in the declaration,
the V2 hot base qcow2 SHA256
`28b617987f3edf14edd835069cdde6a6e4708be8e4ae4eb957200d5da335186a`,
and installed guest AUV ELF SHA256
`327afaf09f11dd5d8926ee5a16f58ac03e96818afe62e165971c046e45f6bd8e`.
Task099 booted a fresh KVM/qcow2 overlay on runtime Pod UID
`80f24ca4-a9d4-4b88-a0a7-7297a34b7c28` (zero container restarts).
Its first install uploaded and hash-checked the guest AUV, then
`devices pair create-token` exited 1: the adapter retained only stderr SHA256
`3a4ad1a7afba61ab14d83c68bf3a9ebcdc830260319d0b76be521d6184f6281c`
and 60 stderr bytes, not the raw text or a bearer token. An immediate install
rerun failed at `/setup/upload` with HTTP 500 after the initial upload;
there is no paired profile or task setup receipt. A read-only diagnostic
found guest port 8080 returned HTTP 401 while the QEMU container had no
listener on its own port 8080, so the Service port-forward refused; it did
**not** establish why owner-socket token creation failed. No agent or
evaluator was run for Task099. The original Pod, proxy, and Service were
removed using recorded UIDs, including runtime UID above. This episode is
an infrastructure block with **no invented score**.

## Task044 blind action and evaluator

Task044 booted a separate fresh overlay on runtime Pod UID
`513dbba1-bfd9-44c7-9e10-7e2237ea892d`, with zero container restarts.
Install/pairing succeeded with the pinned guest AUV; setup read back the
exact 5,789,382-byte video SHA256 and accepted Shotcut launch. A no-history
Codex sub-agent received only the public task instruction, AUV relay command,
and 540-second/single-use-checkpoint rules—not task source, rubric, earlier
results, or evaluator access. The model's other tools were prompt-restricted,
not technically removed.

The agent opened one paired AUV Run
`bfc7ddf49675dc0abadf7a1f407c63a4`. Its durable gateway trace records
**22 typed actions and 23 byte-verified AUV captures**, followed by
`failed-before-terminal` with no `finish` receipt or final PNG. Trace SHA256:
`d3ff5134aab1fce1d20fdd1ef9d45a617b7116a9ba5bf20bc9e294b219e71b2d`;
action sidecar SHA256:
`3f85e8a866eb24c9afdbd618a14aadb0e4e9644a5baabde2e4b02aa1e61c4317`.
The first context file and incomplete action sidecar mtimes span approximately
02:23:21–02:32:21 local, consistent with the 540-second relay cap; the
agent did **not** observe or report a terminal `session_end` line, so the
exact relay error text was not captured. Do not call this a clean stop.

The last AUV checkpoint, `checkpoint-0023.png` (SHA256
`bdcd035d8484818fb5cfecfb3a9ac96c8d412e3c0b34f80f8ca5c445c0993dc3`),
shows Shotcut open with a timeline clip, the export panel on an HEVC preset,
and a `promo_video.mlt` icon on the Desktop. The sub-agent reported applying
a Crop: Source filter and saving the project, but the project bytes were not
retrieved for independent filter inspection. The unchanged pinned Task044
scorer under `opencv-python==4.8.1.78` returned the following observed stdout
in the operator's tool transcript (not a separately hashed raw log file):

```text
[Task044] Exported video promo_video_v1.mp4 not found → score=0.0
{"phase": "evaluate", "result": 0.0, "score": 0.0, ...}
```

The JSON line above is abbreviated in this note; the tool transcript has
the full source/asset identity fields. The official method returned `0.0`
at its missing-export gate and did not inspect the saved project's crop.
The Task044 Pod, proxy, and Service were then removed with UID preconditions.
A final namespace read found no Pods or Services; all five retained PVCs
were Bound. Raw episode files remain under
`/tmp/auv-v2-agent-cohort-20261007-a1/` and are local, not committed.

## Follow-up boundary

Do not silently replay either member under this denominator. A later cohort
must be separately declared before boot, after a reproducible diagnosis of
Task099's owner-socket token failure and an idempotent install path. The
Task044 agent reached project-save/export configuration but needed a terminal
receipt and completed export; more time or different GUI policy may change
its outcome, but this run cannot establish that. Tool-isolated model I/O,
full upstream provider integration, and benchmark-wide claims remain open.
