# Filtered window discovery

Evidence level: hermetic command tests and live macOS discovery on a synthetic
fixture. Approved feature: reduce unrelated window output when a caller knows
the intended app or title. No production network lookup is needed.

`window.list` accepts the existing optional application target and `--title`:

```sh
auv invoke window.list --target app:local.auv.CanvasFixture --title 'AUV Canvas Ledger' --compact-json
```

Application identifiers match exactly, as in existing invoke selection.
On Linux this uses the AT-SPI AccessibleId exposed in `app_bundle_id`.
Titles use a case-sensitive substring, consistent with existing title selectors.
Both filters together use AND. Missing app/title metadata cannot match a
specified filter. An empty title is ignored, consistent with capture selection.
No filters preserves the existing result. No matches returns an empty list.

Every matching record retains all fields, ordering, and non-main windows;
discovery does not silently resolve one candidate. Human report counts and JSON
describe the same filtered set. CLI help documents the supported arguments and
provides a directly reusable example, avoiding guessed discovery syntax.

Local and Runner invoke share the filter in `auv-cli-invoke::commands::window`.
Drivers and the Runner WindowService continue enumerating their normal records;
this change reduces the invoke response, not backend enumeration or RPC traffic.
No parallel selector type, new dependency, cache, or schema was introduced.

For a target already grounded in current state, reuse it directly. List windows
only when identity or multiple-window ambiguity needs fresh discovery, and use
the known app filter then. Reuse installed command help within a session;
refresh window state after relevant UI changes. Do not infer semantic success
from a window record or input delivery.

## Measurement

Two adjacent live invocations returned 16 unfiltered windows versus one fixture
window. The matching record was identical in both responses. Full compact JSON
envelopes measured 4,696 versus 418 UTF-8 bytes and 1,402 versus 131 text tokens
with `o200k_base`: **90.7% fewer response text tokens** in this observation.
This is not a full-session model-token, billing, or latency saving. The benefit
depends on the number of unrelated windows. Unrelated desktop metadata and raw
results remain in ignored local notes; only these scalar measurements are
recorded here.

Validation: invoke unit and integration tests, `cargo check`, `cargo test`,
`cargo fmt --check`, `git diff --check`, root and command-specific help, and the
live filtered/unfiltered record comparison. The shared policy is covered
hermetically; a remote Runner was not live-probed. Existing live tests remain
ignored. No platform delivery or capture behavior changes.
