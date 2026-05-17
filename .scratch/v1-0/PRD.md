Status: ready-for-agent

# quick-capture v1.0 — Inbox + Dock + Tray

## Problem Statement

After v0.1 the write path is solid (Composer + clipboard shortcut, five Capture kinds, SQLite store), but the user has no way to *see* the captures without dropping to `sqlite3` or the `dev_list` CLI. There is also no passive surface for drag-and-drop captures — the user has to remember a keyboard shortcut for every save, even when their hands are already on the mouse moving a file from Finder.

## Solution

v1.0 adds three user-facing surfaces that round out the product into something the user can run as a daily inbox tool:

- **Inbox** — a standalone window that lists every Capture in reverse chronological order with a split layout (list left, detail right). Star, soft-delete, and "open source" actions live here. Live-updates as new captures land. Opens via `Ctrl+Alt+Cmd+I` or the Tray / Dock menu.
- **Dock** — a small, always-on-top, non-activating widget anchored to the bottom-left of the screen. Click opens the Composer. Anything dragged onto it (files, URLs, text, images from a browser) is saved as the appropriate Capture kind. An unread-since-last-Inbox-open badge surfaces ambient feedback.
- **Tray** — a macOS menubar item with the same three commands (Open Composer, Open Inbox, Quit) plus a stable "app is running" indicator.

After v1.0 the app is usable end-to-end without a terminal.

## User Stories

1. As a user, I want to press `Ctrl+Alt+Cmd+I` from any frontmost app and see my Inbox immediately, so that reviewing captures is one keystroke away.
2. As a user, I want the Inbox to show every Capture in reverse chronological order, so that the most recent thing I captured is at the top.
3. As a user, I want each Inbox row to show the kind, a one-line payload preview, a relative timestamp ("2m ago"), a star toggle, and a delete action, so that I can scan and act without opening anything.
4. As a user, I want clicking an Inbox row to select it and show the full payload in a detail pane to the right, so that I can read the whole thing without opening a separate window.
5. As a user, I want the detail pane to offer a kind-appropriate "Open" action: open `Link` URLs in my default browser, reveal `File`s and path-flavor `Shot`s in Finder, open bytes-flavor `Shot`s in Preview, and show `Clip`/`Note` text in a scrollable read-only area, so that the Capture remains a usable shortcut to the original thing.
6. As a user, I want to scroll past the first 50 Captures and load the next 50 transparently, so that older items are reachable without a hard cap.
7. As a user, I want to navigate the Inbox list with the arrow keys, press `Enter` to trigger the row's Open action, press `S` to toggle star, press `Cmd+Delete` to soft-delete, and `ESC` or `Cmd+W` to close the window, so that I never need the mouse.
8. As a user, I want the Inbox to update live when I capture something new from another app while the Inbox is open, so that I don't have to refresh.
9. As a user, I want a soft-delete to remove the row from the Inbox immediately but keep the tombstone in the store, so that no in-app action is unrecoverable at the data layer.
10. As a user, I want a small Dock widget pinned to the bottom-left of the screen at all times, so that I always have a passive drop target without summoning a window.
11. As a user, I want the Dock to never steal keyboard focus when I click around the screen, so that it never breaks my current app's flow.
12. As a user, I want clicking the Dock to open the Composer, so that I have a mouse-based path to start a Note Capture.
13. As a user, I want to drag any file from Finder onto the Dock and have it saved as a Capture (`Shot` for image mimes, `File` otherwise) with the file referenced by its source path, so that I capture without copy + shortcut.
14. As a user, when I drag a URL from a browser address bar, plain selected text, or an image, I currently use the clipboard shortcut (`Cmd+C` then `Ctrl+Alt+Cmd+C`) because the Dock's drop target only accepts Finder files in v1.0 (see ADR-0008 — Tauri's drag-drop API forces a single-channel choice; full multi-source drag is deferred until upstream allows a custom handler).
17. As a user, I want the Dock to show a pulse animation on each successful capture, so that I have visual confirmation without context-switching to the Inbox.
18. As a user, I want the Dock to show a small numeric badge whose value is the number of Captures created since I last opened the Inbox, so that I have an at-a-glance unread count. Opening the Inbox resets the badge to zero.
19. As a user, I want the badge to hide when the unread count is zero, so that the Dock is visually quiet when there is nothing to review.
20. As a user, I want the Dock to auto-hide when any frontmost app enters fullscreen and reappear when I exit fullscreen, so that the Dock never overlaps presentations, video, or games.
21. As a user, I want right-clicking the Dock to give me a menu with "Open Composer", "Open Inbox", and "Quit", so that I have a mouse path to every product surface.
22. As a user, I want a macOS menubar (tray) icon that contains the same three commands, so that I have a stable "the app is running" indicator and a quit path even if the Dock is auto-hidden behind fullscreen.
23. As a user, I want star and soft-delete actions to be reflected in the Inbox immediately and persisted to SQLite, so that the state I see is the state on disk.
24. As a user, I want the unread-since-last-Inbox-open count to survive an app restart, so that a relaunch does not silently reset my badge.

## Implementation Decisions

### Surface architecture

- Each surface (Composer, Inbox, Dock) is a Tauri window pointed at a SvelteKit route. Composer already exists at `/composer`; Inbox lands at `/inbox`, Dock at `/dock`. Routes share the same SvelteKit bundle (one Vite build) per ADR-0007.
- The Tray is built with `tauri::tray::TrayIconBuilder` and lives outside the SvelteKit bundle entirely (Rust-only menu).
- The main app window from slice 02 of v0.1 (currently hidden, pointing at `/composer`) keeps that role. Inbox and Dock are additional windows created at app startup with `WebviewWindowBuilder` so they exist for the life of the app.
- Per ADR-0004 every system / data operation goes through Rust. The Inbox and Dock JS subscribe to Tauri events and call `invoke` commands; they never query SQLite directly.

### Store changes

- New `store::list_before(cursor: Option<Ulid>, limit: u32) -> Vec<Capture>` for cursor pagination. ULIDs are time-sortable so `WHERE id < cursor ORDER BY id DESC LIMIT ?` is the only query needed; no separate index beyond the existing `idx_captures_created_at`.
- New `store::settings_get(key) -> Option<String>` / `store::settings_set(key, value)` backed by a tiny `app_settings(key TEXT PRIMARY KEY, value TEXT NOT NULL)` table created via migration on first open. Used to persist `last_inbox_open_at` so the Dock badge survives restarts. Stays in `store` to keep ADR-0004 in force.
- Captures still immutable per ADR-0003; only `starred` and `deleted_at` mutate.

### Commands

The Tauri invoke surface grows by exactly these:

- `list_captures(cursor: Option<String>, limit: u32) -> Vec<Capture>`
- `star_capture(id: String, starred: bool) -> ()`
- `delete_capture(id: String) -> ()`
- `mark_inbox_opened() -> u64` (returns the count cleared)
- `unread_count() -> u64`
- `open_in_browser(url: String) -> ()` (thin wrapper around `tauri-plugin-shell` or equivalent)
- `reveal_in_finder(path: String) -> ()`
- `open_blob(path: String) -> ()`

Each command composes the relevant store / shell / fs primitive and never embeds SQL.

### Live updates

Rust emits `captures.changed` (payload: the new Capture) on every successful `save` inside `commands`. The Inbox JS calls `listen("captures.changed", ...)` and prepends the row to its list. The Dock JS calls the same listener and increments the local badge state (plus pulses).

### Dock window

- Tauri window options: `decorations: false`, `alwaysOnTop: true`, `resizable: false`, `skipTaskbar: true`, `focus: false`, `visible: true`, `acceptFirstMouse: true`, `width: 80`, `height: 80`. Position set at startup to `(bottom-left + 16px margin)` using the active monitor's work area.
- Non-activating behavior on macOS: set `WindowLevel::Floating` (or higher) and rely on `focus: false` + `acceptFirstMouse: true`. Clicks open the Composer without bringing the Dock window to keyboard-key state.
- Fullscreen auto-hide: subscribe to macOS workspace notifications via `objc2` (or Tauri's `WebviewWindow::on_window_event` if the relevant variant is exposed) to detect fullscreen entry/exit on the frontmost screen; toggle `Dock.set_visible(false)` accordingly.

### Drag-and-drop

Tauri-native file drops only in v1.0 (see ADR-0008): the Dock window registers `WebviewWindow::on_drag_drop_event` (or the equivalent v2 API) and consumes the `DragDropEvent::Drop` event. Each dropped path becomes one Capture — image mime -> `Shot { source_path }`, else `File { source_path, mime, original_name }`. No file copies into `blobs/`. URL, plain-text, and image-bytes drag-drops are out of scope until Tauri exposes a custom drag-drop handler that lets HTML5 and native channels coexist; until then the clipboard shortcut covers those payloads.

### Tray

- `tauri::tray::TrayIconBuilder` registered in `setup`. Menu items: Open Composer, Open Inbox, Quit.
- Icon is a single PNG (already in `src-tauri/icons/`). Template-mode the icon so it adapts to the menubar theme.
- Selecting menu items routes through the same `app.run_on_main_thread` pattern used for the global-shortcut handler.

### Keyboard

The Inbox JS owns keyboard navigation. When a list-pane row is focused: `↑`/`↓` move selection, `Enter` triggers the row's Open action, `S` toggles star, `Cmd+Delete` soft-deletes, `Cmd+W` / `ESC` hide the Inbox window. Plain letter shortcuts intentionally have no modifier — there is no text input field in v1.0 Inbox. When a future search input lands, it must call `stopPropagation` on keydown.

## Testing Decisions

Same policy as v0.1 (ADR-0005): every Rust module and every Svelte component ships tests in the same slice.

- **`store::list_before`** — integration test seeding 60 rows, asserting first page = 50, cursor-based second page = remainder, ordering is descending by id.
- **`store::settings_get`/`settings_set`** — integration test: round-trip a key, overwrite, read.
- **`commands` additions** — each new command has at least one happy-path test composed against a temp store + (where relevant) a fake shell adapter for `open_in_browser` / `reveal_in_finder` / `open_blob`. Introduce a `Shell` trait + `FakeShell` mirroring the `Clipboard` pattern from slice 04 of v0.1.
- **`tray` module** — extract a menu intent registry (`TrayMenuItem` enum + handler mapping) and test the registry the same way `shortcuts` is tested. The OS-level menu is verified by manual smoke.
- **`drag_drop` module** (new) — pure decision function `decide_dropped_files(paths: Vec<PathBuf>) -> Result<Vec<CaptureInput>, DropError>` analogous to `kind_detect::decide`. Tests cover image-mime path, non-image-mime path, mixed list, and empty list. URL / text / image-bytes drag types are out of v1.0 scope per ADR-0008.
- **Svelte components**:
  - `Inbox.svelte` (list pane): mount with a fixed Captures array; assert it renders one row per item; `↑`/`↓` moves visual selection; `Enter` calls injected handler with the selected Capture; `S` calls injected `onStar` handler; `Cmd+Delete` calls injected `onDelete` handler.
  - `InboxDetail.svelte`: mount with each kind in turn; assert the appropriate "Open" affordance is rendered; clicking it calls the injected handler.
  - `Dock.svelte`: mount; assert clicking dispatches `onComposer`; right-click dispatches `onMenu`; the `drag-active` class toggles on the `dragActive` prop. (No HTML5 drop assertions — file drops arrive via the Tauri-native channel; see ADR-0008.)
- **Live update**: a thin integration test on the Svelte side that fires a synthetic `captures.changed` event through the injected event source and asserts the list grows.

Per ADR-0006, each slice runs `cargo check`, `cargo test`, `pnpm check`, `pnpm test` to green before its commit lands.

## Out of Scope

- Inbox search / filter (defer to v1.1).
- Dock drag-drop for URL, plain text, and browser image bytes (defer until Tauri exposes a custom drag-drop handler — see ADR-0008). File drops from Finder are in scope.
- Settings UI: Dock corner config, shortcut rebinding, default open-actions.
- Auto-launch at login.
- Drag-export FROM Dock to other apps.
- Toast notifications / in-app error UI.
- Markdown export, iCloud sync, multi-device sync.
- Title extraction for `Link` Captures.
- Width/height extraction for bytes-flavor `Shot`.
- Non-macOS platforms.

## Further Notes

- v1.0 still relies on manual smoke for the OS-coupled bits (global shortcut binding, Tauri window non-activating behavior, NSPanel-like Dock, fullscreen detection, system tray, native drag-drop). Tests cover the seams; the OS hooks themselves are verified by running the app.
- The two-layer drag-drop design (Tauri + HTML5) is the most architecturally novel part of v1.0; ADR-0008 records why it exists and what the de-duplication contract is.
- The Dock unread-since-last-Inbox-open count is computed on demand from the store (`SELECT COUNT(*) FROM captures WHERE id > last_seen_id AND deleted_at IS NULL`) rather than maintained as an incrementing counter, so it survives crashes and concurrent writes from sibling tools.
