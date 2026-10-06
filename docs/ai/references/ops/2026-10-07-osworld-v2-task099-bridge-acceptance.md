# OSWorld-V2.1 Task099 bridge: live negative-control acceptance

Date: 2026-10-07. The first run checked the repository's pinned, file-only
`evals/osworld/v2_task099_evaluator.py` on a fresh official V2.1 QEMU/KVM
guest. A second fresh run added a source-pinned installed AUV capture before
the same no-answer evaluation. Neither run delivered GUI input, asked an agent
to solve the task, or contributed to a benchmark completion-rate denominator.

## Identity and guest isolation

The bridge was from PR head `59a82e67` (file SHA256
`d342a90c1d974776f9ed35bb1799fe04937282b5ce7ed5601444375a46a14ede`).
Its clean upstream checkout was `3d778a3c9a34a079316f70df023b166700445792`;
the Task099 class, original `get_vm_file` source, and gated image SHA256 were,
respectively, `58c460fdfecf518f64714fdc21933b60818d8cf28ec02f9fa15a10e56ef02e32`,
`4d827d235170ee05a629a483aa972fbc341e994258977816706e36f6c37843ce`,
and `6c99998e7275e2132c6e27d01f212eaabfbf1c10b0e9405d11aa125eb94a16c1`
(1,851,281 bytes). These matched the bridge's pins before guest access.

The task-owned runtime Pod `auv-v2t099-20261007-a1` had UID
`698b166c-5a5a-44f4-a495-1c69c94c37af`, zero restarts, ran on
`liet-gpu-1`, and resolved to runtime image
`happysixd/osworld-docker@sha256:0e6497a9295647cf05bf2b2af522fdd79bdeba2737595259cab310a3bcf6baa9`.
`qemu-img info -U --output=json /boot.qcow2` reported a writable qcow2 with
`/System.qcow2` as its full backing filename; QEMU arguments included
`-enable-kvm` and `-hda /boot.qcow2`. The retained V2 hot base measured
SHA256 `28b617987f3edf14edd835069cdde6a6e4708be8e4ae4eb957200d5da335186a`.
The proxy Pod and Service UIDs were
`78ac1c25-b237-4191-85a3-18cfc2f3b35d` and
`565881c5-1edf-4c80-a7fe-732ff473fc38`.

## Setup and raw evaluation

The local Service-backed port-forward used loopback endpoint
`http://127.0.0.1:15099`. Both commands below exited 0:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 evals/osworld/v2_task099_evaluator.py prepare \
  --upstream /tmp/auv-osworld-batch-20261005/v2 \
  --task-source /tmp/auv-osworld-batch-20261005/v2-task-classes/task_099.py \
  --asset /tmp/auv-osworld-batch-20261005/v2-assets-pilot/task_099/my_image.png \
  --episode-dir /tmp/auv-v2t099-live-20261007-a1 \
  --endpoint http://127.0.0.1:15099

PYTHONDONTWRITEBYTECODE=1 python3 evals/osworld/v2_task099_evaluator.py evaluate \
  --upstream /tmp/auv-osworld-batch-20261005/v2 \
  --task-source /tmp/auv-osworld-batch-20261005/v2-task-classes/task_099.py \
  --asset /tmp/auv-osworld-batch-20261005/v2-assets-pilot/task_099/my_image.png \
  --episode-dir /tmp/auv-v2t099-live-20261007-a1 \
  --endpoint http://127.0.0.1:15099
```

`prepare` invoked the pinned Task099 setup and reported
`"setup":"image_sha256_verified"` after reading back the exact image bytes
from the guest. The pinned Task099 evaluator and original getter then returned:

```json
{"partial_scores":{"distance":{"description":"Geo-localization accuracy","score":0.0,"weight":1.0}},"score":0.0}
```

A separate `/file` request for `/home/user/Desktop/position.txt` returned
HTTP 404. The upstream getter also logged that the file was missing. Thus the
raw `0.0` is the expected no-answer negative control, not a transport error or
an agent result. The bridge's `prepared.json` marker remains in the local
episode directory (SHA256
`6674bfebb93a0d72571fd1d63f9b09a387d7f3d0f22354829b52c67f6f4120c8`).
Command output was observed in the task transcript; no separate stdout file
or AUV screenshot artifact was retained.

## First guest cleanup

The runtime Pod, proxy Pod, and Service were deleted with their observed UIDs
as Kubernetes preconditions. A read-only follow-up found no Pods or Services
in `auv-x11-hami-test`; `osworld-v1-hot`, `osworld-v2-hot`, and
`osworld-v2-image` PVCs remained Bound. The bridge itself checks an endpoint
string, not a Pod UID, so its `prepare`/`evaluate` marker alone must not be
used as unattended same-guest proof. This first run lacked an installed AUV;
the separate current-head capture run below addresses only that observation
gap, not task-directed input or batch scheduling.

## Second fresh guest: current-head AUV capture and evaluator

A separate clean PR HEAD `1148f382ab441605dc42c722073c8e186147fd24`
was archived (SHA256
`d8d8c9e459ad48da05b7d812658255eb6f129cc77f3bdb1ea976e128d5325eeb`)
and verified in a task-owned Ubuntu 22.04 build Pod/PVC on `liet-gpu-1`.
Rust 1.95.0 built `auv-cli`'s `auv` binary with `--release --locked`; Pod,
local copy, and guest agreed on ELF SHA256
`327afaf09f11dd5d8926ee5a16f58ac03e96818afe62e165971c046e45f6bd8e`.
It reported AUV 0.0.28, no missing `ldd` libraries after the guest's bounded
runtime-package installation, and maximum `GLIBC_2.35`, matching guest glibc
2.35. The newer PipeWire development headers were used only in the isolated
build as described in the [runbook](2026-10-05-osworld-kubernetes-runbook.md);
this is not a distributable ABI-policy claim.

The fresh runtime Pod `auv-v2t099-20261007-b1` had UID
`c04dba49-943e-4430-b6ce-fd2c9530f554`, zero restarts, and the same
digest-pinned QEMU image and read-only base qcow2 SHA256 as the first run.
Its own `/boot.qcow2` writable overlay used that base, with KVM enabled.
Pinned Task099 `prepare` exited 0 and verified the guest image by readback.
The installed AUV owner Unix socket then produced:

| AUV operation | Run ID | Evidence |
| --- | --- | --- |
| `display.list` | `01a1120d-f878-71d1-8435-f0e921fc5a70` | Completed; X11 desktop reported |
| `display.capture` | `01a1120e-0f5e-73c1-9ecb-e32490e69983` | Completed through `xcap.x11`; artifact `01a1120e-0fb3-726b-b711-3c47f65f2bbf`, 1920×1080 PNG SHA256 `a3e24facf25beaedc448f8fef151db4805ce837e6e39a0a86cf1c55bfb88b290` |

The PNG was copied to `/tmp/auv-v2t099-20261007-b1-capture.png`; guest
artifact metadata, guest SHA256, and local SHA256 agreed. Independent visual
inspection showed the Ubuntu desktop and `my_image.png` icon. Local AUV Run
records are at `/tmp/auv-v2t099-live-20261007-b1/records.jsonl` (SHA256
`498db7ab446d586f781660fcee5dd3eef9a0d9e70577c1ab5beb6522032c51b2`);
the second episode marker SHA256 is
`b65eda31f4a0ccb40c15ddb6e7ffd064b9765579c3c1b3a5886d4c696f453118`.
These local files are retained evidence, not committed test fixtures.

On that same Pod UID, the committed bridge's pinned upstream Task099
`evaluate` exited 0 and returned raw `score: 0.0`, with distance partial
`score: 0.0`, `weight: 1.0`; upstream logged the missing `position.txt`.
No agent wrote an answer, and no GUI input was delivered. AUV capture proves
pixels were observable through the installed current-head X11 backend; it
does not prove task-solving, other action families, or an agent rate.

The daemon and port-forward were stopped. Runtime Pod, proxy Pod, Service,
build Pod, and build PVC were removed with observed UID preconditions. The
build PV's `Delete` reclaim policy completed after its CSI finalizer. A
read-only follow-up found no Pods or Services in the namespace and confirmed
the retained V1/V2 image and hot PVCs plus workspace PVC still Bound with
their original PV identities. The next gate is a task-directed AUV action
episode using this pinned V2.1 setup/evaluator bridge, followed by a
predeclared multi-task V2.1 cohort; this capture-only `0.0` is not part of
either denominator.
