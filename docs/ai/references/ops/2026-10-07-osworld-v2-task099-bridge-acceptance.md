# OSWorld-V2.1 Task099 bridge: live negative-control acceptance

Date: 2026-10-07. This run checked the repository's pinned, file-only
`evals/osworld/v2_task099_evaluator.py` on a fresh official V2.1 QEMU/KVM
guest. It did not install or invoke AUV, deliver GUI input, ask an agent to
solve the task, or contribute to a benchmark completion-rate denominator.

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

## Cleanup and remaining gate

The runtime Pod, proxy Pod, and Service were deleted with their observed UIDs
as Kubernetes preconditions. A read-only follow-up found no Pods or Services
in `auv-x11-hami-test`; `osworld-v1-hot`, `osworld-v2-hot`, and
`osworld-v2-image` PVCs remained Bound. The bridge itself checks an endpoint
string, not a Pod UID, so its `prepare`/`evaluate` marker alone must not be
used as unattended same-guest proof. A future V2.1 action episode still needs
an installed, source-pinned AUV binary, AUV Runs and artifacts, UID-pinned
guest identity, an independent action budget, evaluator output, and reset.
