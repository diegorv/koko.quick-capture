# 📥 Quick Capture

| | Status |
|---|---|
| **CI** | [![CI][ci-badge]][ci-url] [![Release][release-badge]][release-url] [![Nightly][nightly-badge]][nightly-url] [![Wiki Sync][wiki-badge]][wiki-url] |
| **Security** | [![Security][security-badge]][security-url] [![Privacy][privacy-badge]][privacy-url] |
| **Project** | [![Latest release][version-badge]][version-url] [![Platform][platform-badge]][platform-url] [![Claude Code][claude-badge]][claude-url] |

A frictionless macOS capture inbox built with Svelte 5 and Tauri 2

The app lives in the macOS menubar as an Accessory app - no system Dock icon by default - and only surfaces in Cmd+Tab while the Inbox window is open. Captures land in a single chronological Inbox with per-item read state. Built entirely with [Claude Code](https://docs.anthropic.com/en/docs/claude-code) and human review.

> [!WARNING]
> **Early stage project.** APIs, schemas, and behavior may change without
> notice. Not ready for production use. Expect bugs, breaking changes,
> and missing features.

## ✨ Features

### 🖊 Surfaces

Seven interaction surfaces. Details in [`docs/surfaces.md`](docs/surfaces.md).

| Surface | Description |
|---------|-------------|
| Composer | Borderless popover for free-text notes and voice recordings |
| Inbox | Split-pane main window: capture list + detail pane |
| Archive | Routed captures filtered by Destination |
| Recording | Dedicated recording UI with VU meters and live transcript |
| Dock | 96x96 always-on-top widget at bottom-left. Click, drop, badge |
| Tray | Menubar item (brain-circuit glyph). Open Composer/Inbox, Quit |
| Settings | Transcription config, update channels, destinations |

### 📎 Capture Kinds

Six kinds: **Note**, **Clip**, **Link**, **Shot**, **File**, **Transcription**.
Full table and source modules in [`docs/capture-kinds.md`](docs/capture-kinds.md).

### 🎙 Voice Recording

Full audio pipeline: mic + system audio capture, high-pass filter, denoising, loudness normalization, VAD (voice activity detection), resampling, and local Whisper transcription. Exports to M4A.

### 🔗 Integrations

- **Wikilink mentions** - `[[Name]]` autocomplete in Composer from a configured source folder; person-based filtering in Inbox/Archive
- **Destinations** - named routing targets with color swatches; captures can be routed to a Destination and browsed in Archive
- **Kokobrain** - deep-link forwarding via `kokobrain://capture` URI scheme (ADR-0012)
- **Drag & drop** - Finder file drops onto Dock widget auto-detect Shot vs File
- **Update channels** - stable/nightly in-app updater with runtime channel switching

### ⌨️ Global Shortcuts

| Shortcut | Action |
|----------|--------|
| `Ctrl+Alt+Cmd+Space` | Open / focus the Composer |
| `Ctrl+Alt+Cmd+C` | Capture from clipboard |
| `Ctrl+Alt+Cmd+I` | Open / focus the Inbox |
| `Ctrl+Alt+Cmd+A` | Open / focus the Archive |
| `Ctrl+Alt+Cmd+R` | Open / focus the Recording |

## 🛠 Stack

**Svelte 5** + **SvelteKit** + **TypeScript** | **Tauri 2** (Rust) | **SQLite** | **shadcn-svelte** (Tailwind v4)

## 🚀 Getting Started

**Requirements:** macOS (Apple Silicon or Intel), Rust toolchain (`rustup`), Node.js + pnpm.

```sh
pnpm install
pnpm tauri dev
```

Vite serves the frontend on `localhost:1420`; if the port is held by a previous dev process, kill it (`pkill -9 -f quick-capture`) or run on a different port.

### Build

```sh
pnpm tauri build
```

Produces `src-tauri/target/release/bundle/macos/quick-capture.app` and a `.dmg` under the same directory.

### Checks

```sh
pnpm test            # Vitest (Svelte components + routes)
pnpm check           # svelte-check (types)
cargo test --manifest-path src-tauri/Cargo.toml      # Rust unit + integration
cargo check --manifest-path src-tauri/Cargo.toml     # Rust typecheck
```

Every slice must leave all four green before commit (see ADR-0006).

### Dev Tools

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

## 📚 Documentation

| | Document | Description |
|---|----------|-------------|
| 📖 | [Domain Glossary](CONTEXT.md) | Core concepts and terminology |
| 🖥 | [Surfaces](docs/surfaces.md) | Surface behavior, keyboard shortcuts, read state |
| 📎 | [Capture Kinds](docs/capture-kinds.md) | Kind payloads, detection, global shortcuts |
| ✅ | [Verification Checklist](docs/verification-checklist.md) | Manual smoke test |
| 🏛 | [ADRs](docs/adr/) | Architectural decisions |
| 📝 | [Scratch](.scratch/) | Active PRDs and handoff docs |

<!-- ─── Badge reference definitions ────────────────────────────── -->

[ci-badge]: https://github.com/diegorv/koko.quick-capture/actions/workflows/ci.yml/badge.svg
[ci-url]: https://github.com/diegorv/koko.quick-capture/actions/workflows/ci.yml
[security-badge]: https://github.com/diegorv/koko.quick-capture/actions/workflows/security.yml/badge.svg
[security-url]: https://github.com/diegorv/koko.quick-capture/actions/workflows/security.yml
[privacy-badge]: https://github.com/diegorv/koko.quick-capture/actions/workflows/privacy.yml/badge.svg
[privacy-url]: https://github.com/diegorv/koko.quick-capture/actions/workflows/privacy.yml
[release-badge]: https://github.com/diegorv/koko.quick-capture/actions/workflows/release.yml/badge.svg
[release-url]: https://github.com/diegorv/koko.quick-capture/actions/workflows/release.yml
[nightly-badge]: https://github.com/diegorv/koko.quick-capture/actions/workflows/nightly.yml/badge.svg
[nightly-url]: https://github.com/diegorv/koko.quick-capture/actions/workflows/nightly.yml
[wiki-badge]: https://github.com/diegorv/koko.quick-capture/actions/workflows/sync-wiki.yml/badge.svg
[wiki-url]: https://github.com/diegorv/koko.quick-capture/actions/workflows/sync-wiki.yml
[version-badge]: https://img.shields.io/github/v/release/diegorv/koko.quick-capture?include_prereleases&sort=semver&label=release&color=blue
[version-url]: https://github.com/diegorv/koko.quick-capture/releases
[platform-badge]: https://img.shields.io/badge/platform-macOS-lightgrey?logo=apple&logoColor=white
[platform-url]: https://github.com/diegorv/koko.quick-capture#-quick-capture
[claude-badge]: https://img.shields.io/badge/built%20with-Claude%20Code-D97757?logo=anthropic&logoColor=white
[claude-url]: https://docs.anthropic.com/en/docs/claude-code
