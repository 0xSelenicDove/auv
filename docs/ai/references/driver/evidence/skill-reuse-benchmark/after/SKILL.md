---
name: auv-computer-control
description: Incorporate moeru-ai/AUV into native desktop app control through its CLI or connected MCP for observation, targeted input, reusable operations, and verification. Use for native app automation, repeated desktop workflows, or explicit AUV requests.
---

# AUV computer control

Use AUV for native app operations when requested or when it supplies missing targeting, observation, or reusable actions. Prefer an available connector, web DOM, or native accessibility when it already covers the task. Preserve the user's app, window, machine, access restrictions, and outcome across backends; reobserve before reusing targets or coordinates.

## Start with the needed operation

Use a supplied executable or connected MCP directly. Locate `auv` only if its path is unknown. Check version for compatibility questions; run `doctor` for unknown readiness or a relevant failure, rather than routinely before a permitted observation. Successful operations establish readiness for their own capability, not every capability. Reuse checks within the session; stop on missing permissions instead of probing again.

Read only `auv invoke <command-id> --help` for the operation you need, once for this runtime. When the needed operations are already known, fetch their independent help pages together in one shell/tool call rather than separate model round trips. Keep discovery that depends on a previous result separate. Read `auv invoke --help` when the operation is unknown; use plugin discovery only when looking for a plugin. Avoid root help, full catalogs, and setup probes for a known core operation.

- Observe a known app/title with `window.capture`; inspect its returned artifact. Use `window.list` when selectors are missing or ambiguous, with `--target app:<id>` and `--title` where installed help supports filtering. Keep all matching candidates until disambiguated.
- For a text search through scrollable content, inspect `input.scrollUntil` help and use `--until 'text:<query>'` with a finite step budget. Keep steps below viewport height to avoid skipping lines. Inspect the returned final screenshot artifact (`purpose: auv.scan.scroll_until_final_capture`) to verify the visible result. Reuse that frame instead of another `window.capture` when no input or relevant UI change has occurred since the search. If the artifact/path is absent or the view may have changed, take a fresh capture. An OCR match or no-motion stop is not semantic success or exhaustive coverage.
- For a single scroll, inspect `input.scroll`. Use keyboard navigation when its focus requirements are established, rather than assuming app targeting focuses a control.

Use installed help and connected tool schemas as the contract; never invent command names, flags, or support. Read [references/operations.md](references/operations.md) only for setup, field editing, popups, plugins, or remote workflows that need its details. Do not install a wrapper, SDK, or daemon for a local core invoke.

## Act and verify

Use the cheapest sufficient live observation. Read exposed accessibility values or inspect one targeted image; avoid OCR searches for fields already visible. Input coordinates are logical points; capture/OCR pixels require conversion using returned bounds and scale. Reobserve after UI changes and backend switches.

Use `--compact-json` when help offers it, otherwise `--json`. Keep complete stdout, stderr, failures, verification fields, and artifacts; retain stdout on nonzero exits. Use a task-local `--store-root` when needed. Open screenshot artifacts by returned `file_path`, never a guessed path. Delivery/completion flags do not prove the user's outcome: independently verify changed state, field contents before submission, and the final result.

Choose the input policy deliberately. Foreground input may raise the app; background posting may not be consumed and must honor its stated fallback policy. After ineffective input, inspect partial progress, correct focus/target, and allow one recovery for simple navigation. If unresolved, use a permitted supported alternative or report failure. Never blindly retry insertion, submission, or effects that could duplicate work, grant permissions, bypass denied access, or change a remote task to local.

Reuse verified operations and session help; batch only actions safe without intermediate checks. Finish with the observed outcome, evidence, and any remaining failure. Measure total tokens and successful completion before claiming savings.
