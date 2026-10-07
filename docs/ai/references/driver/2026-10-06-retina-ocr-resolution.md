# Retina OCR resolution experiment: rejected

Classification: test-only and docs. Evidence level: offline native macOS OCR
against a generated 2×-density fixture. Removing the additional 2× enlargement
failed the accuracy gate. Production OCR, subregion behavior, recognition
settings, the installed skill and public APIs remain unchanged.

The candidate used the existing native full-image path without a crop, avoiding
the enlargement performed when a full-region crop is supplied. The baseline
used the current full-region crop path. Both received identical RGBA bytes and
used accurate recognition, language correction and the default language policy.
No Swift/FFI change or new option was introduced.

## Accuracy and timing

The fixture contains eight truth rows: English and mixed Chinese/English at
8, 10, 12 and 14 logical points. It is 1400×960 pixels with 700×480 logical
bounds and a nonzero screen origin. Text was checked exactly after whitespace
removal. Bounding boxes were checked against generated draw-row metadata in
logical screen coordinates, with 3-point position and 6-point width tolerances.
The [protocol](evidence/retina-ocr-resolution/protocol.md) preceded measurement.

| Attempt | Path | Milliseconds | Verified rows |
| --- | --- | ---: | ---: |
| First, A | Current enlargement | 25,384.05 | 8/8 |
| First, B | Native resolution | 266.29 | 7/8 |
| First, B | Native resolution | 247.24 | 7/8 |
| First, A | Current enlargement | 272.82 | 8/8 |
| Repeat, A | Current enlargement | 855.28 | 8/8 |
| Repeat, B | Native resolution | 259.36 | 7/8 |
| Repeat, B | Native resolution | 244.53 | 7/8 |
| Repeat, A | Current enlargement | 273.53 | 8/8 |

Every candidate call misread `中文交接 ZH10001 已完成`, the 10-point mixed row.
For example, it returned `#X ZH1001`, losing both the Chinese content and a digit.
All baseline calls preserved all eight truth rows and their logical bounds.
The result violates the requirement to preserve current recognition performance;
the candidate was rejected before any production or skill edit.

The first comparison asserted that both paths passed the accuracy gate and
failed. Its complete results are retained. The repeat used the same pixels,
oracle, settings, ABBA order and tolerances. The final opt-in diagnostic test
guards the existing baseline and reports the candidate's failed accuracy flag,
rather than asserting that a rejected candidate is production behavior.
It does not convert that failure into an adopted optimization.

The 25-second first call must not be attributed entirely to enlargement: later
enlarged calls took about 0.27 seconds. Comparing the first baseline call with
the subsequent candidate calls would confound resolution with first-use cost.
With only this fixture and a few calls, there is no general speed or recognition
support claim. No model-session benchmark ran after the accuracy failure, so
total tokens and end-to-end speed are unmeasured.

## First-call finding and next slice

Reexamining the existing [stage profile](2026-10-06-scroll-stage-profile.md)
shows that its large OCR variation was concentrated in the first call:

| Profile run | First OCR call, seconds | Remaining 22 calls, seconds |
| --- | ---: | ---: |
| 1, Debug | 24.983 | 4.639 |
| 2, Release | 24.898 | 4.393 |
| 3, Release | 0.268 | 4.392 |
| 4, Debug | 0.277 | 4.788 |

This establishes an intermittent first-call cost at the OCR boundary. It does
not distinguish Vision/model initialization, caching, image preparation, host
scheduling or another internal cause. It also does not establish that every
new process incurs that cost.

Candidate next slice: measure whether reuse through the existing long-lived
runner reduces repeated first-use costs while keeping OCR settings intact.
Include runner startup and preparation in the measurement; moving that cost
outside the clock is not a speed improvement. No new daemon, cache or warm-up
mechanism is approved or implemented by this note.

## Reproduction and validation

The [fixture, generator, oracle, both attempts and checksums](evidence/retina-ocr-resolution/metrics.json)
are retained. The opt-in test is
`session::tests::retina_ocr_enlargement_keeps_small_mixed_text_readable` in
`crates/auv-driver-macos/src/session_test.rs`. Copy `small-mixed.png` and
`small-mixed.json` from the evidence folder into a writable task directory, then:

```sh
AUV_RETINA_OCR_FIXTURE_ROOT=/absolute/task-directory \
  cargo test -p auv-driver-macos --lib \
  retina_ocr_enlargement_keeps_small_mixed_text_readable -- --ignored --nocapture
```

The test writes `small-mixed-results.json`. Its baseline assertion must pass;
inspect `candidate_text_and_bounds_verified` for the experiment's accuracy gate.
It is ignored normally and requires neither live windows nor app activation.
The fixture uses synthetic text and was rendered offscreen; no test windows
were opened during this experiment.

Validation includes the repeated native diagnostic, macOS driver unit tests
(111 passed, nine ignored), default `cargo test`, `cargo fmt --check`,
`cargo check`, `git diff --check`, and invoke help. The original candidate
accuracy-gate failure remains explicitly recorded above.
