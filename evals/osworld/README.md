# OSWorld evaluation harness

This package connects OSWorld evaluation episodes to AUV without routing GUI
input through OSWorld's generic command endpoints. It provides:

- a six-phase Kubernetes episode lifecycle (`boot`, `install`, `setup`,
  `action`, `evaluate`, `reset`);
- fixed-plan and interactive typed actions through one AUV Runner;
- V1 and selected V2 evaluator bridges;
- deterministic Chrome and VLC controllers used as scripted baselines;
- an append-only batch result ledger with per-phase output.

OSWorld remains the owner of task definitions and scores. AUV owns GUI input,
Run recording, captures, and input-delivery evidence. A successful input action
or screenshot is not a successful benchmark task.

## Development

The Python package and its native dependencies are managed with Pixi:

```sh
cd evals/osworld
pixi install
pixi run check
```

`pixi run check` formats, lints, tests, and builds the package. Rust checks run
from the repository root:

```sh
cargo test -p auv-osworld-evals
```

The ignored Xorg tests require an isolated desktop and a live AUV daemon.

## Typed action entry

`auv-osworld-action` accepts either a fixed action plan or a foreground JSONL
session. Both modes keep one AUV Run open and use the same `ActionExecutor`.

```sh
AUV_OSWORLD_EPISODE_DIR=/absolute/episode \
AUV_OSWORLD_ACTION_EVIDENCE=/absolute/episode/action_evidence.json \
  cargo run -p auv-osworld-evals --bin auv-osworld-action -- \
  --plan /absolute/plan.json
```

A plan contains a paired or guest-local AUV context and structured OSWorld
actions. It must end in exactly one `DONE` or `FAIL`. Python source, shell
commands, `EXECUTE`, unknown fields, and invalid coordinates are rejected.

Interactive mode uses a context-only file:

```sh
cargo run -p auv-osworld-evals --bin auv-osworld-action -- \
  --interactive --context /absolute/context.json
```

After the `ready` response, send ordered JSONL requests with consecutive `seq`
values. Supported operations are `capture`, `action`, `finish`, and `abort`.
The process records checkpoints, driver delivery results, the Run ID, and the
final screenshot in the episode directory.

## Agent relays

The paired and guest-local relays connect an operator-supplied JSONL proposal
stream to the interactive Rust entry. They do not call a model or select
actions. Each proposed action must refer to the latest capture; ambiguous
delivery closes the session without retrying the GUI operation.

```sh
pixi run auv-osworld-relay \
  --config /absolute/episode/config.json \
  --episode-dir /absolute/episode \
  --action-binary /absolute/auv-osworld-action \
  --action-sha256 SHA256 \
  --max-actions 32 --max-captures 32
```

Use `auv-osworld-guest-relay` for a guest-local owner socket. Relays are
attended transport boundaries: they do not prove that an external agent lacked
other tools.

## Kubernetes episodes

`auv-osworld-k8s` manages one episode from an explicit config. It uses the
official Kubernetes Python client for resource ownership, creation, discovery,
and deletion. Port forwarding still uses `kubectl`, because the client does
not provide the CLI's local forwarding process abstraction.

Generate a six-phase manifest, inspect it, then pass it to the batch runner:

```sh
pixi run auv-osworld-k8s manifest --config /absolute/config.json > manifest.json
pixi run auv-osworld-batch \
  --manifest /absolute/manifest.json \
  --output-dir /absolute/new-output-directory
```

The output directory must not already exist. The batch runner fixes the
denominator before executing the first phase, runs commands without a shell,
enforces phase timeouts, and always attempts reset. Kubernetes cleanup is
limited to resources whose recorded UID still matches the created resource.

The V1 typed-action, Chrome, and VLC adapters reuse this lifecycle. The V2
Task044 and Task099 adapters reuse it with their task-specific evaluator and
guest-image differences. Scripted controllers are deterministic baselines,
not autonomous-agent or benchmark-wide capability claims.

## Evaluators

The evaluator bridges expose only the setup and read paths needed by their
selected upstream tasks. They do not expose OSWorld `/execute` as a GUI input
path.

- `auv-osworld-v1-evaluator` handles the selected V1 Chrome/VLC tasks.
- `auv-osworld-v2-task044` handles the Shotcut file/launch task.
- `auv-osworld-v2-task099` handles the image-position file task.

Run `prepare`, perform GUI work through AUV, then run `evaluate`. Missing task
output may legitimately score zero; transport, schema, or evaluator failures
produce no score.

## Evidence boundary

The harness records process output, AUV Run IDs, typed `InputActionResult`
values, screenshots, checkpoints, and the upstream evaluator result. Hashes
bind files that cross a process boundary; they are not proof of semantic task
success, reproducible builds, agent isolation, or benchmark support.
