Status: ready-for-agent

# Composer to Note Capture tracer

## Parent

[v0-1-mvp PRD](../PRD.md)

## What to build

The first end-to-end slice of the product: the user presses `Ctrl+Opt+Cmd+Space`, a focused Composer window appears, they type something, hit `Cmd+Enter`, and a Note Capture lands in the local SQLite store under the standard macOS app-data path. ESC dismisses the window without saving.

This slice introduces:

- The `store` module with its full public interface (`save`, `list`, `set_star`, `soft_delete`) and the SQLite schema for Capture rows. Only `save` is exercised in v0.1, but the schema must already support starring and soft-delete so later slices and v1.0 do not require a migration. ULIDs are assigned inside `store` using the `ulid` crate.
- The `shortcuts` module: registers the global shortcut on app startup via `tauri-plugin-global-shortcut` and emits an `open_composer` event when fired.
- The `commands` module exposing `save_note(text: String) -> Result<Capture>` over `invoke`.
- A Svelte `composer-view` window: autofocused textarea, `ESC` closes without saving, `Cmd+Enter` calls `save_note` and closes on success.
- An `app-shell` Svelte component listening for the `open_composer` event and showing the Composer window.

Storage location: `~/Library/Application Support/com.koko.quick-capture/captures.db`. The store creates the directory and file on first save if missing.

Capture row shape (per PRD): `id` (ULID), `kind`, `created_at`, `payload` JSON, `source_app` (nullable), `starred` (default false), `deleted_at` (nullable). For this slice, the only kind written is `Note` with `payload = { text }`.

Tests for this slice (per ADR-0005, every module ships tests in its slice):

Rust:

- `store` integration tests against a temp SQLite file: save a `Note`, list it back, flip its star, soft-delete it, confirm `list` no longer surfaces it while the row still exists.
- `commands::save_note` integration test against a temp store: invoking it persists a row with `kind = 'Note'` and the supplied payload; passing empty text returns an error and writes no row.
- `shortcuts` test against an extracted intent registry (mapping shortcut id -> command name). Assert the registry contains the `Ctrl+Opt+Cmd+Space` -> `open_composer` binding. The actual OS hook is verified by manual smoke.

Svelte:

- `composer-view` component test: mount it, assert the textarea is autofocused, `ESC` emits a cancel event, `Cmd+Enter` calls the injected `save_note` handler with the textarea contents and emits a close event, the textarea resets between mounts.
- `app-shell` component test: when the test fires a synthetic `open_composer` event on the injected event source, the Composer view becomes visible.

## Acceptance criteria

- [ ] Pressing `Ctrl+Opt+Cmd+Space` opens the Composer window from any frontmost app on macOS, with the textarea focused.
- [ ] Typing text and pressing `Cmd+Enter` saves a `Note` Capture to the SQLite store and closes the window.
- [ ] Pressing `ESC` closes the window without saving.
- [ ] On reopen, the Composer's textarea is empty.
- [ ] Capture rows have ULID primary keys, `kind = 'Note'`, a `created_at` timestamp, and a JSON `payload` containing the typed text.
- [ ] The `store` module exposes `save`, `list`, `set_star`, and `soft_delete`; all four compile and behave as described, even if only `save` is wired into the Composer flow.
- [ ] The DB file is created at `~/Library/Application Support/com.koko.quick-capture/captures.db` on first save, with parent directory auto-created.
- [ ] `store` integration tests cover: save + list round-trip for `Note`, `set_star` toggling, `soft_delete` hiding rows from default `list` while leaving the tombstone in the DB.
- [ ] `commands::save_note` integration test covers happy path and empty-text rejection.
- [ ] `shortcuts` intent-registry test asserts the `open_composer` binding is registered.
- [ ] `composer-view` component test covers: autofocus, ESC cancels, Cmd+Enter saves and closes, textarea resets between mounts.
- [ ] `app-shell` component test asserts that an `open_composer` event surfaces the Composer view.
- [ ] No SQL or filesystem access lives outside the `store` module.
- [ ] All checks green: `cargo test`, `cargo check`, `pnpm test`, `pnpm check`. Slice committed in one commit using Conventional Commits per ADR-0006.

## Blocked by

- 01-tauri-svelte-scaffold
