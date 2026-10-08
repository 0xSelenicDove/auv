# Captured-window focus fix and repeated OCR routing

Classification: bug fix and narrow skill routing change. Evidence level:
reproduced local AppKit failure, opt-in regression, verified foreground
search/reset, and diagnostic runtime probes. No model-token benchmark ran.

## Reproduction and fix

Stage-specific native diagnostics identified the
[previous focus blocker](2026-10-08-post-sync-isolation.md): WindowServer put a
66×20 captured-window sharing badge before the fixture's standard window. Both
belonged to the fixture PID. AX reported the badge as `AXWindow/AXDialog`, modal
false, focused false; the target was `AXStandardWindow`, focused true. The old
first-layer-zero-window check rejected a correctly focused target.

The shared predicate now excludes a same-process auxiliary dialog only with
explicit non-modal and unfocused AX evidence. Different processes, other
standard windows, modal/focused dialogs and missing AX state still block
readiness. The exact AX focused-window check remains. AX enumeration is lazy,
only needed when an owned surface precedes the target. This is focus readiness,
not a guarantee of hit testing or semantic success.

`confirm_input_focus` now reports which stage failed rather than conflating app
activation, exact-window focus and WindowServer ordering. Temporary diagnostics
containing detailed window metadata were removed. No FFI or public schema changed.

The ignored regression
`foreground_preparation_confirms_repeated_search_window` failed before the fix
and passes after it. It also rejects a stale window number and rejects the
target after another fixture takes foreground ownership, then confirms the
target again. It briefly activates only the named synthetic fixtures.

The fixed release searched for `Birch transfer | BK-1600 | Stored` through
foreground input in **1.512 seconds**, then reset to top in **0.635 seconds**.
The complete triple was audited in the saved search image; fresh CUA AX state
confirmed scrollbar zero after reset. This is repaired fixture behavior, not
general application support or a speed comparison with a successful baseline.

## Recognition delay and routing

The existing release scroll profiler completed the expected 22-step
`Kestrel handoff | VK-7392 | Ready for review` search. Its **59.613-second** loop
included 0.018s input, 6.681s settling, 3.451s capture, 49.301s recognition and
0.161s remainder. The first OCR call took **39.310s**; subsequent calls took
0.432–0.602s. Native captures were 1800×1364 without fallback. This reproduces
the initialization-cost pattern in the
[earlier stage profile](2026-10-06-scroll-stage-profile.md), but does not separate
Vision initialization from scheduling or other recognition-boundary work.

Identical `window.findText` calls on an already-visible static row used
direct/Runner/Runner/direct order. The private Unix daemon used `--no-register`,
a private store and one observed local Device. Startup to Device discovery took
0.085s; lazy Runner startup remains in the first routed call's elapsed time.

| Attempt | Route | Seconds | Outcome |
| --- | --- | ---: | --- |
| 1 | Direct | 0.734 | Exact text and bounds |
| 2 | Reused Runner, first call | 0.767 | Same text and bounds |
| 3 | Reused Runner, second call | 0.601 | Same text and bounds |
| 4 | Direct | 40.090 | Native capture timeout; xcap fallback failed |

An earlier harness passed `--endpoint` at the root CLI incorrectly, causing two
argument-parser failures. That attempt is retained separately, not counted as
driver/performance evidence. Its direct calls succeeded at 25.801s and 0.725s.
The corrected harness uses `AUV_ENDPOINT` plus `--device-id`.

The skill now prefers existing persistent clients/Runners for repeated
capture/OCR and names both CLI selectors in its entrypoint. Its existing
reference covers Device selection and justified task-owned startup. The
installed Codex skill was updated to the same content and validated. This
uses existing execution paths; no cache, prewarming API, daemon mechanism or
reduced OCR setting was added. Startup must stay counted, and reuse must not
be described as eliminating every first-call stall.

## Limits and validation

The tested foreground rejection is fixed. Intermittent capture timeouts and
recognition stalls remain reproduced; their internal causes are not fixed by
this patch. Persistent routing avoids repeated process creation when followed,
but this small probe does not establish a general speedup or fewer total model
tokens. The negative
[full-session benchmark](2026-10-08-post-sync-benchmark.md) remains applicable
token evidence until another matched model benchmark runs. Capture resolution,
multilingual accurate OCR, language correction and verification are unchanged.

The [evidence pack](evidence/focus-badge-fix/protocol.json) retains all attempts,
failures, stage metrics, synthetic images and executable/skill hashes, covered
by [checksums](evidence/focus-badge-fix/SHA256SUMS). Private daemons were stopped,
the pre-existing ledger was restored to its initial top viewport, and the
task-owned repeated-search window was closed.

Validation: failing-then-passing live regression, foreground semantic checks,
release profiler, routed/direct probes, regenerated Swift bridge, touched-package
`swift build --build-system native`, skill validator, Rust workspace tests
(1,159 passed, zero failed, 18 ignored across 139 targets), `cargo fmt --check`,
`cargo check`, invoke help and evidence checksums. Live measurements ran separately from
other benchmarks and broad compilation/testing. Vision also powers the image
auditor, so that audit is independent of command results but not its OCR engine.

The fixed release executable is `target/release/auv` (v0.0.31), identified by
the evidence hash. The shell's Homebrew-managed `auv` remains v0.0.29; these
checks explicitly used the fork build, not that installation.
