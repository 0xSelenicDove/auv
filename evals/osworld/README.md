# OSWorld typed action adapter

This crate is the narrow boundary between OSWorld-style GUI actions and AUV.
It does not provision Kubernetes, run benchmark episodes, copy upstream
evaluators, or own task-specific policies. Those responsibilities stay with
OSWorld and the evaluation environment.

`auv-osworld-action` accepts either an operator-audited fixed plan or a JSONL
interactive session. Both paths use the same typed `ActionExecutor`, AUV Run,
input-delivery evidence, and artifacts.

## Fixed plan

```sh
cargo run -p auv-osworld-evals --bin auv-osworld-action -- \
  --plan /absolute/path/to/plan.json
```

The plan must contain an explicit AUV context and end with exactly one `DONE`
or `FAIL` signal:

```json
{
  "version": 1,
  "context": {
    "kind": "guest-local",
    "device_id": "observed-device-id",
    "daemon_endpoint": "unix:///run/user/1000/auv.sock"
  },
  "actions": [
    { "action_type": "CLICK", "x": 640, "y": 480 },
    "DONE"
  ]
}
```

The process writes Run and artifact evidence under the episode directory
provided by `AUV_OSWORLD_EPISODE_DIR`. It rejects shell commands, generic
execute actions, unsupported keys, invalid coordinates, empty delivery
evidence, and delivery results without a successful attempt.

## Interactive session

```sh
cargo run -p auv-osworld-evals --bin auv-osworld-action -- \
  --interactive --context /absolute/path/to/context.json
```

The process emits a `ready` JSON object and accepts ordered JSONL requests:

- `capture` records an AUV screenshot checkpoint.
- `action` delivers one typed GUI action.
- `finish` records the final screenshot and completes the Run.
- `abort` cancels the Run.

The caller owns model execution, OSWorld task setup, evaluation, scheduling,
and infrastructure lifecycle. The adapter deliberately exposes no Python or
Kubernetes framework of its own.

## Verification

```sh
cargo fmt --check
cargo test -p auv-osworld-evals
```

The ignored Xorg tests require an isolated desktop, an AUV daemon, and the
environment variables named in each test.
