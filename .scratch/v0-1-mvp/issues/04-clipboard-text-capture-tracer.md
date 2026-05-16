Status: ready-for-agent

# Clipboard text capture tracer

## Parent

[v0-1-mvp PRD](../PRD.md)

## What to build

The second end-to-end tracer: the user copies any text (CMD+C anywhere on macOS), presses `Ctrl+Opt+Cmd+C`, and a Capture lands in the store with the correct kind. URLs become `Link` Captures, other text becomes `Clip` Captures. No window is shown; the only feedback is the row appearing in the store (verifiable via the dev CLI from slice 03).

This slice introduces:

- The `clipboard` module: a trait with a single method `read() -> ClipboardSnapshot`, where `ClipboardSnapshot` is a sum type with at least a `Text(String)` variant in this slice. The other variants (`Image`, `Files`) are declared but unimplemented stubs — slice 05 fills them in. A real implementation is backed by `arboard` or `tauri-plugin-clipboard-manager` (choose whichever has cleaner Mac support; both are acceptable). A fake implementation lives in tests.
- The `kind_detect` module: a pure function `decide(snapshot: ClipboardSnapshot) -> CaptureInput`. For this slice it handles only the text path: URL-looking text becomes a `Link` payload (`{ url, raw_text, title: None }`), everything else becomes a `Clip` payload (`{ text }`). URL detection is a regex; treat `http(s)://`, `mailto:`, and bare `www.` as URLs. Lift `title` extraction is out of scope here — it stays `None`.
- A new entry on the `commands` module: `capture_clipboard_now() -> Result<Capture>` that composes `clipboard::read` -> `kind_detect::decide` -> `store::save` and returns the saved Capture.
- A second global shortcut registration in `shortcuts`: `Ctrl+Opt+Cmd+C` fires `capture_clipboard_now` directly (no window).

Failure modes (empty clipboard, unsupported snapshot variant) return an error from the command that is logged and otherwise silenced for v0.1.

Tests (per ADR-0005):

Rust:

- `kind_detect` unit tests for the text branches: `https://...`, `http://...`, `www.example.com`, `mailto:a@b.com`, plain text, empty string, whitespace-only.
- An integration test for `capture_clipboard_now` using the fake `clipboard` implementation: feed each kind of text snapshot, assert the resulting row in a temp SQLite store has the correct kind and payload.
- A `commands::capture_clipboard_now` failure-path test: empty clipboard returns an error and writes no row.
- `shortcuts` intent-registry test: after this slice the registry contains both `open_composer` and `capture_clipboard` bindings.
- `clipboard` fake-implementation tests: confirm the fake correctly surfaces the `Text` variant fed into it.

## Acceptance criteria

- [ ] Pressing `Ctrl+Opt+Cmd+C` reads the current clipboard text and saves a Capture without opening any window.
- [ ] Clipboard text matching a URL pattern produces `kind = 'Link'` with `payload.url` and `payload.raw_text` set; non-URL text produces `kind = 'Clip'` with `payload.text` set.
- [ ] Empty clipboard or unsupported variants produce a logged error and no row.
- [ ] All clipboard reads in the codebase go through the `clipboard` trait. No direct `arboard`/`tauri-plugin-clipboard-manager` calls live in `commands` or anywhere else.
- [ ] `kind_detect::decide` is a pure function with no I/O.
- [ ] `kind_detect` unit tests cover every text branch listed above.
- [ ] `capture_clipboard_now` integration test using the fake clipboard exercises each text branch end-to-end into the store.
- [ ] `capture_clipboard_now` failure-path test covers empty clipboard.
- [ ] `shortcuts` intent-registry test now asserts both bindings (`open_composer`, `capture_clipboard`) are registered.
- [ ] All checks green: `cargo test`, `cargo check`, `pnpm test`, `pnpm check`. Slice committed in one commit using Conventional Commits per ADR-0006.

## Blocked by

- 02-composer-note-tracer
