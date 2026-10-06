# OSWorld-V2.1 Task044 evaluator bridge: offline boundary audit

Date: 2026-10-07. Status: offline implementation and tests only; **no new
Kubernetes run or agent score**. This is a benchmark-local setup/evaluator
bridge for the already selected Shotcut Task044, not a replacement for the
official V2.1 runner or an AUV GUI-input path.

The bridge pins upstream revision
`3d778a3c9a34a079316f70df023b166700445792`, the Task044 source SHA256
`3ff702eb197aff0e4987537c7a98c40f50100f8c416371c9f0d424c6a3a860ad`,
official `get_vm_file` SHA256
`4d827d235170ee05a629a483aa972fbc341e994258977816706e36f6c37843ce`,
and gated source video SHA256
`987ee02e31537ad83cbe3da15502366d88be944e8b8cec08538adf51185d5108`
(5,789,382 bytes). It requires the upstream `opencv-python==4.8.1.78`
distribution, rejecting the different headless wheel used in the earlier
[exploratory Task044 attempt](2026-10-06-osworld-v2-task044-episode.md).
This reduces a known evaluator-dependency difference; it does not retroactively
make that prior attempt an official or predeclared batch result.

`prepare` executes the unchanged pinned Task044 `setup()` method through an
allowlisted loopback HTTP transport: upload that one video to
`/home/user/Desktop/promo_video.mp4`, verify the guest readback hash, then
request `shotcut` launch. The launch receipt proves the guest accepted the
request, not that the GUI stayed open. `evaluate` executes the unchanged
pinned Task044 scorer and official file getter, allowing `/file` reads only
for the source video, exported MP4, and MLT project. It distinguishes the
task's genuine missing-output `0.0` from transport/cache/source-integrity or
decoder failures, which exit without a score. There is no `/execute`,
screenshot, or GUI-action transport in this bridge. All task-directed GUI
work must go through installed AUV between the two phases.

The bridge verifies the checkout is clean and refuses a changed revision,
source, getter, asset, or OpenCV package before guest access. Offline tests
exercise those pins, endpoint/path/command allowlists, exact setup receipts,
the original scorer's crop/Center/size/resolution boundaries, legitimate
missing-output zeroes, and evaluator failure layering. The complete
`evals/osworld/tests` suite passed **151 tests, 10 skipped** under local
CPython 3.12.13 with `opencv-python==4.8.1.78`,
`numpy==1.26.4`, and `requests==2.34.2`. The skips are unrelated optional
fixtures; this is local test evidence, not live guest acceptance.

The episode marker records the endpoint string and source identity but cannot
prove a reused local port-forward still reaches the original Pod. The new
`k8s_v2_task044_adapter.py` uses the existing six-phase lifecycle to pin Pod
identity, require a fresh overlay, capture through AUV, and perform
UID-preconditioned task-owned cleanup. Its fixed action is capture-only, and
it has **not** yet passed a live Task044 episode. The relay now accepts
`--adapter v2-task044` for a later attended agent action phase; it does not
enforce isolation from the agent's other tools. The earlier exploratory Task044 score
`0.6000000000000001` remains separate from any future cohort denominator.
