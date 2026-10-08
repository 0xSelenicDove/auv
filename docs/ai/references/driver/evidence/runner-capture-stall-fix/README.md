# Runner capture stall evidence

`before.json` contains the eight-search competing-client reproduction and the
idle-owner peer capture failure. `after.json` contains the corrected build hash,
eight successful searches, 20 reused capture timings, and the competing-process
rejection. The before/after binaries are identified by SHA-256, not a release tag.

`search-01.webp` through `search-08.webp` are the original recorded synthetic
final frames. Their original hashes and independently read full oracle rows are
in `after.json`. No replacement screenshot was captured for verification.

`ocr-regression.txt` records the test-only red/green reproduction of a blocked
current-thread Runner. The native ownership and explicit-endpoint routing tests
are checked into their owning Rust modules. `validation.txt` records final
validation commands and results. `SHA256SUMS` covers this evidence pack.

These are native/CLI regression receipts, not a model-token benchmark. Normal
search latency was slower in the final probe; the evidence supports preventing
the reproduced conflicting-client stall, not an overall speed claim.
