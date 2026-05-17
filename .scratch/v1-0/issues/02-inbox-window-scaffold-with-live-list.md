Status: ready-for-agent

# Inbox window scaffold with live list

## Parent

[v1-0 PRD](../PRD.md)

## What to build

Stand up the Inbox window as a new Tauri window pointing at a new SvelteKit route `/inbox`. The window opens via any of: the new `Ctrl+Alt+Cmd+I` global shortcut, the Tray "Open Inbox" item (event from slice 01), or — eventually — the Dock right-click menu.

Inbox content in this slice is the **list pane only** with split-layout markup ready (detail pane stub on the right says "Select a capture"). The list shows every Capture in reverse-chronological order with cursor-based infinite scroll (50 per page), and it updates live when new captures land.

Star and delete icons render on each row but are no-ops (slice 03 wires them).

### Rust additions

- `store::list_before(cursor: Option<Ulid>, limit: u32) -> Result<Vec<Capture>, StoreError>` — query `WHERE deleted_at IS NULL AND id < cursor ORDER BY id DESC LIMIT limit`. When `cursor` is `None`, omits the `id < cursor` clause (first page).
- Tauri command `list_captures(cursor: Option<String>, limit: u32) -> Result<Vec<Capture>, String>` that calls into `store::list_before`. The CLI binary keeps using `store::list` — no need to update it.
- `lib.rs` registers the `OpenInbox` shortcut from `default_registry()` and dispatches it the same way `OpenComposer` is dispatched (run on main thread, show the Inbox window, set focus).
- After every successful `save_note`, `capture_clipboard_now`, and any future drop save in v1.0, emit `captures.changed` over the event bus with the new `Capture` as payload.
- New Inbox window created in `setup` via `WebviewWindowBuilder::new(app, "inbox", WebviewUrl::App("/inbox".into()))` with `visible: false`, `width: 900`, `height: 600`, `title: "quick-capture inbox"`. Listens for `tray.open_inbox` and shows itself.

### SvelteKit additions

- `src/routes/inbox/+page.svelte` — the window root. Manages list state, cursor, subscription to `captures.changed`. Lays out a CSS grid with a list pane and a detail pane stub.
- `src/lib/inbox/InboxList.svelte` — pure presentational component for the list pane: takes `captures: Capture[]`, `selectedId: string | null`, and callbacks. Renders each row with the kind icon, single-line payload preview (first ~80 chars of the kind-appropriate text), relative timestamp, plus a star icon and a delete icon (both render but emit no-op callbacks in this slice).
- Infinite scroll: when the list scrolls within ~100px of the bottom, fetch the next page using the last visible Capture's ULID as the cursor. Show a small spinner during the fetch.
- Live update: subscribe to `captures.changed`; on each event, prepend the Capture to the list (de-dup by id).

### Tests (per ADR-0005)

Rust:

- `store::list_before` integration test: seed 60 Captures across kinds; assert first page returns 50 newest in descending order; cursor-based second page returns the remaining 10; default ordering matches `list`.
- Command-level test for `list_captures` using a temp store + the existing pattern.
- Extended `shortcuts` test: registry now contains `OpenInbox` -> `Ctrl+Alt+Cmd+I` -> `open_inbox`.

Svelte:

- `InboxList.test.ts`: mount with a fixed array, assert one row per item rendered with the right preview; clicking a row calls injected `onSelect` with that row's id; star/delete icons render but their click handlers are present (no-op acceptable in this slice).
- A live-update test that fires a synthetic `captures.changed` event through the injected listener and asserts the list grows by one with the new row at the top.

## Acceptance criteria

- [ ] Pressing `Ctrl+Alt+Cmd+I` opens the Inbox window focused. So does the Tray "Open Inbox" item.
- [ ] The Inbox shows up to 50 Captures newest-first on open. Scrolling near the bottom triggers a second page fetch using the last item's id as cursor.
- [ ] Each row shows kind icon, payload preview, relative timestamp, and star + delete icons. Star/delete are visible but inert.
- [ ] Capturing from another flow (Composer / clipboard) while the Inbox is open prepends the new row in real time.
- [ ] `store::list_before` honors the cursor and the soft-delete filter (tombstones do not appear).
- [ ] All four gates green: `cargo test`, `cargo check`, `pnpm test`, `pnpm check`. Slice committed as one commit using Conventional Commits per ADR-0006.

## Blocked by

- 01-tray-icon-and-app-menu
