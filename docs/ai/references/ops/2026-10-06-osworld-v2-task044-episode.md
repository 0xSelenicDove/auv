# OSWorld-V2.1 Task044 bounded AUV attempt

Date: 2026-10-06 UTC. This is **one exploratory, blinded AUV-only agent
attempt**, not a representative OSWorld-V2.1 completion rate or a full
upstream-runner score. The agent received the task instruction and AUV
connection information, not Task044 source or evaluator rules. Infrastructure
setup and evaluation were handled separately.

## Pinned identity and environment

- V2.1 source commit: `3d778a3c9a34a079316f70df023b166700445792`;
  Task044 class SHA256:
  `3ff702eb197aff0e4987537c7a98c40f50100f8c416371c9f0d424c6a3a860ad`.
- Gated `task_044/promo_video.mp4` LFS/local/guest SHA256:
  `987ee02e31537ad83cbe3da15502366d88be944e8b8cec08538adf51185d5108`
  (5,789,382 bytes). See the [asset preflight](2026-10-06-osworld-v2-assets-preflight.md).
- Official QEMU runtime image digest:
  `sha256:0e6497a9295647cf05bf2b2af522fdd79bdeba2737595259cab310a3bcf6baa9`;
  fresh read-only-base V2.1 overlay on `liet-gpu-1`, namespace
  `auv-x11-hami-test`, Pod/Service `osworld-v2-task044-episode`, Pod UID
  `be648237-a384-4e7f-815d-52d2940f9a8b`.
- Guest-installed AUV 0.0.28 source `25e2320570a72d3b9580451ea2917a9e03fa6b95`,
  binary SHA256
  `2a8e53eecfef1dcd8fa8368fa480d6df36e254527c7e60be3ac82802e7073427`;
  paired Mac binary SHA256
  `cf9485c4a2ec0fbf14c0fa6f874ef77decba00f8c61ae704ec67b77915a3c08a`,
  Device ID
  `9642e86951c75e04411e232c1d6ac994d64cc0c6daafecf868a796b7ebae79ca`.

The pinned `Task044.setup()` ran through a constrained file/launch transport:
upload returned HTTP 200 and `File Uploaded: 5789382 bytes`; Shotcut launch
returned HTTP 200 and `shotcut launched successfully`. Guest file hash and
Shotcut PID were checked. The setup transport did not inject GUI input.

## Blinded action and result

The action window began with the first AUV capture at **00:43:31 UTC**, Run
`2b69c3ec-b496-8eda-e060-5d254bc2d442`, PNG SHA256
`67e7a74d81699166ce3291827baaf170af514ef5c8e13e3059e3f5a0d5dd7761`.
The fixed ten-minute deadline was **00:53:31 UTC**. The agent used only paired
AUV GUI observations/input to edit in Shotcut and reported saving
`/home/user/Desktop/promo_video.mlt` and exporting
`/home/user/Desktop/promo_video_v1.mp4`. The final AUV capture Run
`a022c257-23bb-9ea6-e842-6c53d2d2f1a0`, PNG SHA256
`e161c4bd9d124f67cab61771ecda97f31ee2fcc26ad2ec66e1e6dc34969dca33`,
shows the saved project, completed export jobs, and 834×1112 video properties.
Its local AUV lifecycle record is timestamped **00:53:27 UTC**, before the
deadline; no later AUV event appears in the local records through 00:53:36.
The agent's stop confirmation arrived at **00:53:36**, five seconds after the
deadline. Record this acknowledgment lag; do not silently claim an
unambiguously on-deadline stop. No GUI operation was reported after stopping.

At **00:54:26–00:54:28 UTC**, the fixed upstream `Task044.evaluate()` method
ran with the unchanged official `get_vm_file` getter through a minimal HTTP
file-transport adapter, not the full upstream provider/runner. It exited 0
and emitted raw score **`0.6000000000000001`** (reported as 0.6). The raw log
said `Crop top=90.0 outside acceptable range [75, 85]`; both output files
were found. The selected crop top was 90 px, so this was a **partial-score
attempt**, not a completed task. The agent's GUI reported export size below
1 MiB and unchanged resolution; evaluator file retrieval confirmed the
export was 964,508 bytes and the project was 7,282 bytes.

| Retained task-local evidence | SHA256 |
| --- | --- |
| `/tmp/auv-task044-eval-20261006.log` (raw stdout/stderr) | `4c3066bf1454a8650f719fc93978ef1f4e012ddb9c2b9782dddc3939536b0ffa` |
| `/tmp/auv-task044-eval-20261006.json` (receipt) | `3d4140aa05e9ed873f7b6985f02ff5fc65883cf1723926613c86f0873ab62d27` |
| `/tmp/auv-task044-eval-20261006-promo_video_v1.mp4` | `229e334cb7e169aec8bb24c0d9dbd6fc24853fd47687a7333c21048ae52bcc27` |
| `/tmp/auv-task044-eval-20261006-promo_video.mlt` | `0835fff18a49b651741d821b4644123a390be16929981faa8a78efe43b96af11` |

The task-local evaluator adapter SHA256 was
`9d92b38c8a8ac28a1181e638f4d4c94aa16b2c38f7bbcc0864aafad6d798a41e`;
the unchanged upstream getter SHA256 was
`4d827d235170ee05a629a483aa972fbc341e994258977816706e36f6c37843ce`.
The evaluator environment used Python 3.13.12 with
`opencv-python-headless` 4.12.0.88, whereas the pinned project specifies
`opencv-python~=4.8.1.78`; source resolution decoded as 834×1112. This
dependency/version deviation and the five-second stop acknowledgment lag
limit comparability with a strict official run. The trial is **one selected
task**, not a V2.1 pass rate, and its score must not be averaged with the
earlier Task099 incomplete attempt as a predeclared batch.

The task-owned VM Pod, proxy Pod, Service, local port-forward, and paired AUV
profile were removed; a post-cleanup label query returned no episode
resources. The retained `osworld-v2-hot` PVC remained Bound. A durable
scheduler should make the monotonic action cutoff enforceable by the harness,
persist raw results beyond `/tmp`, and pin evaluator dependencies before
larger batches.
