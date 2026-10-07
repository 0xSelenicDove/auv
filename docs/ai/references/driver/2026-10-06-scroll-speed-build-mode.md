# Scroll speed: build-mode pilot

Classification: test-only and docs. Evidence level: four verified local AppKit
operation replays. The optimization objective is lower elapsed time with total
model tokens no higher than the current skill workflow and no reduction in
successful completion or screenshot verification.

The preceding [three-method pilot](2026-10-06-scroll-routing-benchmark.md) used a
debug binary. Before changing runtime behavior or guidance, this experiment
built the same implementation with
`cargo build --release -p auv-cli --bin auv`. It changed no skill, driver policy,
response format, navigation budget or verification requirement.

| Replay | Build | Seconds | Steps | Verified |
| --- | --- | ---: | ---: | --- |
| 1 | Debug | 19.513 | 22 | Yes |
| 2 | Release | 41.830 | 22 | Yes |
| 3 | Release | 17.046 | 22 | Yes |
| 4 | Debug | 43.565 | 22 | Yes |

Debug averaged 31.539 seconds; release averaged 29.438 seconds. The observed
6.7% mean difference is smaller than the variation within either build. This
does not establish a repeatable speed improvement, so release mode is not
adopted as the optimization on this evidence. No result was discarded.

The [protocol](evidence/scroll-speed-build-mode/protocol.md) was recorded before
replays. The order was debug/release/release/debug, with a reset top viewport
before every search and exactly one synthetic receiver and cover. Each replay
used background-only delivery, a 420 px step, 300 ms settling and a 40-step
budget to find `Kestrel handoff`. No benchmark model sessions ran. The binaries
returned identical semantic results, delivery metadata and final image bytes;
help for activation, capture and scroll-until also matched byte for byte.
Separate coordinator screenshots verified `VK-7392 / Ready for review` after
each replay. Both fixtures were closed, and window enumeration confirmed zero
remaining fixture windows.

[Metrics and checksums](evidence/scroll-speed-build-mode/metrics.json) preserve
all four results, executable hashes, parity checks and the replay script.
The implementation revision was `b29e50f7`; compilation completed successfully.
Model token usage was not measured in this experiment. Response parity cannot
prove unchanged model usage or end-to-end session speed.

Candidate next slice: measure input, capture, OCR, pixel comparison and encoding
durations separately in the existing scroll-until path before choosing a runtime
change. The current timings cannot assign the variance to any one stage. Keep
settling, step overlap and independent verification intact until their costs and
correctness boundaries are measured. Any resulting optimization should pass
both operation replay and matched model-session checks for speed, total tokens
and successful completion.
