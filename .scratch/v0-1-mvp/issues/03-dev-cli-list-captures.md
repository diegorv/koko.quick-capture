Status: ready-for-agent

# Dev CLI to list recent Captures

## Parent

[v0-1-mvp PRD](../PRD.md)

## What to build

A small developer-only command that opens the same SQLite store the running app writes to and prints the most recent Captures. This is the verification path for v0.1 since the app has no Inbox window yet. It is intentionally not packaged for end users.

Implementation is a separate Cargo binary (e.g. `src-tauri/src/bin/dev_list.rs`) that calls into the existing `store` module's `list` function. It must not duplicate SQL — it should consume the same public interface that the Tauri commands use, so any drift in the store schema is felt here too.

Output format: one line per Capture, newest first. At minimum: `<short-ulid>  <kind>  <created_at>  <payload preview>`. Payload preview is a truncated single-line rendering — for `Note`, the first 60 chars of `text`; for other kinds (which don't exist in this slice yet), a sensible kind-specific summary.

The command takes an optional `--limit N` flag (default 20).

The point of this slice is to close the v0.1 verification loop: open the app, capture a Note via the shortcut, run the CLI, see the row.

Tests (per ADR-0005):

- One integration test that runs the compiled binary against a fixture SQLite file seeded with a couple of Captures, captures stdout, and asserts the output contains one line per row in the expected `<short-ulid>  <kind>  <created_at>  <payload preview>` format. Also asserts that `--limit 1` truncates to a single line.

## Acceptance criteria

- [ ] A new Cargo binary exists alongside the Tauri app that compiles via `cargo build --bin dev_list` (or equivalent target name).
- [ ] Running it prints recent Captures from the same `captures.db` the app writes to, newest first.
- [ ] Default limit is 20; `--limit N` overrides it.
- [ ] Each line shows ULID, kind, `created_at`, and a payload preview.
- [ ] The binary calls `store::list` directly. No SQL strings appear in the CLI source.
- [ ] Integration test runs the compiled binary against a fixture DB and asserts output format and `--limit` behavior.
- [ ] Documented in the project README under a "Dev verification" or similar section.
- [ ] All checks green: `cargo test`, `cargo check`, `pnpm test`, `pnpm check`. Slice committed in one commit using Conventional Commits per ADR-0006.

## Blocked by

- 02-composer-note-tracer
