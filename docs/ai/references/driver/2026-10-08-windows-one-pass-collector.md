# Windows one-pass text-evidence collector

Owner-approved workflow pilot following the Windows benchmark and upstream
integration. The source is
[`collect_scroll_targets.rs`](../../../../crates/auv/examples/collect_scroll_targets.rs).
It uses the existing bidirectional `WindowClient::scroll_until` stream and
`CapturesClient::record_artifact`; no new protocol surface is introduced.

The example accepts a JSON configuration path. Supply a private daemon endpoint,
its observed local Device ID, a freshly observed window ID, query names,
normalized input point and viewport, scroll step, total step budget, settle time,
and mode (`one_pass` or `sequential`). Use a task-owned Windows named-pipe daemon
with `serve --no-register --store-root` and stop only that owned process.
Build with `cargo build -p auv-core --release --example collect_scroll_targets`.

One-pass mode checks all pending names in each OCR update. Sequential mode checks
one name per stream while retaining forward position. Both modes preserve the
same Runner and use identical capture/OCR/input policy. The initial and terminal
updates are processed; matched captures are recorded immediately rather than
relying on temporary references surviving eviction. Split OCR columns are grouped
by vertical alignment. Near-edge and ambiguous matches are rejected. Original
candidate OCR remains unchanged; every result explicitly needs screenshot
verification. The supplied viewport must exclude static chrome.

Twelve mechanical A/B trials (three per mode/case, rotated ordering) used the
merged Windows GNU build and the same collector binary SHA-256:
`e18bc180202f9ada4f9dfeb54298e8b8f19a2883111a4293c90dc2dd11c48547`.
The read-only 180-row canvas fixture and targets match the preceding benchmark.
All 96 retained record screenshots passed visual/oracle and receipt-hash checks.
Both modes used 28 steps on A and 29 on B with no reset/recovery.

| Case | One-pass median | Sequential median | Median saving |
| --- | ---: | ---: | ---: |
| A | 5.57 s | 5.98 s | 6.76% |
| B | 5.80 s | 6.15 s | 5.72% |

Timing includes daemon startup, discovery, collector, recording and daemon
cleanup. It excludes model reasoning, final visual auditing and tokens. These
figures are execution evidence, not an end-to-end model benchmark or platform
support claim. Both modes already use persistent execution; its independent
benefit remains unmeasured. The evidence pack is attached to the originating
Windows chat under `outputs/windows-collector/`, with full metrics, raw streams,
failures, original receipts, images, source and fingerprints.

Four focused tests cover row association, clipping/ambiguity, chrome exclusion
and separator-jitter normalization. Coverage signatures are heuristic and may
reject unstable OCR. The collector records partial evidence and stops on a gap;
adaptive backtracking is deferred pending a separately approved recovery slice.
Multiline table parsing and repositioning to an earlier final requested record
are outside this pilot. `last_requested_visible` reports the actual condition.

Do not promote skill guidance or claim token savings until fresh matched A/B
model trials include final screenshot inspection, native control, rotated
ordering, repeated measurements and all failures.

## Follow-up model evidence

The originating Windows chat subsequently ran 18 fresh model trials on merge
`41969075`: three repeats per A/B case with native control, current skill and
the one-pass helper. All 144 records and final targets passed the evidence
audit. The collector pooled median was 38.71 seconds and 60,191 total tokens;
the skill median was 65.13 seconds and 143,392 tokens. The native comparison
also differed in image dimensions and included coordinator relay delay; these
results do not isolate the collector algorithm or establish general support.
Evidence is retained in `outputs/windows-model-trials/` in that chat.

After merging upstream through `a9100ed2` into local merge `ac06c71e`, 12 new
paired collector trials compared full and compact model-facing manifests.
Both formats retained complete receipts and raw updates on disk. Compact stdout
kept query, unverified candidate text, image path, SHA256 and scan status, with
paths to the complete manifest and raw stream. All 96 records, receipt hashes,
image deliveries and final visible targets passed the audit.

Compact reduced median stdout bytes by 48.94%, but pooled median total tokens
were effectively unchanged: 77,981 versus 78,066. Median elapsed time was
44.61 versus 43.29 seconds. These six runs per format do not establish a token
or latency improvement; leave the full helper as the default and keep the
compact projection experimental. No Rust operation or public API changed.
The source helper, exact prompts, rollout image calls, stores, build checks and
individual metrics are retained in `outputs/windows-compact-latest/` in the
originating chat. Verification batching varied (two full-format runs used two
image batches), so a separately controlled verification workflow is a candidate
next experiment, not an implemented optimization at that point.

A subsequent 12-run paired experiment on the same frozen merge required one
batch of all eight original images in both formats. All 96 records, receipt
hashes, delivered images, complete rows and final targets passed. Full pooled
medians were 40.01 seconds and 62,986 total tokens; compact medians were 38.19
seconds and 60,056 tokens (4.56% faster and 4.65% fewer tokens). Compact won
five of six pairs for both metrics, but used more tokens cumulatively: 388,799
versus 378,253. One compact subject read additional permitted Runner reuse
guidance, taking 49.43 seconds and 88,713 tokens; it remains included. There
were no timed tool failures and all subjects used exactly one image batch.

Keep full as the default and compact experimental. Lower medians than the
earlier experiment do not isolate batching: that experiment ran at a different
time and mostly already used one image batch. Optional guidance reads still
vary and add repeated context. Evidence is retained in
`outputs/windows-batch-trials/` in the originating chat. Complete-row crops
with originals retained and separately controlled reference reads are candidate
next experiments; neither is implemented in this slice.

## Complete-row crop experiment

The originating chat then completed 12 fresh paired full-window/crop trials
on the same frozen merge. Both arms used the same compact stdout schema and
exactly one batch of eight images at original detail. A task-scoped Python
helper derives full-width row bands from the recorded physical OCR bounds,
subtracting capture origin and scaling to actual bitmap dimensions. It adds
at least 16 pixels or one OCR row height of vertical padding on each side,
falls back to originals on invalid/clipped bounds, and retains all original
Runner receipts and raw updates. Crops are lossless PNGs with parent SHA256,
coordinates, dimensions and their own hash; OCR remains unverified.

All 96 records, original/derived hashes, full rows, final targets and exact
crop-to-parent decoded pixels passed. No fallback, image recovery or timed
tool failure occurred. Full pooled medians were 42.01 seconds / 62,875 total
tokens; crops were 34.60 seconds / 56,177 tokens: 17.63% faster and 10.65% fewer
tokens. Crops won all six pairs for both metrics. Cumulative tokens were
392,704 full and 337,265 crop. One full subject read an extra permitted
reference and is retained. Image encoding and path lengths also differ, so
these are workflow gains rather than an isolated pixel-area effect.

Evidence is retained in `outputs/windows-crop-trials/` in the originating
chat. The helper is experimental for these single-line records; full-window
verification remains the general default. Surrounding context and multiline
tables require further evidence. No Rust operation, public API or skill was
changed. Consolidating required task guidance to reduce repeated context is
a candidate next slice, not part of this implementation.

## Joined collection and image delivery

A subsequent 12-run experiment kept the crop helper, full skill, image pixels,
manifest schema and Runner lifecycle unchanged. The separate arm returned
collector stdout to the model before a second image-view invocation. The
joined arm ran collection and opened the eight returned paths in the same
`functions.exec` call, returning manifest and crops together. Both first read
the complete skill. All 96 exact records, complete rows, original/crop receipts,
derived pixels, initial/final views and delivered PNG bytes passed. No timed
tool failure, yielded collection session or image recovery occurred.

Separate pooled medians were 31.57 seconds / 56,775 total tokens; joined
medians were 26.21 seconds / 41,015 tokens: 16.98% faster and 27.76% fewer tokens.
Joined won all six pairs for both metrics. Tool calls fell from three to two
in every pair. Cumulative tokens were 340,233 separate and 246,213 joined.
Provider totals include cached input; these are invocation-workflow results,
not an independent measurement of model-turn cost or dollar savings.

The audit distinguishes image loading from model input detail: both request
`view_image` original detail and deliver exact PNG bytes, while the host labels
the emitted `image(image_url)` blocks as `high`. All 96 image blocks use the
same setting. Earlier references to original detail describe the viewer
request; they should not be interpreted as evidence of original model-input
mode without checking the rollout.

Evidence and the concrete invocation script are retained in
`outputs/windows-joined-trials/` in the originating chat. The workflow remains
task-scoped; no general skill or public AUV API was changed. Further guidance
consolidation would require a separate controlled experiment that preserves
all applicable instructions.
## Complete skill in the initial prompt

A further 12 fresh Windows subjects compared reading the complete skill from its file with receiving the identical complete UTF-8 skill bytes in the initial prompt. Both arms used the same joined collector invocation, complete-row crops, release binaries and upstream merge ac06c71e (upstream through a9100ed2). There were three repeats per A/B case and arm, in rotated serial order, using gpt-5.6-sol with low reasoning.

File delivery had a pooled median of 25.63 seconds and 41,064 total tokens; inline delivery had 21.38 seconds and 29,426 tokens: 16.6% less time and 28.3% fewer tokens. Inline won all six paired comparisons on both measures and reduced tool calls from two to one. Median uncached input plus output was 10,006.5 versus 9,458 tokens; provider caching varies, so this does not establish a dollar-cost saving. File trial 12 took 43.85 seconds and remains included without replacement.

All 96 exact records passed, including every B run. Actual model rollouts independently verified complete skill delivery and exact prompt hashes. Every subject received eight images in one batch; emitted model image blocks used high detail while the viewer loaded original PNG bytes. Delivered hashes, original receipts, crop-parent hashes and decoded pixels, crop bounds, complete visible rows and final target screenshots all passed. There were no failed operations, image recovery calls or crop fallbacks.

The initial prompt preserves the full skill, including required reference-reading instructions; it does not summarize or remove instructions. Prompt construction is coordinator preparation outside the agent timer, but the complete inline skill counts toward provider input tokens. Placement and removal of the file-read turn change together, so this measures the whole delivery workflow. This remains a task-scoped experiment for synthetic single-line rows; no Rust operation, public API, skill text or general default changed. The complete report, prompts, rollouts, receipts, images, audits and fingerprints are retained in outputs/windows-inline-trials.

## Skill delivery on multiline cards

A further test-only Windows fixture used 54 three-line canvas cards with current codes, statuses and quantities, historical-code distractors and reversed field order on neighboring cards. All selected targets happened to use Code then Status, so reversed target-field order remains untested. Each subject extracted eight cards, computed quantity totals for all four statuses, including absent-status zeroes, and selected the largest quantity. Both arms used original full-window evidence because the single-line crop helper cannot cover multiline cards.

Twelve fresh subjects, three repeats per A/B case and delivery arm, passed all 96 records and all 12 derived summaries. Full-skill delivery, executed prompt hashes, original receipts, full image dimensions and decoded RGBA pixels delivered to the model, initial top viewport and complete final target passed. Sixteen distinct target captures were also manually inspected, with reuse permitted only for matching hashes. WebP-to-PNG encoding changes preserve the decoded pixels. Model image detail remained high; the viewer loaded original detail.

File delivery had median 26.52 seconds and 46888.5 total tokens; inline delivery had 22.48 seconds and 35193 total tokens. Inline won all six pairs on both measures. Uncached input plus output totals were 123,358 versus 122,313 tokens; caching varied and this does not establish a billing saving.

The same ac06c71e merge, upstream through a9100ed2, release binaries, collector, helper implementations, full skill and gpt-5.6-sol/low settings remained fixed. Prompt preparation and reset are outside subject timing; complete inline instruction bytes count as input. No trial was excluded or rerun. This expands evidence to read-only multiline retrieval and aggregation with a deterministic supplied collector; interactive editing, unstructured workflows and a general default remain unvalidated. The report and complete evidence pack are retained in outputs/windows-varied-trials.


## Editing workflow after five upstream commits

Merged upstream c3e67470217799be0d43d81bfd050dbd4d44db49 locally as be2b7660, including all five commits after a9100ed2. Adapted the local collector from RatioRect to RelativeRect without a logic change. Rebuilt release CLI and collector. Validation: 4 collector, 125 Windows driver (2 ignored; 3 clipboard tests skipped), 1 capture-preview, 2 targeted geometry, 18 API-server and 18 scroll tests passed. JavaScript suites and Unix signal behavior were not tested on Windows.

Two retained preliminary editing subjects stopped because Windows intentionally rejects targeted keyboard batches. The corrected adapter uses supported foreground keyboard input after a window-targeted field click and visual focus check. This changes the adapter, not the driver's deferred keyboard lease support. A preflight endpoint flag error was corrected before timed runs. Failed pilots remain in outputs/windows-editing-trials.

Twelve fresh gpt-5.6-sol/low subjects used three repeats per case/arm in rotated serial order. All 12 saved the requested Quantity and verified it after leaving/reopening; all six B subjects observed the first blocked save, confirmed review and saved exactly once. Untouched fields stayed exact. Complete results including reported fields: file 3/6, inline 6/6. File trials 1, 6 and 9 reported an extra space after “double” in the unchanged Note. They remain in the comparison, as does slow file trial 9.

Combined medians: file 113.49 seconds / 568221.5 total tokens / 47383.5 uncached input plus output; inline 90.60 seconds / 580967.5 total tokens / 57971 uncached input plus output. Inline is about 20.16% faster but uses about 2.24% more median total tokens. Aggregate totals differ because the retained file outlier is large. This is not evidence of savings at equal full accuracy or billing savings.

Actual rollouts verify complete skill and operations-reference delivery, receipts and exact decoded original image pixels/dimensions. All 12 native final frames were visually checked and independently OCR checked; app-owned events verify exact whitespace. The model dynamically chooses command/image call consolidation, so this experiment does not isolate instruction placement. Next candidate is one joined command-and-original-image call per UI step, keeping exact field and recovery checks. Full raw evidence, sources and hashes are in outputs/windows-editing-latest. Persistence covers record navigation, not restart or crash durability. No general skill/API default changed and nothing was pushed.


## Combined action and image calls on the editing workflow

Twelve fresh serial gpt-5.6-sol/low subjects used the same full inline skill, operations reference, fixture, adapter and release binary at be2b7660 / upstream c3e67470. Three repeats per A/B case and arm used rotated order. Separate arm required adapter execution and original image viewing in different consecutive calls; joined arm required both in one call, with no next state-dependent action before inspecting images. Full model rollouts audit the delivery rule for each adapter operation, full instructions and exact decoded original image pixels/dimensions. App events and all twelve visually/OCR checked final native screenshots independently verify saves and navigation reload. No code, skill or helper changed mid-trial.

Save/reload outcomes 12/12; exact reported fields 12/12; all checks 10/12. separate: median 123.94 seconds, 799074.5 total tokens, 50358.5 uncached input plus output; 5/6 all checks. joined: median 90.58 seconds, 496203.0 total tokens, 48094.5 uncached input plus output; 5/6 all checks. Joined median change: -37.90% total tokens and -26.92% elapsed time. All subjects contribute, including failures; no savings at equal flawless execution or billing savings are claimed.

Trial 5 searched only for PNG paths although capture returned WebP. Its first capture was not delivered in that call; a fresh observation with JSON image_paths parsing corrected it. Trial 6 double-encoded keyboard JSON, received invalid_input before delivery, then corrected the argument. Both final tasks succeeded but failed strict all-checks criteria and remain in measurements. Any additional failure is disclosed in the per-trial output audits. No subject was rerun or excluded. The next reliability improvement is explicit image_paths JSON parsing plus a keyboard actions-file transport to avoid shell double-encoding.

Timer includes private Runner startup, subject startup, help/reference reads, capture/actions/verification/final answer and owned cleanup; reset/audit excluded. Foreground keyboard input still requires a targeted click and fresh focus verification. Persistence covers record navigation, not restart/crash durability. Evidence, source and hashes: outputs/windows-editing-joined. No Rust validation was repeated because runtime code stayed frozen; prior Windows checks remain in windows-editing-latest. No general skill default changed and nothing was pushed.


## Editing transport hardening reliability check

Bug fix to the task-scoped adapter and image-delivery guidance after two reproduced subject errors. keyboard-file reads/validates UTF-8 JSON, rejects a double-encoded string before invoking input, and passes one serialized process argument without shell payload quoting. File hashes and parsed CLI arrays are independently audited. The canonical image parser reads image_paths from JSON and rejects empty/malformed results, covering WebP without filename guessing. Six keyboard and seven actual-image-parser regressions pass, including a real Windows Unicode/quote/newline argument round-trip.

Six fresh gpt-5.6-sol/low joined subjects (A/B/B/A/A/B), full original inline skill/reference: 6/6 complete checks, 6/6 save/reload outcomes, 6/6 exact final answers. All final native frames were visually and independently OCR checked. All subjects retained; no reruns. Median 77.10 seconds / 388609.0 total tokens / 39063.0 uncached input plus output. No paired control: this is reliability evidence, not a new efficiency claim or a general failure-rate estimate.

Evidence/source/hashes: outputs/windows-editing-hardened. Previous packs and adapters preserved. Runtime remains be2b7660 / c3e67470; no new Rust/API/global skill changes or push. Foreground keyboard targeting and record-navigation-only persistence limits remain. One coordinator geometry refresh occurred before timing; all source hashes stayed frozen during subjects.


## Paired previous versus hardened editing transport

Twelve fresh serial gpt-5.6-sol/low subjects compare the previous JSON-keyboard adapter with the hardened UTF-8 actions-file adapter plus canonical image parser/guidance. Both arms use the full original inline skill/reference and joined action/image calls; three repeats per A/B case per arm in rotated order. This is a package comparison, not isolation of either change. Frozen runtime: be2b7660 / c3e67470; no mid-trial source changes, reruns or exclusions.

previous: median 111.19 seconds / 491384.5 total tokens / 42561.5 uncached input plus output; 4/6 complete checks and 6/6 save/reload outcomes. hardened: median 84.29 seconds / 389554.5 total tokens / 41221.0 uncached input plus output; 5/6 complete checks and 6/6 save/reload outcomes. Hardened median changes: -20.72% total tokens, -24.19% elapsed time, -3.15% uncached usage. Paired wins: 6/6 tokens, 5/6 time. All failures contribute. These small cohorts are not a general reliability estimate or billing-savings proof.

Rollouts verify complete instructions, original decoded images, receipts, joined delivery and hardened source-file/CLI array equality. App events verify exact fields, one committed save, blocked-save recovery and record-navigation reload; all twelve native final frames were visually and independently OCR checked. Evidence/source/hashes: outputs/windows-editing-transport-pair. No Rust/API/global skill changes or push. Existing foreground-focus and navigation-only persistence limits remain.


## Exact-whitespace reporting verification

The reproduced failure was an extra space in the final report, not a changed app value. Task-scoped inspect-text captures the reloaded state and provides its original image plus a model-selected lossless 4x crop. Mandatory guidance checks individual space runs and abstains on unresolved spacing; neither the adapter nor the subject receives expected text. Six fresh gpt-5.6-sol/low subjects include the original A/B notes and varied single/double/triple spaces: 6/6 exact reports, 6/6 save/reloads and 5/6 complete audits. Every subject retained. Derived pixels/source receipts, full joined image delivery, keyboard transport and exact app state independently checked; two image crop/bounds regressions pass. Evidence and implementation: outputs/windows-editing-exact/source and report.md. This is a reliability check, not a paired efficiency claim or universal byte-accurate visual reader. Ambiguous invisible whitespace still requires abstention or a supported verbatim text source. No Rust runtime or global skill default change and no push.


## Exact-whitespace report serialization verification

The reproduced failure was an extra space in the final report, not a changed app value. Task-scoped inspect-text captures the reloaded state and provides its original image plus a model-selected lossless 4x crop. Mandatory guidance checks individual space runs and abstains on unresolved spacing; editing_report.py derives diagnostic counts from the unchanged note and verifies latest zoom provenance; neither the adapter nor the subject receives expected text. Two fresh gpt-5.6-sol/low subjects verify the shifted double-space pattern in A and combined triple/double-space pattern in B: 2/2 exact reports, 2/2 save/reloads and 2/2 complete audits. Every subject retained. Derived pixels/source receipts, full joined image delivery, keyboard transport and exact app state independently checked; two image crop/bounds and four report serialization regressions pass. Evidence and implementation: outputs/windows-editing-exact-final/source and report.md. This is a reliability check, not a paired efficiency claim or universal byte-accurate visual reader. Ambiguous invisible whitespace still requires abstention or a supported verbatim text source. No Rust runtime or global skill default change and no push.


## Paired benchmark of exact-whitespace reporting fix

Twelve fresh gpt-5.6-sol/low subjects compare the hardened adapter with mandatory lossless text zoom plus whitespace guidance/report-file serialization. Six matched pairs, rotated arm order, three A and three B per arm, varied internal spaces and B blocked-save recovery. Both use full original inline instructions, canonical joined image delivery and file-based keyboard actions. This is a package comparison; every failure and slow subject retained, no reruns/exclusions.

hardened: median 81.45s / 393901.0 total tokens / 47251.5 uncached input plus output; 6/6 exact answers, 6/6 save/reloads, 6/6 shared complete checks and 6/6 package checks. exact: median 95.74s / 519082.5 total tokens / 49801.5 uncached input plus output; 6/6 exact answers, 6/6 save/reloads, 6/6 shared complete checks and 6/6 package checks. Fix median changes: +17.55% time, +31.78% total tokens, +5.40% uncached usage. All subjects contribute. Extra zoom/report requirements apply only to the fix; shared checks enable comparison. Small-cohort and caching/billing limits remain.

Evidence/source/hashes: outputs/windows-editing-exact-pair. App events prove exact whitespace; OCR does not. All twelve native final frames manually inspected. Timer includes startup and owned cleanup; audits/resets excluded between subjects with no overlap. Runtime be2b7660/c3e67470 stayed frozen; no Rust/API/global skill change or push.


## Conditional versus mandatory text zoom

Twelve fresh subjects, six matched pairs, all runs retained without reruns or exclusions. Both arms use the same gpt-5.6-sol/low model, full original inline skill/reference/help, exact zoom adapter, canonical joined image delivery, file keyboard actions and report serializer. Mandatory requires a final zoom even when clear; conditional first inspects the reloaded full screenshot and zooms only on unresolved spacing. Both must abstain if uncertainty remains. Report serialization mechanically derives space counts from the unchanged supplied note and checks latest image provenance; it does not know expected text.

| Policy | Median seconds | Median total tokens | Median uncached input + output | Exact answers | Save/reload | Complete | Zoom used |
|---|---:|---:|---:|---:|---:|---:|---:|
| mandatory | 119.21 | 555,277.5 | 46,472.0 | 5/6 | 6/6 | 5/6 | 6/6 |
| conditional | 101.37 | 455,814.5 | 44,449.0 | 5/6 | 6/6 | 5/6 | 1/6 |

Conditional median changes: -14.97% elapsed time, -17.91% total tokens and -4.35% uncached input plus output. Paired wins: 5/6 time and 6/6 total tokens. All subjects contribute, including failures.

Matched cases vary single/double/triple internal ASCII spaces, including shifted gaps. Each arm has three A and three B tasks; B blocks its first save, requires confirmation and commits exactly once. Pair order rotates which arm runs first. Oracle and app events are withheld from subjects. App events independently verify exact untouched fields, quantities, one committed save, B recovery and navigation reload. Original receipts, exact decoded model images, derived zoom pixels, full instructions, keyboard/report source files, final report equality, focus and raw failures are audited. Native final frames are manually inspected; OCR checks visible fields but cannot prove whitespace.

Timer includes private Runner startup/device selection, model startup/help/reference, all captures/actions, verification, report serialization, final answer and owned cleanup. Resets and independent audits occur between timed subjects, with no overlap. Runtime remains be2b7660/c3e67470; adapters, fixture, binary and instructions are frozen. No upstream fetch, Rust/API/global skill change or push. Four new provenance regressions pass; unchanged suites were not rerun.

This small cohort does not establish equal general reliability, billing savings or the correctness of model confidence. If conditional subjects never zoom, it tests skipping zoom on these cases, not the uncertain-text fallback. Navigation reload does not prove restart/crash durability. Pixel-only inspection cannot generally resolve invisible whitespace; unsupported exact-byte claims require abstention or a supported verbatim text source.

| Trial | Policy | Case | Seconds | Total tokens | Exact answer | Complete | Zoom |
|---:|---|---|---:|---:|---|---|---|
| 1 | conditional | A | 84.11 | 411,494 | True | True | False |
| 2 | mandatory | A | 114.31 | 510,111 | True | True | True |
| 3 | mandatory | B | 106.19 | 609,130 | True | True | True |
| 4 | conditional | B | 104.17 | 517,074 | True | True | False |
| 5 | mandatory | A | 110.94 | 522,801 | True | True | True |
| 6 | conditional | A | 90.19 | 413,470 | True | True | False |
| 7 | conditional | B | 98.56 | 498,159 | True | True | False |
| 8 | mandatory | B | 129.61 | 587,754 | True | True | True |
| 9 | conditional | A | 111.16 | 412,655 | False | False | False |
| 10 | mandatory | A | 124.11 | 493,665 | False | False | True |
| 11 | mandatory | B | 134.91 | 591,526 | True | True | True |
| 12 | conditional | B | 138.64 | 553,840 | True | True | True |

Discrepancies:
- Trial 9: see preserved audit for failed checks and operations.
  - note: reported `'Keep  double  spaces.'`; expected `'Keep   double  spaces.'`.
- Trial 10: see preserved audit for failed checks and operations.
  - note: reported `'Keep  double  spaces.'`; expected `'Keep   double  spaces.'`.

Across all six subjects per arm, conditional totals change by -12.95% time, -15.33% total tokens and +4.52% uncached input plus output. The uncached total increases despite its lower median; token totals do not establish billing savings.

Conditional uses zoom in trial 12 and reports its single-space note correctly. It skips zoom in trial 9 and misses one space in the combined triple/double pattern; mandatory trial 10 misses the same space despite verified zoom pixels. Both report two/ two internal gaps instead of three/ two. The serializer faithfully preserves their supplied note and derives counts; it cannot correct a mistaken visual reading.

Decision: conditional zoom is a promising cost optimization in this cohort, but neither policy meets reliable exact-whitespace reporting. Keep this result experimental. Next investigate explicit gap counting or a supported verbatim text source, and verify the combined spacing case plus genuinely ambiguous text on fresh subjects before changing a general default.

Evidence/source/hashes: outputs/windows-editing-conditional. All subjects retained. No Rust/API/global skill changes or push.


## Explicit visual gap counting trial

Six fresh subjects in three matched pairs, all retained, no reruns or exclusions. This is a prompt-only, test-only comparison of current mandatory zoom against explicit visual gap counting. Both arms use the same frozen gpt-5.6-sol/low model setting, complete original inline skill, required operations/help, adapter, joined original-image delivery, file keyboard transport and unchanged report serializer. Both require the same final lossless 4x nearest-neighbor text zoom. Counting adds a glyph-origin rule: count last-left to first-right character advances, subtract one, record each blank run independently in visual_gap_counts, then reconstruct the note. Unresolved gaps require abstention. The serializer derives note_space_runs without changing the note and cannot fix a mistaken visual reading.

| Workflow | Median seconds | Median total tokens | Median uncached input + output | Exact answer | Save/reload | Complete |
|---|---:|---:|---:|---:|---:|---:|
| baseline | 103.48 | 496,819 | 46,998 | 2/3 | 3/3 | 2/3 |
| counting | 109.03 | 532,490 | 49,290 | 2/3 | 3/3 | 2/3 |

Counting median changes: +5.36% time, +7.18% total tokens, +4.88% uncached input plus output. Aggregate changes: +24.86%, +22.99%, +7.67% respectively. Paired wins: 0/3 time, 1/3 total tokens.

Matched note patterns are internal space counts [3,2], [4,1] and [1,3]. The first reproduces the prior failure; two patterns are new. Each arm gets two A tasks and one B task. B must recover from its first blocked save and commit once. Pair order alternates. Timer includes private Runner/model startup, help/reference reads, every input/image/report step, final answer and owned cleanup. Fixture resets and independent audits occur between timed subjects, with no overlap. Oracle and app events are withheld from subjects.

Independent audit verifies app-owned exact fields/whitespace, one committed save, B recovery and navigation reload; instruction provenance; focus; keyboard/report files; all source/delivered images and zoom pixels; latest evidence; final report equality. Counting additionally requires its supplied ledger to equal independently recorded app whitespace. A matching ledger is not proof of independent reasoning. Native final frames are manually inspected; OCR checks visible fields but does not prove whitespace. All failures remain in the denominator.

| Trial | Workflow | Case | Seconds | Total tokens | Exact | Complete |
|---:|---|---|---:|---:|---|---|
| 1 | baseline | A | 103.48 | 454,422 | False | False |
| 2 | counting | A | 107.11 | 497,282 | False | False |
| 3 | counting | B | 109.03 | 532,490 | True | True |
| 4 | baseline | B | 103.61 | 590,509 | True | True |
| 5 | baseline | A | 81.17 | 496,819 | True | True |
| 6 | counting | A | 143.78 | 866,388 | True | True |

Discrepancies:
- Trial 1: see preserved audit for all failed checks.
  - note: reported `'Keep  double  spaces.'`; expected `'Keep   double  spaces.'`.
- Trial 2: see preserved audit for all failed checks.
  - note: reported `'Keep    double   spaces.'`; expected `'Keep   double  spaces.'`.
  - Visual gap counts: `[4, 3]`; ledger did not match app-owned note.

This small exploratory cohort cannot establish general reliability or billing savings. Pixel-only spacing depends on font, scaling and confidence; unsupported exact-byte claims require abstention or a supported verbatim source. A successful reload does not prove crash/restart durability. Runtime remains be2b7660/c3e67470; no upstream fetch, Rust/API/global skill change, commit or push. No helper/runtime changes require rerunning unchanged suites. Cleanup and frozen hashes are checked, and source/evidence files are packaged with a verified SHA256 manifest.

Decision: do not adopt this counting prompt. Both workflows pass 2/3 exact answers and both fail the reproduced [3,2] pattern. Baseline reports [2,2]; counting reports [4,3] despite its subtract-one instruction. Counting increases median and aggregate time/tokens. Trial 6 is slow and expensive but retained; all six save/reload checks and both B recoveries pass. The next scoped investigation is a supported verbatim text source where available, with explicit abstention when pixel-only inspection cannot resolve exact whitespace. No production fix is claimed by this prompt trial.

Evidence/source/hashes: outputs/windows-editing-gaps.


## Windows verbatim accessibility text probe

The existing AUV Windows accessibility driver preserved the fixture note verbatim in **18/18 reads across six live cases**. All compared strings match independent app-owned state. The reproduced [3,2] gap pattern, [4,1], [1,3], leading/trailing spaces, Unicode and quoted text passed. Search and quantity ValuePattern values also matched the loaded record in each read. Missing-argument and closed-window checks failed without emitting a snapshot.

A test-only helper calls existing window enumeration and `auv_driver_windows::snapshot_window`. It only accepts the observed fixture window and emits raw node names/values as UTF-8 JSON. It contains no expected notes, fixture/event/source reads, OCR, text normalization or input actions. The note is exposed through **UIA CurrentName**, not ValuePattern. The label prefix is part of that raw name; its removal must preserve all following characters. Native frames confirm the loaded records; trailing spaces cannot be proven from pixels alone.

The Computer Use accessibility summary collapses repeated spaces in these cases, while the direct AUV driver retains them. This identifies a usable verbatim source in the fixture, not a general exact-text guarantee for every Windows app. Providers may expose alternate, clipped, stale or incomplete accessible names; node selection and app-state verification remain necessary.

The generic installed AUV invoke CLI has no accessibility snapshot command. Existing app-specific Rust consumers use the driver snapshot; the prior model workflow cannot access this source through its current capture-only adapter. This probe does not establish model success, performance/token savings or an API support claim. No production runtime, CLI, API or global skill was changed. The standalone helper reuses current driver source but has its own Cargo lock; the new lock and build log are packaged.

| Case | ASCII space runs in note | Direct reads |
|---:|---|---:|
| 1 | [3, 2] | 3/3 |
| 2 | [4, 1] | 3/3 |
| 3 | [1, 3] | 3/3 |
| 4 | [2, 3, 2, 2] | 3/3 |
| 5 | [2, 3] | 3/3 |
| 6 | [2, 3] | 3/3 |

Evidence collection: original raw UTF-8 driver output is retained; JSON parsing preserves strings. Independent audit copies the complete original fixture log and reconstructs each session without trimming spaces. The coordinator's convenience event slices use trimEnd and lose final trailing spaces; those originals remain preserved and are not used to validate trailing whitespace. Auditing instead uses the original untrimmed app log. Only record navigation occurred; no saves or quantity edits. All native final frames were inspected.

Recommended next slice: provide a narrow, recorded read-only accessibility snapshot through the existing typed AUV execution path, preserving raw names/values and their source/window/node identity. The editing workflow should cite fresh verbatim data for exact notes and abstain when the provider cannot expose the requested text. Benchmark this consumer on fresh A/B subjects before claiming cost or reliability improvements. The snapshot already exists in the driver; no parallel UIA implementation is needed.

Evidence/source/hashes: outputs/windows-verbatim-probe.


## Recorded Windows accessibility text benchmark

Six fresh model subjects in three matched pairs; all retained without reruns or exclusions. Baseline uses mandatory visual zoom and the existing space-count serializer. Verbatim uses the new recorded window.accessibility read and a report helper that derives the note from a model-selected raw property, removing only an explicit exact label prefix. Both use the same new binary, model (gpt-5.6-sol/low), original full inline skill/operations reference, canonical joined image delivery and file keyboard transport. This compares complete read/report workflows, not an isolated prompt or zoom-policy change.

| Workflow | Median seconds | Median total tokens | Median uncached input + output | Exact answer | Save/reload | Complete |
|---|---:|---:|---:|---:|---:|---:|
| baseline | 104.59 | 620,115 | 46,035 | 1/3 | 3/3 | 1/3 |
| verbatim | 75.72 | 468,547 | 41,411 | 3/3 | 3/3 | 3/3 |

Verbatim median changes: -27.61% time, -24.44% total tokens, -10.04% uncached input plus output. Aggregate changes: -19.90%, -14.55%, +0.08% respectively. Paired wins: 3/3 time and 3/3 tokens.

Notes match [3,2], [4,1], [1,3] internal ASCII space runs. The first reproduces the previous visual failure. Both arms receive two A subjects and one B subject. B blocks its first save, requires review confirmation and commits once. Pair order alternates. Oracle and app state logs are withheld from subjects. Timer includes private Runner and model startup, all help/reference, actions/reads/captures, report serialization, final answer and owned cleanup. Resets and independent audits occur between timed subjects without overlap.

Audit verifies exact app-owned fields/whitespace, one committed save, B recovery and target navigation reload; complete inline skill/reference provenance; raw input/capture failures and keyboard files; receipt hashes; actual decoded model images; final visible native frames; report equality and latest source. Baseline zoom pixels are checked. Verbatim additionally checks the recorded JSON artifact against the raw CLI result and source hash, its selected window/node/property/prefix, and the note derived from that actual property. No expected text is in the adapter or report helper.

| Trial | Workflow | Case | Seconds | Total tokens | Exact | Complete |
|---:|---|---|---:|---:|---|---|
| 1 | baseline | A | 93.86 | 542,854 | False | False |
| 2 | verbatim | A | 75.72 | 468,547 | True | True |
| 3 | verbatim | B | 106.64 | 609,381 | True | True |
| 4 | baseline | B | 110.58 | 640,019 | True | True |
| 5 | baseline | A | 104.59 | 620,115 | False | False |
| 6 | verbatim | A | 65.17 | 462,774 | True | True |

Discrepancies:
- Trial 1: see preserved audit for failed checks and all operations.
  - note: reported `'Keep  double  spaces.'`; expected `'Keep   double  spaces.'`.
- Trial 5: see preserved audit for failed checks and all operations.
  - note: reported `'Keep double    spaces.'`; expected `'Keep double   spaces.'`.

Implementation: window.accessibility is available through the normal invoke registry (CLI/MCP), typed WindowService RPC and SDK WindowClient. It reuses the existing Windows UIA snapshot. Raw names/values remain untrimmed, including exposed empty values. Invoke records the exact direct result in an auv.window.accessibility JSON artifact. Other backends return unimplemented; depth/node bounds are reported and completeness_known remains false. Node geometry is excluded pending DPI coordinate validation. The original binary was preserved before building the new version; no upstream fetch, commit or push.

Validation: four invoke tests (provider text/optional values, missing identity, dry-run target validation, exact recording); one Runner missing/unknown-window test; 18 API-server tests; six report helper regressions; release build, cargo fmt and git diff checks. Final live local/Runner reads match and their artifacts preserve the previously failed text. An initial check used a wrong window reference and failed safely; its output is retained separately from the six model subjects. Existing unrelated warnings remain.

This small exploratory cohort does not establish general Windows app text fidelity, model reliability or billing savings. Providers may expose incomplete/stale/alternate text. Paths are snapshot-local and consumers must verify the selected control/application state. Navigation reload does not prove crash/restart durability. The tested report helper is a task-scoped consumer; the general skill defaults were not changed.

Decision: retain the recorded verbatim read for this editing workflow: it preserved all three exact notes and reduced time and total tokens in all three matched pairs. Before changing general skill defaults, test other Windows providers and controls, including missing or stale accessibility text. Aggregate uncached input plus output was essentially unchanged (+0.08%); total-token reductions do not establish billing savings.

Evidence/source/hashes: outputs/windows-editing-verbatim.
