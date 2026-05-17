Status: ready-for-agent

# Inbox actions: star, soft-delete, full keyboard navigation

## Parent

[v1-0 PRD](../PRD.md)

## What to build

Wire the star and delete icons in the Inbox list pane to the store, and add full keyboard navigation. After this slice the Inbox is a real triage surface: user can move down the list with `↓`, star with `S`, delete with `Cmd+Delete`, jump to "Open" action with `Enter` (no-op for now — slice 04 implements the kind-specific Open behavior), and close the window with `ESC` or `Cmd+W`.

### Rust additions

- Tauri commands:
  - `star_capture(id: String, starred: bool) -> Result<(), String>` — composes `store::set_star`.
  - `delete_capture(id: String) -> Result<(), String>` — composes `store::soft_delete`.
  Each command emits `captures.changed` after success so all subscribers (Inbox list, future Dock badge) re-fetch or reconcile.
- Integration tests for both commands against a temp store.

### SvelteKit additions

- `InboxList.svelte` extended:
  - Star icon click toggles state, calls `onStarToggle(id, !current)`.
  - Delete icon click calls `onDelete(id)`.
  - Selection state: tracks `selectedId`. Clicking a row selects it.
  - Keydown handler on the list pane root:
    - `↑` / `↓` move selection up/down; loops not required (clamp).
    - `Enter` calls `onOpen(id)` (slice 04 will pass a real handler; pass a stub in this slice).
    - `S` calls `onStarToggle(id, !current)` for the selected row.
    - `Cmd+Delete` (or `Backspace+Meta`) calls `onDelete(id)` for the selected row.
    - `Cmd+W` and `Escape` call `onClose()`.
- `/inbox/+page.svelte` wires the callbacks to the new commands plus `getCurrentWindow().hide()` for close.

### Tests (per ADR-0005)

Rust:

- `commands::star_capture` test: star, list, assert flag set; unstar, assert flag cleared.
- `commands::delete_capture` test: delete, assert row no longer in `list_before` output but `find_with_deleted` still finds it (tombstone preserved).

Svelte:

- `InboxList.test.ts` extended:
  - Star icon click invokes `onStarToggle` with the right args.
  - Delete icon click invokes `onDelete`.
  - `↓` after mount moves visual selection from row 0 to row 1.
  - `Enter` on selection calls `onOpen`.
  - `S` toggles star on selection.
  - `Cmd+Delete` calls `onDelete` on selection.
  - `ESC` calls `onClose`.

## Acceptance criteria

- [ ] Clicking the star icon on a row immediately reflects in the UI and persists to SQLite.
- [ ] Clicking the delete icon removes the row from the list and writes a `deleted_at` tombstone in SQLite (visible via `Store::find_with_deleted` in tests).
- [ ] `↑` / `↓` move selection. `Enter` triggers the row's Open callback. `S` toggles star. `Cmd+Delete` soft-deletes. `ESC` and `Cmd+W` close the window.
- [ ] Every UI mutation goes through a Tauri command — no direct SQL or store access from JS.
- [ ] Both new commands emit `captures.changed` so any sibling subscriber stays in sync.
- [ ] All four gates green. Slice committed per ADR-0006.

## Blocked by

- 02-inbox-window-scaffold-with-live-list
