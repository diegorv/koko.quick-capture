Status: ready-for-agent

# Dock pulse and unread badge

## Parent

[v1-0 PRD](../PRD.md)

## What to build

Add ambient feedback to the Dock: a pulse animation on every successful Capture save, and a small numeric badge whose value is the number of Captures created since the user last opened the Inbox. Opening the Inbox resets the badge to zero. The unread count must survive app restarts.

### Rust additions

- New tiny `app_settings` table created via a `Store` migration on first open:
  ```sql
  CREATE TABLE IF NOT EXISTS app_settings (
      key TEXT PRIMARY KEY NOT NULL,
      value TEXT NOT NULL
  );
  ```
- `store::settings_get(key: &str) -> Result<Option<String>, StoreError>`.
- `store::settings_set(key: &str, value: &str) -> Result<(), StoreError>`.
- Key used: `last_inbox_open_id` — a ULID string. The unread count is computed on demand: `SELECT COUNT(*) FROM captures WHERE id > :last_inbox_open_id AND deleted_at IS NULL`. Using a ULID instead of a timestamp avoids any clock-skew edge case across processes.
- New Tauri commands:
  - `unread_count() -> Result<u64, String>` — reads `last_inbox_open_id`, defaults to ULID min when missing, runs the count query.
  - `mark_inbox_opened() -> Result<u64, String>` — reads the highest existing capture id (via `Store::list_before(None, 1)`); writes it to `last_inbox_open_id`; returns the cleared count for telemetry / pulse-reset.
- After every successful save (Note, clipboard, dropped) Rust now emits two events: the existing `captures.changed` and a new `dock.pulse`. The Dock subscribes to both; pulse animates on `dock.pulse`, badge increments on `captures.changed`.
- The Inbox window's existing show flow calls `mark_inbox_opened` and emits `dock.badge.cleared`.

### SvelteKit additions

- `/dock/+page.svelte` on mount fetches the current unread count via `unread_count()`. Subscribes to `captures.changed` (increment), `dock.badge.cleared` (set to zero), `dock.pulse` (animate).
- Dock visual: badge component overlaid in the top-right corner of the Dock widget showing the number when > 0; hidden when 0. Pulse animation = brief scale + glow (CSS keyframes).
- `Dock.svelte` extended with two state-driven props: `unread: number`, `pulseKey: number` (parent bumps the key whenever a pulse should fire; the component triggers a CSS animation on key change).

### Tests (per ADR-0005)

Rust:

- `store::settings_get/_set` round-trip test in `tests/store.rs`.
- A test for the unread-count query: seed N captures, write `last_inbox_open_id` to the id of the (N/2)th, assert `unread_count()` returns N/2.
- `mark_inbox_opened` test: seed captures, call the command, assert `last_inbox_open_id` updated to the newest existing id and the returned cleared count matches.

Svelte:

- `Dock.test.ts` extended:
  - With `unread = 5`, the badge renders "5".
  - With `unread = 0`, the badge is hidden.
  - Bumping `pulseKey` toggles the animation class on the widget (assert via class presence within a `tick()`).

## Acceptance criteria

- [ ] On app launch the Dock shows the current unread count read from `app_settings.last_inbox_open_id` (or 0 if never set).
- [ ] Every successful Capture save pulses the Dock and increments the badge.
- [ ] Opening the Inbox window resets the badge to zero immediately AND persists the new `last_inbox_open_id`.
- [ ] Killing and relaunching the app preserves the unread count (it is computed on demand from the persisted id).
- [ ] Deleting a Capture does NOT subtract from the unread count (deletion is independent of triage).
- [ ] All four gates green. Slice committed per ADR-0006.

## Blocked by

- 05-dock-window-scaffold-click-to-composer
- 02-inbox-window-scaffold-with-live-list
