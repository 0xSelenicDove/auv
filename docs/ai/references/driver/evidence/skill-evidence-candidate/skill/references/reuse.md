# Reuse for repeated work


Prefer an already connected MCP for related operations; keep that server alive.
For an existing SDK workflow, retain its connection, client and routed Runner.
Call `startAuv()` once per owning host; `connect()` attaches to someone else's
existing daemon. Close your connection at completion and stop only a daemon you
started. Preserve the selected machine and reobserve stale app/window targets.

For repeated CLI work with an available daemon, discover its Devices once and
select the exact Device for every invocation. `AUV_ENDPOINT` selects the daemon
but does not by itself route an unqualified `invoke` through its Runner:

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
