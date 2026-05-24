# quick-capture

> [!WARNING]
> **Early stage project.** APIs, schemas, and behavior may change without
> notice. Not ready for production use. Expect bugs, breaking changes,
> and missing features.

Frictionless macOS capture inbox. Tauri 2 + SvelteKit + Rust.

The app lives in the macOS menubar as an Accessory app - no system Dock
icon by default - and only surfaces in Cmd+Tab while the Inbox window
is open. Captures land in a single chronological Inbox with per-item
read state.

## Surfaces

Five surfaces. Details in [`docs/surfaces.md`](docs/surfaces.md).

| Surface    | Description                                                        |
|------------|--------------------------------------------------------------------|
| Composer   | Borderless popover for free-text notes and voice recordings.       |
| Inbox      | Split-pane main window: capture list + detail pane.                |
| Dock       | 96x96 always-on-top widget at bottom-left. Click, drop, badge.    |
| Tray       | Menubar item (brain-circuit glyph). Open Composer/Inbox, Quit.    |
| Settings   | Transcription config, updates, destinations.                       |

## Capture kinds

Six kinds: Note, Clip, Link, Shot, File, Transcription.
Full table and source modules in [`docs/capture-kinds.md`](docs/capture-kinds.md).

## Global shortcuts

| Shortcut              | Action                                |
|-----------------------|---------------------------------------|
| `Ctrl+Alt+Cmd+Space`  | Open / focus the Composer.            |
| `Ctrl+Alt+Cmd+C`      | Capture from clipboard.               |
| `Ctrl+Alt+Cmd+I`      | Open / focus the Inbox.               |

## Requirements

- macOS (Apple Silicon or Intel).
- Rust toolchain (`rustup`).
- Node.js + pnpm.

## Dev

```sh
pnpm install
pnpm tauri dev
```

Vite serves the frontend on `localhost:1420`; if the port is held by
a previous dev process, kill it (`pkill -9 -f quick-capture`) or run
on a different port.

## Build

```sh
pnpm tauri build
```

Produces `src-tauri/target/release/bundle/macos/quick-capture.app`
and a `.dmg` under the same directory.

## Checks

```sh
pnpm test            # Vitest (Svelte components + routes)
pnpm check           # svelte-check (types)
cargo test --manifest-path src-tauri/Cargo.toml      # Rust unit + integration
cargo check --manifest-path src-tauri/Cargo.toml     # Rust typecheck
```

Every slice must leave all four green before commit (see ADR-0006).

## Dev tools

**Icon generation:**

```sh
cargo run --manifest-path src-tauri/Cargo.toml --bin gen_icon
pnpm tauri icon src-tauri/icons/source-1024.png
```

**dev_list** - prints recent captures from SQLite:

```sh
cd src-tauri
cargo run --bin dev_list -- --limit 5
```

Output: `<short-ulid>  <kind>  <created_at>  <payload preview>`.
Flags: `--limit N` (default 20), `--db <path>`.

## Docs

- [`CONTEXT.md`](CONTEXT.md) - domain glossary
- [`docs/surfaces.md`](docs/surfaces.md) - surface behavior, keyboard shortcuts, read state
- [`docs/capture-kinds.md`](docs/capture-kinds.md) - kind payloads, detection, global shortcuts
- [`docs/verification-checklist.md`](docs/verification-checklist.md) - manual smoke test
- [`docs/adr/`](docs/adr/) - architectural decisions
- [`.scratch/`](.scratch/) - active PRDs and handoff docs
