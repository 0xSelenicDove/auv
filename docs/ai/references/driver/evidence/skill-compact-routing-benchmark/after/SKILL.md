---
name: auv-computer-control
description: Use AUV CLI or connected MCP for native app observation, targeted input, reusable workflows, and verification; for native automation or explicit AUV requests.
---

# AUV computer control

Honor the user's app, window, machine, restrictions and outcome. Prefer an existing connector, DOM or accessibility when sufficient; use AUV when requested or for missing capabilities. Reobserve after backend switches.

## Discover only what is needed

Use the supplied executable/MCP; locate `auv` only if unknown. Check version for compatibility, `doctor` for unknown readiness or relevant failures. Reuse capability-specific readiness checks; stop on missing permissions.

Read only the needed `auv invoke <command-id> --help` once per runtime; batch independent reads. For known core operations, skip root help, full catalogs, setup probes and help for unused operations. Use `invoke --help` only for an unknown operation, plugin discovery only for plugins. Installed help/tool schemas define support: never invent flags or commands. Local core invokes need no wrapper, SDK or daemon.

- Known covered app: when foreground activation is allowed, inspect `app.activate` help and activate once before observation. Honor background-only/no-focus restrictions; avoid repeated capture/list attempts for known occlusion.
- Known app/title: `window.capture`, then inspect its returned artifact. Use `window.list` only for missing/ambiguous selectors; filter by `--target app:<id>`/`--title` if supported, retaining matches until disambiguated.
- Scrollable text search: `input.scrollUntil`, `--until 'text:<query>'`, finite step budget, steps below viewport height. Inspect screenshot: OCR matches/no-motion stops prove neither semantic success nor exhaustive coverage.
- Single scroll: `input.scroll`. Keyboard navigation requires established control focus; app targeting alone does not establish it.

Read [references/operations.md](references/operations.md) only for setup, editing, popups, plugins or remote workflows.

## Execute and verify

Use sufficient live AX values or a targeted image; avoid OCR for visible fields. Input coordinates are logical points: convert capture/OCR pixels using returned bounds/scale. Reobserve after UI changes.

Prefer supported `--compact-json`, otherwise `--json`; retain complete stdout/stderr, failures, verification and artifacts, including stdout on errors. Use task-local `--store-root` when needed. Open artifacts by returned `file_path`. Activation/input delivery/completion is not semantic success: independently verify changed state, fields before submission and final outcome.

Choose input policy deliberately: foreground may raise apps; background may be ignored and must honor fallback policy. After ineffective navigation, inspect partial progress and permit one focus/target recovery. If unresolved, use a permitted supported alternative or report failure. Never blindly retry insertion/submission/duplicate effects, grant permissions, bypass denied access or replace remote tasks with local ones.

Reuse verified operations/help; batch only actions safe without intermediate checks. Report observed outcome, evidence and remaining failures. Claim savings only from measured total tokens and successful completion.
