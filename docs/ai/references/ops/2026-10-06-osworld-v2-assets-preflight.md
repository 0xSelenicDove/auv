# OSWorld-V2.1 gated assets: narrow preflight

Date: 2026-10-06. Scope: pinned release/asset audit followed by a separate
live guest **environment preflight**. No task solve, export, or evaluator was
run. This is not an AUV task result.

## Release identity and access

The pinned [V2.1 release manifest](https://github.com/xlang-ai/OSWorld-V2/blob/3d778a3c9a34a079316f70df023b166700445792/benchmark_releases/osworld-v2.1.json)
selects task dataset commit `0a1aadad95aa79b00b3783e717d865089ab06e26`
and gated asset dataset commit `384b3834faba5700a7b589e6cc181490c9808949`.
Authenticated HF metadata access succeeded for the latter exact commit. Its
`repo_info(files_metadata=True)` inventory has **1,084 files, 4,920,057,674
bytes** (about 4.58 GiB); this count agrees with the release's verification
note. This is metadata, not a local full-snapshot verification. The complete
snapshot was intentionally not downloaded for this two-task preflight. Download
it once, using the pinned commit and byte verification, when an approved batch
actually needs broader task coverage; do not treat two files as 108-task
readiness. The official [asset downloader](https://github.com/xlang-ai/OSWorld-V2/blob/3d778a3c9a34a079316f70df023b166700445792/scripts/tools/download_osworld_v2_assets.py)
supports repeated `--allow-pattern` filters for this narrow stage.

## Two task-specific files

The [pinned gated asset inventory](https://huggingface.co/datasets/xlangai/osworld_v2_assets_gated/tree/384b3834faba5700a7b589e6cc181490c9808949)
reported these LFS hashes and sizes. Both local files were byte-hashed and
matched. The two [task classes](https://huggingface.co/datasets/xlangai/osworld_v2_tasks/tree/0a1aadad95aa79b00b3783e717d865089ab06e26)
also match the release's [task hash manifest](https://github.com/xlang-ai/OSWorld-V2/blob/3d778a3c9a34a079316f70df023b166700445792/benchmark_releases/osworld-v2.1.task_hashes.json).

| Task | Asset path under `OSWORLD_FILE_BASE_URL` | Bytes | LFS/local SHA256 | Task-class SHA256 |
| --- | --- | ---: | --- | --- |
| 099 | `task_099/my_image.png` | 1,851,281 | `6c99998e7275e2132c6e27d01f212eaabfbf1c10b0e9405d11aa125eb94a16c1` | `58c460fdfecf518f64714fdc21933b60818d8cf28ec02f9fa15a10e56ef02e32` |
| 044 | `task_044/promo_video.mp4` | 5,789,382 | `987ee02e31537ad83cbe3da15502366d88be944e8b8cec08538adf51185d5108` | `3ff702eb197aff0e4987537c7a98c40f50100f8c416371c9f0d424c6a3a860ad` |

Both are retained at
`/tmp/auv-osworld-batch-20261005/v2-assets-pilot/task_{099,044}/`.
The existing Task099 image was rechecked; only Task044's 5,789,382-byte video
was newly downloaded. No `--clean` was used.

## Task044 execution boundary for a later pilot

The pinned [Task044 class](https://huggingface.co/datasets/xlangai/osworld_v2_tasks/blob/0a1aadad95aa79b00b3783e717d865089ab06e26/task_044.py)
asks the agent to use Shotcut to crop the AI watermark from the top of
`promo_video.mp4`, avoid stretching, export `promo_video_v1.mp4` at no more
than 1 MiB without changing resolution, and save `promo_video.mlt`. It is a
single local-media task with `proxy=False`, `fixed_ip=False`, no mocked-site
URL, and no CDP path in the task class.

- `setup()` calls the official [SetupController.download/launch](https://github.com/xlang-ai/OSWorld-V2/blob/3d778a3c9a34a079316f70df023b166700445792/desktop_env/controllers/setup.py)
  for exactly `task_044/promo_video.mp4` to
  `/home/user/Desktop/promo_video.mp4`, then launches `shotcut`. This is
  benchmark preparation, not AUV GUI input. The launch helper logs non-200
  responses rather than raising, so a later pilot must independently verify
  the guest process/window. Guest Shotcut/codec availability has **not** been
  verified here.
- `evaluate()` fetches the exported MP4 and MLT project using the official
  [get_vm_file getter](https://github.com/xlang-ai/OSWorld-V2/blob/3d778a3c9a34a079316f70df023b166700445792/desktop_env/evaluators/getters/file.py).
  The getter calls the controller's `/file` endpoint and saves returned bytes
  to evaluator cache. Missing either output gives `0.0`.
- If both exist, `_evaluate_natural_crop_task()` parses MLT XML and scores the
  first crop filter: top crop 75–85 px (0.4), Center disabled or `qtcrop`
  (0.2), export size at most 1 MiB (0.2), and unchanged source/export video
  resolution (0.2). It fetches the source video through the same getter and
  reads resolution with OpenCV (`opencv-python` appears in the pinned
  [pyproject](https://github.com/xlang-ai/OSWorld-V2/blob/3d778a3c9a34a079316f70df023b166700445792/pyproject.toml)).
  The reachable Task044 evaluator body contains no PyAutoGUI calls or GUI
  input. It also does not use CDP or a mocked website.

The task class does not request a physical GPU. Whether the pinned Xorg guest
has a working Shotcut install, codecs, and enough software-rendering capacity
was a live preflight question; no GPU sufficiency claim follows from the
static code. AUV must own every agent GUI observation/action in that pilot.

## Task044 live environment preflight

A fresh official V2.1 QEMU/KVM overlay ran on `liet-gpu-1` as task-owned Pod
and Service `osworld-v2-task044-preflight` (Pod UID
`94d38c79-391e-4fc9-914d-8498a00c060b`) with a separate `-proxy` Pod.
The pinned video was transferred through the setup control plane to
`/home/user/Desktop/promo_video.mp4`; `/setup/upload` reported
`File Uploaded: 5789382 bytes`, and the guest SHA256 was
`987ee02e31537ad83cbe3da15502366d88be944e8b8cec08538adf51185d5108`.
`/setup/launch` with `['shotcut']` reported `shotcut launched successfully`,
and the process was observed running. This was a control-plane-equivalent
setup, **not** a direct call to Python `Task044.setup()`.

Guest `ffprobe` identified H.264/AAC video at 834×1112, about 7.06 seconds;
`ffmpeg -hwaccel none` decoded three frames on CPU. Installed guest AUV
0.0.28 (binary SHA256
`2a8e53eecfef1dcd8fa8368fa480d6df36e254527c7e60be3ac82802e7073427`)
was paired as Device
`f39358fa3caccdd02ba781a049495401f7a105207b60687a1158094f14ede4e2`.
Only AUV `input.clickPoint`, `input.typeText`, `input.keys`, and capture
opened the video and canceled Shotcut's initial variable-frame-rate
“Convert to Edit-Friendly” prompt. Capture Run
`e69063bd-e625-691f-c6be-f476b20c1346` (PNG SHA256
`86b565969313a65b60284f1a0cd6e74580198dc9db878e1a624ff6c06552d5a0`)
visibly shows `promo_video.mp4` in Shotcut's preview. The prompt cancellation
belongs only to this disposable preflight, not a later benchmark episode.
No crop, project save, export, or evaluator call occurred. A GUI preview and
CPU `ffmpeg` decode do **not** prove Shotcut's own pure-software rendering or
export performance.

Before AUV ran, the guest needed a bounded install of `libtesseract4`,
`liblept5`, and English Tesseract data. The task-owned Pod, Service, proxy,
port-forward, and pairing profile were subsequently removed and checked
absent; `osworld-v2-hot` remained Bound. A later Task044 attempt must start
from a **new** overlay, treat the VFR prompt as part of its action budget, and
record an actual evaluator result rather than promoting this preflight to a
task score.
