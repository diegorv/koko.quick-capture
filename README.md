# quick-capture

Frictionless macOS capture inbox. Tauri 2 + SvelteKit + Rust.

See `CONTEXT.md` for the domain glossary, `docs/adr/` for architectural decisions, and `.scratch/v0-1-mvp/` for the active plan.

## Requirements

- macOS
- Rust toolchain (`rustup`)
- Node.js + pnpm 10

## Dev

```sh
pnpm install
pnpm tauri dev
```

## Build

```sh
pnpm tauri build
```

## Checks

```sh
pnpm test     # Vitest (Svelte components)
pnpm check    # svelte-check (types)
cargo test    # Rust unit + integration tests (run from src-tauri/)
cargo check   # Rust typecheck (run from src-tauri/)
```

Every slice must leave all four green before commit (see ADR-0006).

## Dev verification

v0.1 has no Inbox UI yet. The `dev_list` binary is the verification path: it opens the same SQLite store the app writes to and prints recent Captures, newest first.

```sh
cd src-tauri
cargo run --bin dev_list -- --limit 5
```

Each line has four columns, separated by two spaces:

```
<short-ulid>  <kind>  <created_at>  <payload preview>
```

- `<short-ulid>` — first 8 chars of the Capture's ULID.
- `<kind>` — one of `Link`, `Clip`, `Shot`, `File`, `Note`.
- `<created_at>` — ISO-8601 UTC timestamp.
- `<payload preview>` — single-line preview of the payload. For `Note`, the first 60 chars of the text with newlines escaped as `\n` and a `...` suffix if truncated.

Flags:

- `--limit N` — number of rows to print (default 20).
- `--db <path>` — read from a specific SQLite file instead of the default location. Mainly used by the integration test for a hermetic fixture DB.
