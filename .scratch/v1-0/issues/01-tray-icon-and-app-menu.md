Status: ready-for-agent

# Tray icon and app menu

## Parent

[v1-0 PRD](../PRD.md)

## What to build

Install a macOS menubar (tray) icon at app startup that offers three actions: **Open Composer**, **Open Inbox**, **Quit**. Selecting "Open Composer" shows the existing Composer window. Selecting "Open Inbox" emits a `tray.open_inbox` event (the Inbox window does not exist yet, so this slice stops at the event); slice 02 will pick it up. Quit exits the app cleanly.

This slice also extends the `ShortcutId` enum with `OpenInbox` and adds its binding (`Ctrl+Alt+Cmd+I`) to `default_registry()`, but the OS-level shortcut is NOT registered yet (slice 02 wires it). Putting the enum + registry change here keeps slice 02 from rewriting test fixtures.

Introduce a `tray` Rust module with a testable intent registry: `TrayMenuItem` enum + `default_menu()` that returns the three labelled items mapped to event names. The registry is the testable seam; `TrayIconBuilder` consumes it in `lib.rs`.

## Acceptance criteria

- [ ] App startup installs a tray icon visible in the macOS menubar.
- [ ] Tray icon uses one of the existing PNGs from `src-tauri/icons/` in template mode so it adapts to light/dark menubar themes.
- [ ] Clicking the tray icon opens the menu with three items in this order: Open Composer, Open Inbox, Quit.
- [ ] "Open Composer" shows + focuses the existing main window (same behavior as the `OpenComposer` shortcut).
- [ ] "Open Inbox" emits `tray.open_inbox` over the Tauri event bus. (Inbox window comes in slice 02.)
- [ ] "Quit" exits the app cleanly.
- [ ] `tray` module exposes `default_menu() -> Vec<TrayMenuBinding>`; integration test asserts the three items are present with the right event names.
- [ ] `shortcuts::default_registry()` now includes `OpenInbox` with accelerator `Ctrl+Alt+Cmd+I` and event `open_inbox`; existing shortcuts test extended to assert this binding.
- [ ] All four gates green: `cargo test`, `cargo check`, `pnpm test`, `pnpm check`. Slice committed as one commit using Conventional Commits per ADR-0006.

## Blocked by

None — can start immediately.
