# Skill token correction evidence

Both arms use the identical f9abd0e5 release binary. `before/` is the skill
from dfa4abee; `after/` is the tested revision, matching the bundled and installed
skill byte-for-byte. The manifest and protocol were frozen before trial 01.

`metrics.json` retains all four provider usage totals, startup-inclusive elapsed
times, full-task audits, operation counts and original artifact hashes.
Matching prompts, answers, audits, 32 reported images and four independent
final-window images are included. Both model-selected routes remain in the
comparison; no failed attempt was replaced or pooled with previous pilots.
One control/A operation failed with System Events -600 and recovered; its cost
and full response are retained in `failed-operation-responses.json`.

The coordinator required each full oracle triple on one OCR row, ignoring
separator punctuation, and checked every image hash against its original
operation receipt. The publisher parses both compact and pretty JSON outputs.
`audit-images.swift` is the independent Vision reader; build it with
`swiftc audit-images.swift -o /tmp/auv-audit-images`. The oracle was withheld.
The CLI stream does not independently expose subject image-view tool calls.

Scripts preserve original local paths and require matching binaries, an
authorized Codex CLI and the existing synthetic fixture. Large binaries and raw
model transcripts remain local. Temporary authentication contents are excluded.
`SHA256SUMS` covers this pack except itself.
