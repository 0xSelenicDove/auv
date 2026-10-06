# Compact invoke JSON

`auv invoke <command> --compact-json` renders the existing JSON envelope on one
line with a trailing newline. It implies JSON output and can also accompany
`--json`. Ordinary `--json` remains pretty printed; human output is unchanged.

This approved feature is a presentation-only optimization in `auv-cli-invoke`.
It applies to local and Runner invokes through their shared result renderer.
No command is executed differently. Recording, exit status, and MCP output are
unchanged. The flag is not a command argument or replay input.

All values survive unchanged: targets, requests, text matches, whitespace inside
strings, delivery attempts, `verified: false`, diagnostics, failure codes and
partial progress, artifact metadata, URIs, and file paths. No filtering or
truncation is applied. Command-specific diagnostic projection is deferred until
a typed schema names which fields can be omitted; whitespace removal already
provides a measurable saving without losing evidence.

## Measurement

A temporary hermetic test harness exercised the actual invoke renderer with
four scroll-search fixture responses. Each pretty/compact pair parsed to
identical JSON. Random artifact UUIDs were normalized equally in both outputs.
Token counts use `tiktoken 0.14.0`, `o200k_base`.

| Response | Pretty tokens | Compact tokens | Reduction |
| --- | ---: | ---: | ---: |
| Match after scrolling | 839 | 553 | 34.09% |
| Match already visible | 727 | 482 | 33.70% |
| No visual progress | 763 | 507 | 33.55% |
| Budget exhausted | 763 | 508 | 33.42% |

These are response-text counts, not model billing or full-session usage. They
exclude tool arguments, chat framing, images, reasoning, cached input, discovery,
and repeated context processing. Input strings and semantic content are equal.
Different command payloads can have different savings. The earlier final-capture
optimization and these percentages must not be added together.

Local raw measurements and temporary harness are in
`docs/notes/token-benchmark/compact/`, `compact-results.json`, `compact_run.py`,
and `compact_count.py`; they are ignored scratch files, not published fixtures.

## Verification

Parser tests cover compact alone, both flag orders, unchanged `--json`, help,
and exclusion from operation inputs. Renderer tests compare complete parsed
success/failure envelopes and check a single line plus trailing newline.
Unicode and whitespace in strings, raw verification flags, fallback diagnostics,
and artifact paths are retained.

The broader invoke suite has an existing failure:
`registry_contract::every_registered_command_keeps_examples_with_its_typed_help`
expects `Examples:` but `input.holdKeys` emits `Example:`. It also fails on
unchanged commit `81c84fb9`; no unrelated help change is included here.

Checks run:

- `cargo test -p auv-cli-invoke`: 102 unit tests passed (1 live test ignored);
  integration suite reached the existing help failure above.
- `cargo test -p auv-cli-invoke -- --skip every_registered_command_keeps_examples_with_its_typed_help`:
  114 tests passed, 1 ignored, the known failing test excluded.
- `cargo fmt --all --check` and `git diff --check`: passed.
- `cargo run --quiet -- invoke --help`: passed; command help also exposes the flag.
- Built CLI `input.clickPoint 10 20 --relative-to screen --dry-run --compact-json`
  with a temporary store: completed, valid single-line JSON; no input delivered.
