# Windows accessibility text read

This owner-approved slice exposes the existing Windows UIA snapshot through
`auv invoke window.accessibility`, `WindowService/SnapshotAccessibility` and
`WindowClient::accessibility_snapshot`. The invoke registry also makes the
command available to the existing MCP frontend. It reuses the Windows driver;
there is no second UIA traversal or OCR implementation.

The command requires an `app:` or observed `window:` target, reads without
activation or input, and returns the selected window reference, node paths,
roles, raw names, optional raw values, automation IDs, class names and focus.
The provider's strings are preserved, including an exposed empty value;
unsupported ValuePattern remains absent. Invoke publishes the exact direct
result as an `auv.window.accessibility` JSON artifact through normal tracing.
Runner-backed CLI and SDK reads use the same typed WindowService method.

Traversal retains the existing driver limits of depth 40 and 2,000 nodes.
`completeness_known:false` explicitly reflects that the driver does not report
whether those limits or a provider failure omitted nodes. Paths belong to the
current snapshot and are not durable selectors. A provider name can differ
from rendered text or expose a clipped/offscreen label; consumers must verify
the selected control and current application state. Other backends return
unimplemented. Dry run validates the target without performing a snapshot.

NOTICE: Node geometry is deliberately excluded until its Windows DPI coordinate
contract is validated. Input must use the existing fresh capture contract. This
text read does not add focus, value-write, input or hidden-state capabilities.

The preceding live probe preserved fixture text in 18/18 reads across six cases,
including the [3,2] pattern that visual zoom/counting repeatedly misreported,
leading/trailing spaces, Unicode and quotes. Its test helper is not a public
command support claim. Evidence: outputs/windows-verbatim-probe. Current command
validation and the fresh model benchmark are recorded separately in
outputs/windows-editing-verbatim; results will be added when complete.

The editing trial derives its note mechanically from a model-selected raw node
property and explicit label prefix. Prefix removal never trims following text.
The helper knows no expected fields. This trial evaluates the complete new
read/report workflow, not an isolated change in zoom policy. Provider absence
or uncertainty requires an incomplete result rather than invented exact text.


## Completed validation and editing trial

| Workflow | Median seconds | Median total tokens | Median uncached input + output | Exact answer | Save/reload | Complete |
|---|---:|---:|---:|---:|---:|---:|
| baseline | 104.59 | 620,115 | 46,035 | 1/3 | 3/3 | 1/3 |
| verbatim | 75.72 | 468,547 | 41,411 | 3/3 | 3/3 | 3/3 |

Verbatim median changes: -27.61% time, -24.44% total tokens, -10.04% uncached input plus output. Aggregate changes: -19.90%, -14.55%, +0.08% respectively. Paired wins: 3/3 time and 3/3 tokens.

Notes match [3,2], [4,1], [1,3] internal ASCII space runs. The first reproduces the previous visual failure. Both arms receive two A subjects and one B subject. B blocks its first save, requires review confirmation and commits once. Pair order alternates. Oracle and app state logs are withheld from subjects. Timer includes private Runner and model startup, all help/reference, actions/reads/captures, report serialization, final answer and owned cleanup. Resets and independent audits occur between timed subjects without overlap.

See outputs/windows-editing-verbatim/report.md for all retained failures, validation and limitations.


## Windows-MCP-inspired UIA caching

The owner-approved caching slice batches the existing node properties and
ValuePattern into a fresh `IUIAutomationCacheRequest` per snapshot. Root and
walker reads cache only one element at a time (`TreeScope_Element`), so the
40-depth/2,000-node traversal bounds still apply before fetching more nodes.
No shared cache survives a snapshot. A provider that rejects cache setup,
navigation or individual cached properties retains the live-property path.
Cached Name and Value strings are not trimmed; `Some("")` remains distinct
from an unsupported pattern. Geometry, focus and control-view child-index
paths retain their existing meanings. The recorded CLI/Runner contract is
unchanged.

Source: CursorTouch/Windows-MCP commit
`b455c2766c63599d466a6178641bac70787979a4`, `tree/cache_utils.py`.
See `THIRD_PARTY_NOTICES.md` for attribution and the upstream MIT notice.
This is a Rust implementation of the caching idea, not a Python dependency.

NOTICE: explicit truncation reasons and RuntimeId cycle detection remain
candidate slices requiring their own contract and validation; they were not
part of the accepted first caching implementation. Current completeness
metadata remains unknown. Provider blocking calls are not time-bounded by
this change.

Validation evidence and matched snapshot timings: outputs/windows-uia-cache.
The timings measure the driver read only, not model latency or token use.


## Matched model editing benchmark with UIA caching

Six fresh model subjects, three matched pairs, all retained without exclusions or reruns. Both arms use recorded window.accessibility and the same mechanical verbatim report helper. Only the frozen AUV CLI/Runner binary differs: before versus after Windows-MCP-inspired property caching. Pair prompts are verified identical after replacing trial directory names. The full original inline skill, required operations reference, relevant help, joined original image delivery and file keyboard transport are shared. Model: gpt-5.6-sol, low reasoning. No upstream fetch or runtime source changes during the cohort.

| Arm | Median seconds | Median total tokens | Median uncached input + output | Complete |
|---|---:|---:|---:|---:|
| uncached | 86.31 | 527,823 | 45,504 | 3/3 |
| cached | 87.77 | 482,552 | 49,710 | 3/3 |

Cached median changes: +1.68% time, -8.58% total tokens, +9.24% uncached input plus output. Aggregate changes: -1.58%, -1.35%, +6.18% respectively. Paired wins: 2/3 time and 2/3 total tokens.

| Trial | Arm | Case | Seconds | Total tokens | Uncached input + output | Complete |
|---:|---|---|---:|---:|---:|---|
| 1 | uncached | A | 86.31 | 527,823 | 53,199 | True |
| 2 | cached | A | 88.42 | 482,552 | 41,592 | True |
| 3 | cached | B | 87.77 | 631,028 | 58,868 | True |
| 4 | uncached | B | 88.05 | 554,176 | 45,504 | True |
| 5 | uncached | A | 81.41 | 520,808 | 42,728 | True |
| 6 | cached | A | 75.53 | 467,630 | 49,710 | True |

All six actual saves, independent navigation reloads and exact answers pass. Both B subjects recover from one blocked save with review confirmation and commit exactly once. Notes have internal space runs [3,2], [4,1] and [1,3], identical within each pair. Pair order alternates uncached/cached, cached/uncached, uncached/cached. Expected state/event logs and other subjects are withheld from models.

The timer includes private Runner/model startup, all help/reference/tool calls and reads/captures, report serialization, final answer and owned cleanup. Coordinator resets and audits occur between timed subjects. Audit verifies app-owned state, untouched fields, exact whitespace and mechanical counts; original inline skill/reference provenance; raw operation failures and input files; all original model-delivered image bytes/pixels and joined delivery; selected node/property/prefix against fresh recorded UIA artifacts and their hashes; report equality; and every original native final frame, manually inspected. All frozen protocol/source/binary hashes verify. No accuracy or instruction failures in this cohort.

Interpretation: this small exploratory trial does not demonstrate a consistent full-task speed or cost improvement from caching. Median total tokens fall, median time rises, and uncached input/output rises. Model/tool-call variation exceeds the small driver-read saving measured separately in outputs/windows-uia-cache (7.5–8.6% faster driver snapshots). Token differences here are observations, not proof that caching caused them, and total-token counts do not establish billing savings. Keep the verified driver optimization; do not claim end-to-end savings or change general skill defaults from this cohort. The next useful optimization would target repeated model context/image/tool overhead.

Preparation notes: a generated indentation error was fixed before any subject ran and before final protocol hashes were frozen. The first audit invocation used a Python without Pillow; it failed before reading results, then the bundled runtime completed all audits. Neither changed subject behavior or excluded a subject. The six model subjects were not rerun. Existing unrelated repository changes are preserved. No commit or push. All owned daemons/fixture closed and temporary auth copies removed.

Evidence: outputs/windows-editing-cache.
