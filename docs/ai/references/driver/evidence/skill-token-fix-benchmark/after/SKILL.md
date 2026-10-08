---
name: auv-computer-control
description: Incorporate moeru-ai/AUV into native desktop app control through its CLI or connected MCP for observation, targeted input, reusable operations, and verification. Use for native app automation, repeated desktop workflows, or explicit AUV requests.
---

# AUV computer control

Use AUV for native app operations when requested or when it supplies missing targeting, observation, or reusable actions. Prefer an available connector, web DOM, or native accessibility when it already covers the task. Preserve the user's app, window, machine, access restrictions, and outcome across backends; reobserve before reusing targets or coordinates.

## Start with the needed operation

Use a supplied executable or connected MCP directly. Locate `auv` only if its path is unknown. Check version for compatibility questions; run `doctor` for unknown readiness or a relevant failure, rather than routinely before a permitted observation. Successful operations establish readiness for their own capability, not every capability. Reuse checks within the session; stop on missing permissions instead of probing again.

Read only `auv invoke <command-id> --help` for the operation you need, once for this runtime. Batch independent help reads. Read `auv invoke --help` when the operation is unknown; use plugin discovery only when looking for a plugin. Avoid root help, full catalogs, and setup probes for a known core operation.

- Use a supplied app/title and observed foreground state. Activate only when foreground input needs it or a covered app needs revealing, if permitted. Use `window.list` for missing or ambiguous selectors and retain matching candidates. An operation returning usable current pixels can supply the first observation; capture separately when choosing a target or input position requires it. Honor background-only and no-focus restrictions; activation does not prove success.
- For scrolling text search, read `input.scrollUntil` help and use `--until 'text:<query>'` with a finite budget and steps below viewport height. Continue from the observed viewport for related searches; reset only when navigation or recovery requires it. Verify the returned screenshot; OCR matches and no-motion stops do not prove semantic success or exhaustive coverage.
- For a single scroll, inspect `input.scroll`. Use keyboard navigation when its focus requirements are established, rather than assuming app targeting focuses a control.

Use installed help and connected tool schemas as the contract; never invent command names, flags, or support. Read [references/operations.md](references/operations.md) only for setup, field editing, popups, plugins, or remote workflows that need its details. Do not install a wrapper, SDK, or daemon for a one-off local invoke.

Reuse an already connected MCP/SDK or Runner. Set `AUV_ENDPOINT` and retain known Device/Run selection. Without an existing daemon, a bounded sequential CLI batch can run directly; start a Runner when capture-reference reuse, a required workflow or measured startup cost justifies it. Read [reuse details](references/operations.md#reuse-for-repeated-work) then. Count startup in timing; avoid competing capture clients.

## Act and verify

Inspect the returned `auv.scan.scroll_until_final_capture` artifact after each search. Reuse it when the full requested content is visible and the UI is unchanged. For a clipped row, make a small corrective scroll and repeat the same bounded search for fresh final evidence; do not restart from the top. Capture separately when usable evidence is missing. A text stop alone is not verification.

Use the cheapest sufficient live observation. Read exposed accessibility values or inspect one targeted image; avoid OCR searches for fields already visible. Input coordinates are logical points; capture/OCR pixels require conversion using returned bounds and scale. Reobserve after UI changes and backend switches.

Use `--compact-json` when help offers it, otherwise `--json`. Keep complete stdout, stderr, failures, verification fields, and artifacts; retain stdout on nonzero exits. Use a task-local `--store-root` when needed. Open screenshot artifacts by returned `file_path`, never a guessed path. Delivery/completion flags do not prove the user's outcome: independently verify changed state, field contents before submission, and the final result.

Choose the input policy deliberately. Foreground input may raise the app; background posting may not be consumed and must honor its stated fallback policy. After ineffective input, inspect partial progress, correct focus/target, and allow one recovery for simple navigation. If unresolved, use a permitted supported alternative or report failure. Never blindly retry insertion, submission, or effects that could duplicate work, grant permissions, bypass denied access, or change a remote task to local.

When a shell call yields a running session, wait on that session using the host's completion tool; do not poll artifact directories or replace the wait with sleep/probe calls. Wait before more input or recovery. If macOS capture ownership is busy, reuse the owning Runner; a second client can stall capture.

Reuse verified operations and session help; batch only actions safe without intermediate checks. Finish with the observed outcome, evidence, and any remaining failure. Measure total tokens and successful completion before claiming savings.
