# Setup and operation examples

Inspected 2026-10-06 against upstream commit `f87265e4768e3456381d50ef24443e256f5315cd`. Released binaries may differ; installed help takes precedence.

Sources: [README](https://github.com/moeru-ai/auv/blob/f87265e4768e3456381d50ef24443e256f5315cd/README.md), [invoke help](https://github.com/moeru-ai/auv/blob/f87265e4768e3456381d50ef24443e256f5315cd/crates/auv-cli-invoke/src/help.rs), [input commands](https://github.com/moeru-ai/auv/blob/f87265e4768e3456381d50ef24443e256f5315cd/crates/auv-cli-invoke/src/commands/input.rs), [window commands](https://github.com/moeru-ai/auv/blob/f87265e4768e3456381d50ef24443e256f5315cd/crates/auv-cli-invoke/src/commands/window.rs), [result envelope](https://github.com/moeru-ai/auv/blob/f87265e4768e3456381d50ef24443e256f5315cd/crates/auv-cli-invoke/src/models/invoke_result.rs).

## Setup

Upstream's prebuilt macOS installation:

```sh
brew install moeru-ai/tap/auv
auv --version
```

Official releases include the signed helper. If needed on macOS 13+:

```sh
auv setup macos-helper install
auv setup macos-helper status
```

The user grants Accessibility for AX/input, Screen Recording for capture/OCR, and Automation for relevant activation fallbacks to the application launching AUV. This can be the agent's host rather than an interactive terminal. Restart that launcher after permission changes, then run `auv doctor` and `auv invoke app.probePermissions`. Helper setup can also require user approval of Background Items/Accessibility. Do not change OS permissions automatically.

Linux/Windows have native drivers, but individual core commands have narrower platform support. Follow upstream installation instructions and installed help rather than transplanting macOS selectors. Windows helper setup requires elevated PowerShell. Cargo/Nix builds do not embed the signed macOS helper. The inspected README lists a first-party Python SDK and AUV REPL as planned; do not assume they exist.

## Core examples

These demonstrate syntax; substitute identifiers/titles from live observations. Do not execute TextEdit examples as an incidental test on the user's desktop.

```sh
auv invoke window.list --json
auv invoke window.capture --target app:com.apple.TextEdit --title Untitled --json
auv invoke window.findText 'Search' --target app:com.apple.TextEdit --title Untitled --json
auv invoke input.focusText 'Search' --target app:com.apple.TextEdit --json
auv invoke input.typeText 'example query' --target app:com.apple.TextEdit --input-policy foreground-preferred --json
auv invoke input.pressKeys cmd a --target app:com.apple.TextEdit --json
auv invoke window.waitForText 'Ready' --target app:com.apple.TextEdit --title Untitled --json
```

Inspect help for wait limits and OCR candidate selection. `window.clickText` locates/clicks OCR anchors: disambiguate instead of blindly using candidate zero. `input.clickPoint` accepts logical coordinates or target-local normalized values where supported. A window target supported by keyboard commands is not proof that capture/OCR accepts it.

For ordered keyboard actions, inspect `input.keyboard --help`; `--actions` accepts tagged JSON actions. Limit batches to actions safe without intermediate verification; do not bundle submission after unverified field edits.

## Plugins, MCP, remote targets

- `auv plugin list` discovers `auv-*` executables on PATH. Inspect `auv <plugin-name> --help`; core installation does not guarantee app/game plugins are installed.
- The optional stdio MCP server uses command `auv`, args `["mcp", "serve"]`. Use the host's supported registration flow when integration setup is requested. Starting a server alone does not expose tools to an active chat.
- `auv devices list` discovers daemon-visible Devices. Root selection precedes the subcommand: `auv --device-id <observed-id> invoke display.list --json`. Preserve selection for every related call. Pairing/enrollment is separate setup.
- Inspect `auv run --help` for correlation/inspection and `auv runner --help` for persistent execution when needed. Do not start network listeners, enroll unlock credentials, or modify host configuration incidentally for local one-off operations.

## Field editing and popup evidence

An app/window target selects a recipient, not a text field. Focus the intended
control through supported AX `input.focusText` or an observed click; refocus
after clearing or changing views. Verify complete field contents before Return
or submission and verify the final state afterward. Change requested fields in
place; when replacing a value, preserve untouched content verbatim, including
whitespace and trailing newlines when exact preservation is requested.

If a changed search field has no visible results in app-only evidence, inspect
the display: menus and results may occupy separate popup windows. Map input
using returned display bounds and scale, then verify the selected view by title.
An absent popup in an app capture does not establish failed input.

Dry runs validate operations, not semantic outcomes or later readiness. For
repeated workflows, prefer an existing verified operation or plugin; otherwise
use the smallest task-scoped script or supported SDK with explicit targets,
bounded waits, semantic checks, and a stop on ambiguous partial effects. Keep
live observations at decisions; avoid blind click/sleep sequences. Do not fall
back from an unavailable remote Device to the local desktop.

## Reuse for repeated work

Prefer an already connected MCP for related operations; keep that server alive.
For an existing SDK workflow, retain its connection, client and routed Runner.
Call `startAuv()` once per owning host; `connect()` attaches to someone else's
existing daemon. Close your connection at completion and stop only a daemon you
started. Preserve the selected machine and reobserve stale app/window targets.

For repeated CLI work with an available daemon, discover its Devices once and
select the exact Device for every invocation. `AUV_ENDPOINT` selects the daemon
and routes live unqualified `invoke` through its Runner. Pin a known Device
to preserve the selected machine:

```sh
auv devices list --endpoint "$auv_endpoint" --json
AUV_ENDPOINT="$auv_endpoint" auv --device-id "$observed_device_id" invoke window.findText 'Search' --target app:com.apple.TextEdit --title Untitled --compact-json
```

Substitute the endpoint, exact observed Device ID, app, title and query for the
requested task. Confirm that Device belongs to the requested machine; `local`
means local to that daemon. Keep any existing `--run` selection. Use command help
once if syntax differs. The existing local Runner is created lazily and normally
remains available for five minutes after requests/Run attachments finish. Do not
create or stop a Runner for every call. Keep full failures and per-invocation
recording; connection failure must stop the workflow, without local fallback.

When repeated work justifies starting a daemon and the task authorizes it, use
one task-owned private Unix socket on macOS/Linux, `serve --listen`, a private
`--store-root`, and `--no-register`. Keep its process alive across operations,
count startup in timing, and shut it down gracefully once at completion. Use
installed platform help on Windows. Do not replace default discovery, add a
network listener or modify host MCP configuration incidentally. For one-off work
without a daemon, use the supplied direct CLI.

Keep captures in the owning Runner by reference when using the SDK: recognition
accepts a capture reference, and pixels are fetched only when needed. Do not
upload AUV-produced pixels back into OCR or reduce OCR resolution/settings for
speed. Reuse measured faster in the offline eight-row fixture pilot; that is not
a guarantee of model-token savings or of removing the intermittent first-call
stall. Measure successful completion, total tokens and startup-inclusive elapsed
time before claiming an end-to-end improvement.
