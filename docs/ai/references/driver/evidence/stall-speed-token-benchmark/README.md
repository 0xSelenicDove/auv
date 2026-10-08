# Stall fix speed and token evidence

`metrics.json` contains all four provider usage totals, elapsed times, success
audits, operation counts and original artifact hashes. `manifest.json` freezes
both binaries and skills; `protocol.md` was written before trial 01. Matching
prompts, complete answers, audits, reported screenshots and independent final
window images are retained. Both skill snapshots and the measurement/audit
scripts are included; large binaries and raw model transcripts remain local.
No authentication contents or copies are included.

`audit-images.swift` is the exact independent Vision OCR reader used for the
saved images; build it with `swiftc audit-images.swift -o /tmp/auv-audit-images`.
The coordinator additionally required each complete oracle triple to occur on
one OCR row and each reported image hash to match its original operation receipt.
`oracle.json` was withheld from the subjects. The JSON event stream does not
independently expose subject image-view tool calls; inspection is a subject claim.

The scripts retain the original local benchmark paths; reproduction requires
the matching binaries, synthetic fixture and an authorized local Codex CLI.
Temporary authentication copies created by the harness are removed at exit.
`SHA256SUMS` covers every file in this pack other than itself.
