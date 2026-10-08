# Rejected focused reuse and screenshot guidance candidate

Classification: docs-only skill candidate and test-only validation. Evidence
level: one retained model session on repeated-search case B, with exact oracle
and original screenshot audit verifying all eight records and final target.

The candidate moved the existing reuse procedure from `operations.md` into a
2,485-byte reference, replacing a 7,937-byte read. It also explicitly preferred
returned final screenshot artifacts while preserving a fresh capture when a
Runner response supplied no image. The candidate was rejected and the bundled
and installed skill remain identical to the previous benchmark version.

| Case B, skill | Total tokens including cache | Seconds | Verified |
| --- | ---: | ---: | ---: |
| Previous fixed-version run | 455,518 | 129.0 | 8/8 |
| Candidate | 534,831 | 222.2 | 8/8 |

The candidate used 17.4% more total tokens and 72.3% more time. The subject read
both references despite the split, reset to the top for successive targets and
performed additional positioning and recovery. The result does not isolate the
causal effect of wording, but does not justify adoption as an optimization.
No favorable rerun substituted for this attempt. No native controls were rerun;
this is one follow-up, not a new three-way benchmark.

Same frozen binary SHA-256
`f837b61cf2d6000df7007262a7ee5463fefb53578768e5e3ee3b42ce546cfe5b`, fixture,
case B prompt except owned paths, model gpt-5.6-sol / low, Codex CLI 0.150.1 and
900-second limit as the [previous benchmark](2026-10-07-post-fix-benchmark.md).
Setup/reset and coordinator Vision image audit were outside timing; startup,
help, reading, discovery, recovery and verification were included. Audit shares
the Vision engine with AUV and is independent of command output, not OCR engine.
The owned test window was closed and temporary credentials removed. Raw logs and
stores remain local; only synthetic final-state pixels are published.

The inspection clarified a concrete boundary: local `input.scrollUntil` emits
`auv.scan.scroll_until_final_capture` from its exact final observation, while
`crates/auv-cli-invoke/src/runner.rs::execute_scroll_until` currently omits the
artifact, with an explicit deferral marker. Thus Runner case B's extra captures
were required for screenshot evidence, not simply model waste. No production
code or deferred evidence API was implemented in this experiment.

Next candidate slice: persist the final Runner capture reference through the
existing run artifact owner and return its artifact receipt to the invoke
frontend, without another capture or OCR pass and without sending pixels through
the client. This needs a shared recording contract and regression proving the
artifact is the matched observation before speed claims.

[Registration](evidence/skill-evidence-candidate/protocol.md),
[manifest](evidence/skill-evidence-candidate/manifest.json),
[metrics](evidence/skill-evidence-candidate/metrics.json),
[audit](evidence/skill-evidence-candidate/trial-04.audit.json),
[candidate skill](evidence/skill-evidence-candidate/skill/SKILL.md), and
[checksums](evidence/skill-evidence-candidate/SHA256SUMS).
