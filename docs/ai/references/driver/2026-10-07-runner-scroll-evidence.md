# Runner scroll search final-observation evidence

Classification: owner-approved recording slice and regression fix. Evidence
level: synthetic-frame unit regressions, complete Rust workspace and SDK tests,
one successful final-release macOS background-only search, and one live
unavailable-recording-RPC check. No new total-token benchmark was run.

Recorded Runner `input.scrollUntil` calls now return
`auv.scan.scroll_until_final_capture`, as local calls already did. The frontend
retains the last streamed `CaptureRef` and explicitly asks the producing Runner
to record it. `RecordCaptureArtifact` reads that existing capture, prepares the
standard logical-resolution WebP and persists it through `auv-tracing` in the
host-configured store. Only metadata and the producer's path cross the client
boundary: no new platform capture, OCR pass or pixel download is needed to
obtain this evidence. A client cannot choose a destination filesystem path.

The receipt uses the invocation's canonical tracing Run UUID, independently of
the daemon's RunRef routing identity. The Rust capture client validates the
receipt's Run, purpose, digest and downscale metadata before attaching it.
The producer path is preserved when the frontend's store differs. A remote
producer path belongs to that machine, not the client. The API is gRPC-only,
explicit SDK recording; automatic invoke discovery remains intentionally
unimplemented until separately approved.

Dry runs and unrecorded operations do not request recording. Recording failure
emits the existing `auv.invoke.artifact_preparation_failed` event and preserves
the direct scroll result, following the
[direct-result contract](../../../TERMS_AND_CONCEPTS.md#direct-operation-result).
No input is retried to recover evidence. A missing or clipped screenshot still
requires separate semantic verification. Other capture/OCR command migrations
remain outside this slice, covered by their existing deferral marker.

## Verification

- Unit regression records a requested synthetic frame after a newer capture
  exists, checks logical pixels and source-scale metadata, and rejects unknown
  references and invalid Run identity.
- Receipt decoding preserves metadata and rejects another Run; compact invoke
  output preserves the producer path rather than substituting the client store.
- Rust workspace: **1,162 passed, 0 failed, 19 ignored**, across 139 targets.
  `cargo check`, `cargo fmt --check`, invoke help and `git diff --check` passed.
- The opt-in public CLI regression also passed against a private local daemon
  with different producer/frontend stores and background-only input.
- Buf lint, complete generation and breaking check against main passed. The
  touched schema's format check passed; the workspace-wide format diff also
  reports an existing option-order difference in vendor reflection source,
  which was left unchanged. SDK typecheck passed; **77 SDK tests passed, one
  skipped**. Generated bindings follow the repository's ignored-output policy.

The final-release live search used a private unregistered Unix daemon,
background-only wheel delivery and separate daemon/frontend stores. It stopped
after one step on `Birch transfer | BK-1600 | Stored` and returned a 900×682 WebP
with native 1800×1364 source metadata at scale 2. Its digest was verified against
the original bytes; the saved image visibly contains the complete matched row.
The screenshot exists in the daemon store and not at the corresponding frontend
artifact filename. Input evidence reports no focus or mouse disturbance; the
fixture was not explicitly raised. This is one fixture probe, not general
background-input support.

The final live call took **26.4 seconds**, including first-use Runner/capture/OCR
work but excluding daemon startup. It is not a before/after timing benchmark;
first-call stalls remain unresolved. A separate probe used the final frontend
with an older Runner lacking the recording RPC. Its scroll still completed with
`text_visible`, no screenshot receipt, and the explicit preparation-failure
event in frontend tracing. That probe took 26.9 seconds and establishes error
separation, not a compatibility guarantee or speed claim.

An initial development check incorrectly required the entire frontend Run
artifact directory to be absent. Frontend argument artifacts legitimately
create it. The check was corrected to assert absence of the specific screenshot
filename; the final-release check passed. The initial image was also verified,
but is not substituted for the final-release evidence.

The owned fixture and private daemons were closed. Existing user daemons and
the Homebrew installation were left untouched. The bundled and installed skill
now prefer the returned final screenshot when it is complete and current;
the rejected reference-splitting candidate was not reinstated.

## Evidence

[Manifest and validation](evidence/runner-scroll-evidence/manifest.json),
[final invoke response](evidence/runner-scroll-evidence/invoke.json),
[live check summary](evidence/runner-scroll-evidence/summary.json),
[final screenshot](evidence/runner-scroll-evidence/final.webp),
[unavailable-recording result](evidence/runner-scroll-evidence/recording-failure-invoke.json),
[error event](evidence/runner-scroll-evidence/recording-failure-event.json), and
[checksums](evidence/runner-scroll-evidence/SHA256SUMS).

The release binary SHA-256 is
`f2f63d5ab740a44760b192b9764dfc0c30b1b7362c84b5151354af6b06c09d7c`.
The change removes the need for a second capture after eligible Runner searches;
full-session token and elapsed-time effects remain unmeasured.
