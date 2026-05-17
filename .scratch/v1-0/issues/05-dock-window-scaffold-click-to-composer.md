Status: ready-for-agent

# Dock window scaffold and click-to-Composer

## Parent

[v1-0 PRD](../PRD.md)

## What to build

Stand up the persistent Dock widget as a small, frameless, always-on-top, non-activating Tauri window pinned to the bottom-left corner of the active monitor. The Dock is a permanent part of the app session; it has no close button.

### Behavior in this slice

- Single click on the Dock body opens the Composer (same path the existing `OpenComposer` shortcut takes).
- Right click on the Dock surfaces a context menu with three items mirroring the Tray: Open Composer, Open Inbox, Quit.
- The window is non-activating: clicking it must NOT pull focus from whatever app the user is in. Use Tauri's `focus: false` plus `accept_first_mouse: true`, and set the window level to floating (`WebviewWindow::set_always_on_top(true)`).
- Auto-hide when the frontmost app enters fullscreen; reappear on exit. Subscribe to macOS workspace notifications (`NSWorkspaceActiveSpaceDidChangeNotification` or the screen-fullscreen-state APIs) via a small `objc2` wrapper.
- No drag-and-drop yet — that lands in slice 06.
- No badge yet — that lands in slice 07.

### Rust additions

- New Tauri window `dock` in `setup`: `WebviewWindowBuilder::new(app, "dock", WebviewUrl::App("/dock".into()))` with `decorations: false`, `resizable: false`, `skipTaskbar: true`, `alwaysOnTop: true`, `focus: false`, `acceptFirstMouse: true`, `visible: true`, `width: 80`, `height: 80`. Position computed at startup: `(screen.work_area.bottom_left.x + 16, screen.work_area.bottom_left.y - 80 - 16)`.
- Small `dock` Rust module owning the workspace observer: a `FullscreenObserver` struct that listens to macOS workspace notifications and emits `dock.fullscreen.entered` / `dock.fullscreen.exited` events. The Dock window subscribes and hides / shows itself.
- Right-click menu handler hooks the same intent registry as the tray (defined in slice 01).

### SvelteKit additions

- `src/routes/dock/+page.svelte` — minimal window root. Renders a small visual (initially just a circular gradient placeholder; final design polish OK in later slices) with click + contextmenu handlers wired to `invoke("open_composer_window")` and a contextmenu API call. NOTE: introduce a new Tauri command `open_composer_window` that mirrors the existing shortcut path so JS does not have to know about window handles.
- `src/lib/dock/Dock.svelte` — pure presentational widget component. Props: `onComposer: () => void`, `onContextMenu: (x, y) => void`. Click on the body calls `onComposer`. Right-click calls `onContextMenu`.

### Tests (per ADR-0005)

Rust:

- `dock` module: a small unit test on the fullscreen observer's event-to-Tauri-event mapping (the seam, not the OS hook).
- `commands::open_composer_window` test: assert it shows + focuses the Composer window (use a mocked `WindowManager`-like trait OR just assert the underlying helper is called — keep it pragmatic).
- Tray menu intent registry test from slice 01 should remain green.

Svelte:

- `Dock.test.ts`:
  - Mount the component. Click anywhere on the visible body -> `onComposer` is called.
  - Right-click -> `onContextMenu` is called with the event's `clientX/clientY`.

## Acceptance criteria

- [ ] On app launch a small bottom-left Dock widget is visible and stays on top of every other window.
- [ ] Clicking other apps does NOT cause the Dock to disappear or steal focus.
- [ ] Single-clicking the Dock opens the Composer window (same behavior as the `Ctrl+Alt+Cmd+Space` shortcut).
- [ ] Right-clicking the Dock shows a menu with Open Composer, Open Inbox, and Quit (each routes to the same intent as the Tray).
- [ ] When any frontmost app enters fullscreen the Dock hides; when it exits fullscreen the Dock returns.
- [ ] No drag-and-drop wiring in this slice (slice 06).
- [ ] All four gates green. Slice committed per ADR-0006.

## Blocked by

- 01-tray-icon-and-app-menu
