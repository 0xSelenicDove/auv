# Stall fix: registered speed and token benchmark

Registered before trial 01, 2026-10-08. Test-only; no runtime or skill edits during measurement.
Four fresh isolated model sessions: before/A, after/A, after/B, before/B.
Frozen pre-fix 9eefd507 binary and skill versus f9abd0e5 binary and skill.
Same synthetic 180-row canvas, eight names per case, initial top viewport,
foreground uncovered window, gpt-5.6-sol low, Codex CLI version in manifest,
900-second timeout. Same task prompts as the preceding Runner evidence pilot,
with only binary, skill and scratch paths updated. No prestarted daemon supplied.
Subjects may choose direct CLI, private daemon or built-in MCP; count discovery,
startup, help/skill reading, recovery and screenshot inspection in elapsed time
and provider tokens. Expose full output and retain failed attempts without replacement.

Primary results: verified 8/8 exact records with saved screenshots and final target
visible, total input+output including cached input exactly once, and wall time
from model launch to exit. Secondary: uncached input+output. Setup/reset and
independent image auditing are outside measurement. All runs sequential; wait for
subject exit and stop only subject-owned processes before reset. No production
builds or independent OCR audits during measurement. No pooling with older pilots.

Auditing checks withheld oracle triples, original retained image pixels and an
independent final-window screenshot. Subjects must inspect evidence; model tool
image-view calls may not be independently exposed in the CLI JSON log.
Equal verified success is required for savings claims. Two cases are a small
synthetic pilot; route/model variability and macOS state remain confounds. Both
runtime and guidance changed, so improvements cannot be attributed to one alone.
No native or without-skill comparison arm is included in this before/after test.
Temporary authentication copies are private, removed after each trial and excluded
from published artifacts. No permission changes or other-app access allowed.
