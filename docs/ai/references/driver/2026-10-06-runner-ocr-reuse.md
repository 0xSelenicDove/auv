# Existing Runner OCR reuse pilot

Classification: test-only and docs. Evidence level: offline macOS recognition
through the existing owner-only daemon and first-party local Runner. Reuse
preserved all fixture text and bounds and reduced three-call completion time in
the repeat pilot. Production behavior, OCR settings and the installed skill
were not changed. No daemon, cache, prewarming mechanism or public API was added.

## Protocol and results

The [registered protocol](evidence/runner-ocr-reuse/protocol.md) compares three
fresh daemon/Runner sessions with one OCR each (A) against one fresh session with
three calls using the same transport (B), in ABBA order. Both use the prior
[caller-owned offscreen fixture](evidence/retina-ocr-resolution/small-mixed.json):
1400×960 RGBA pixels, 700×480 logical bounds, eight English and mixed Chinese
rows at 8–14 logical points. The existing `RecognizeText` RPC retains the current
full-region enlargement and accurate multilingual recognition settings.

Every session uses a private Unix socket, private temporary store and
`--no-register`; the user's default daemon discovery is untouched. Startup
includes daemon spawn, socket readiness, API connection, lazy Runner creation,
health readiness and the initial Runner identity audit. Health does not invoke
OCR. Completion includes startup, all request uploads/RPCs, oracle checks and a
final identity audit. Teardown is reported separately. Fixture decoding and test
runtime setup precede both arms; this is an RPC lifecycle measurement, not full
model-session elapsed time. No test windows were opened or focused.

| Attempt/cohort | Mode | Startup, ms | Completion, ms | Teardown, ms |
| --- | --- | ---: | ---: | ---: |
| First A | Three fresh sessions | 486.07 | 26,805.98 | 14.08 |
| First B | One reused session | 43.06 | 1,063.31 | 4.78 |
| First B | One reused session | 43.16 | 1,080.47 | 5.00 |
| First A | Three fresh sessions | 126.05 | 1,410.06 | 14.31 |
| Repeat A | Three fresh sessions | 514.69 | 1,781.42 | 13.44 |
| Repeat B | One reused session | 43.10 | 1,051.06 | 4.68 |
| Repeat B | One reused session | 42.18 | 1,082.48 | 7.61 |
| Repeat A | Three fresh sessions | 128.48 | 1,394.58 | 14.17 |

All 24 calls preserved all eight exact rows after whitespace removal and their
logical bounding boxes: 192/192 row checks passed. The oracle allows 3-point
position and 6-point width tolerances. Every session retained the same Runner
resource and process ID before/after its calls and remained ready.

The first RPC took 25,471.41 ms; subsequent fresh-session RPCs in that attempt
took 419–430 ms. Reused sessions took 416–426 ms on their first OCR and about
300–308 ms on subsequent calls. The initial stall is retained in full. It does
not occur in every fresh process, so the large first-attempt aggregate advantage
must not be advertised as a universal speedup.

The identical repeat averaged 1,588.00 ms per three calls with fresh sessions
versus 1,066.77 ms with reuse: a 32.82% reduction including startup. The last
fresh/reused pair, where startup was already fast in both arms, differed by
about 22.4%. Teardown adds only 5–14 ms per cohort and does not reverse this
pilot's outcome. Reuse saves repeated startup and shows faster subsequent RPCs;
the test does not isolate transport reuse, native initialization, OS cache state
or host scheduling. This small sequential ABBA pilot is not a confidence
interval or general support claim.

## Decision and next boundary

Existing Runner reuse is a promising speed path with unchanged fixture accuracy.
It does not yet establish faster live scroll/search workflows, fewer model
calls, lower total tokens, or elimination of the intermittent 25-second stall.
The current direct CLI benchmark and this routed RPC pilot are different paths;
do not pool their timings or describe this as a CLI-versus-Runner result.

Candidate next slice: run a matched model workflow through the existing client
and reused Runner, retaining capture references and counting connection/startup
and total model tokens. Adoption requires equal semantic success and no token
regression. No new lifecycle mechanism is approved by this note.

## Reproduction and validation

The ignored macOS-only test `local_runner_ocr_reuse_profile` lives beside existing
owner-only daemon routing coverage in `crates/auv-cli/tests/root_cli.rs`. Copy
`small-mixed.png` and `small-mixed.json` from the prior fixture evidence into a
writable directory, then run:

```sh
AUV_OCR_RUNNER_PROFILE_ROOT=/absolute/task-directory \
  cargo test --release -p auv-cli --test root_cli \
  local_runner_ocr_reuse_profile -- --ignored --nocapture
```

It writes `runner-reuse-results.json`, checks accuracy after retaining all
cohorts, and shuts down each daemon with SIGINT. Preserve that file before a
repeat. [Raw attempts, summaries and hashes](evidence/runner-ocr-reuse/metrics.json)
include the initial slow call. The measured source subsequently received only
formatting and an equivalent named `RunnerPhase::Ready` assertion; both source
hashes and the measured executable hashes are recorded.

Validation: both ignored release profiles, normal root CLI tests, default
`cargo test`, `cargo fmt --check`, `cargo check`, `git diff --check`, and
`cargo run --quiet -- invoke --help`.
