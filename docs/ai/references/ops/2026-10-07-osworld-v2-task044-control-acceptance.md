# OSWorld-V2.1 Task044: live capture-only control

Date: 2026-10-07 local (2026-10-06 17:55–17:58 UTC). This is **one fixed
infrastructure negative control**, not a Codex-agent attempt or a Shotcut
task completion. It is excluded from the predeclared
[two-task agent pilot](2026-10-07-osworld-v2-codex-agent-cohort.md).

The single-episode six-phase manifest and final ledger remain at
`/tmp/auv-v2t044-control-20261007-c1/manifest.json` (SHA256
`e71614fbfec02681baec95c471c8a801d27c89316d348c457a4efe6ead08d91c`)
and `run/ledger.json` (SHA256
`c4617472238ca06acff471fb1af7beca964f9d3961f6d572e16f0e633a8e9456`).
All six phases (`boot`, `install`, `setup`, `action`, `evaluate`, `reset`)
exited 0. The runtime Pod UID was
`d1ee4dcb-8d3b-40f2-afa6-9aeff7455386`, with zero container restarts;
its QEMU command used `-enable-kvm` and fresh `/boot.qcow2` backed by the
pinned V2.1 hot base SHA256
`28b617987f3edf14edd835069cdde6a6e4708be8e4ae4eb957200d5da335186a`.
The installed guest AUV ELF SHA256 was
`327afaf09f11dd5d8926ee5a16f58ac03e96818afe62e165971c046e45f6bd8e`.

Task044 setup uploaded the pinned 5,789,382-byte video, verified its guest
SHA256 by readback, and received the Shotcut launch receipt. The only action
was paired AUV `display.capture`, Run
`b647c932-689b-4cc0-b16c-a068f33a3fbd`; its PNG SHA256 was
`6933e516a25b9570f25f713312204e8dbab6586b2ecf751677c626d628d5bb78`.
Visual inspection of that PNG showed the video icon and the **Shotcut splash
screen still loading plugins**. Thus the launch and capture prove the
application started, not that its editor was ready for immediate input.
No crop, export, or project save was attempted. The unchanged pinned Task044
scorer, under `opencv-python==4.8.1.78`, returned raw float `0.0` for the
missing export; this is an expected task zero, not an evaluator failure.

Reset removed only the task-owned runtime Pod, proxy Pod, and Service using
their recorded UIDs; its report verified the retained V2 hot PVC Bound.
A later read-only namespace check found no Pods or Services and all five
retained PVCs Bound. The adapter verifies Pod/container identities between
phases, but does not separately detect a QEMU process restart inside the
same unchanged container; this remains an evidence limit. The live run used
the adapter as committed at `1975c2c2`; a subsequent regression fix preserves
the original evaluator's diagnostic lines in future phase stdout without
changing its score or guest behavior.
