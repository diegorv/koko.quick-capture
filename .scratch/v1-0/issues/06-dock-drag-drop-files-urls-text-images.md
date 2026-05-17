Status: ready-for-agent

# Dock drag-drop: Finder files (v1.0 scope)

## Parent

[v1-0 PRD](../PRD.md)

## What to build

Make the Dock accept Finder file drops. Drag any file(s) from Finder onto the Dock and one Capture per file lands in the store. Image-mime files become `Shot { source_path }`; everything else becomes `File { source_path, mime, original_name }`. No file is copied into `blobs/`.

URL, plain-text, and image-bytes drags from browsers are **not** handled in this slice (per the revised ADR-0008). The Dock JS registers no HTML5 `drop` listeners — only the Tauri-native channel is wired. Browser-sourced captures continue to flow through the existing `Ctrl+Alt+Cmd+C` clipboard shortcut.

### Rust additions

- New `drag_drop` module at `src-tauri/src/drag_drop/mod.rs`:
  - `pub fn decide_dropped_files(paths: Vec<PathBuf>) -> Result<Vec<CaptureInput>, DropError>` — pure, no I/O. Reuses the existing mime-based split (`kind_detect`-style) so image-mime files become `CaptureInput::Shot { source: ShotSource::Path { source_path, mime, width: None, height: None } }` and the rest become `CaptureInput::File { source_path, mime, original_name }`. Empty input -> `DropError::Empty`.
  - `DropError` enum kept minimal; `Empty` is the only variant the v1.0 surface produces.
- Tauri command:
  - `save_dropped_files_with_store(store, paths) -> Result<Vec<Capture>, String>` — composes `decide_dropped_files` + `store::save` per input. Tests drive this helper.
  - `#[tauri::command] save_dropped_files(paths: Vec<String>, app: AppHandle, store: State<'_, Store>) -> Result<Vec<Capture>, String>` — thin wrapper. Emits `captures.changed` for each saved Capture (matching the slice-02 pattern).
- `lib.rs` setup wires the Dock window's drag-drop handler. On `WindowEvent::DragDrop(DragDropEvent::Drop { paths, .. })` (or whatever the Tauri 2.11 callback variant is — verify via `WebviewWindow::on_drag_drop_event` or `Builder::on_window_event`), call into `save_dropped_files_with_store` against the managed `Store`, then emit `captures.changed` per row. The save must run on the main thread (mirror the existing `OpenComposer` / Tray pattern) so the SQLite write does not race with the Tauri event loop.

### SvelteKit additions

- `src/lib/dock/Dock.svelte` — gains an optional `dragActive: boolean` prop and a `.drag-active` class so the visual subtly "wakes up" while a file is hovering. The hover state is driven by the Rust side via `dock.drag.enter` / `dock.drag.leave` events emitted from the same Tauri drag-drop handler.
- `/dock/+page.svelte` — subscribes to `dock.drag.enter` / `dock.drag.leave` and toggles `dragActive`. No HTML5 `dragover` / `drop` listeners.

### Tests (per ADR-0005)

Rust:

- `src-tauri/src/drag_drop/mod.rs` unit tests (or `tests/drag_drop.rs`) for `decide_dropped_files`:
  - One image-mime path -> a single `Shot { Path }` input.
  - One non-image path -> a single `File` input.
  - Mixed list (image, non-image, image) -> three inputs in order, kinds matching their mimes.
  - Empty vec -> `DropError::Empty`.
- `src-tauri/tests/dock_drops.rs`:
  - `save_dropped_files_with_store` end-to-end against a temp `Store`: feed two paths, assert two rows persisted, kinds and payloads match.

Svelte:

- `src/lib/dock/Dock.test.ts` — extend (additive, don't delete slice-05 tests):
  - With `dragActive = true`, the widget has the `drag-active` class.
  - With `dragActive = false`, the class is absent.

No new page-level tests required; the Dock's drag-drop wiring is verified end-to-end by manual smoke (file drops require an OS drag gesture).

## Acceptance criteria

- [ ] Dragging one or more files from Finder onto the Dock saves one Capture per file with the right kind (`Shot` for image mimes, `File` otherwise) and `source_path` payload. No file is copied into `blobs/`.
- [ ] `save_dropped_files` emits one `captures.changed` event per saved row so the open Inbox prepends each row in real time.
- [ ] The Dock visibly "wakes up" while a file is being dragged over it and resets on drop or drag-leave.
- [ ] `drag_drop::decide_dropped_files` is pure (no I/O) and covered by unit tests for every branch.
- [ ] No HTML5 `drop` listeners exist on the Dock route (those land in a future slice once Tauri supports a custom drag-drop handler — see ADR-0008).
- [ ] All four gates green: `cargo check`, `cargo test`, `pnpm check`, `pnpm test`. Slice committed as one commit using Conventional Commits per ADR-0006.

## Blocked by

- 05-dock-window-scaffold-click-to-composer
