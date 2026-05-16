Status: ready-for-agent

# quick-capture v0.1 MVP — Clipboard + Composer

## Problem Statement

I capture too many things across the day on my work Mac (Chrome tabs, copied text, Slack thread links, screenshots, fleeting ideas) and have no fast, low-friction way to file them. Alfred and Raycast don't shape themselves around the capture flow I want, and I can't install Chrome extensions because the laptop is locked down. Today these references get lost in chat threads, sticky notes, or open browser tabs that I never come back to.

## Solution

A macOS-only desktop app that turns any in-flight clipboard content or quick thought into a saved Capture in under a second, without leaving the current app. Two global shortcuts cover the entire v0.1 flow:

- `Ctrl+Opt+Cmd+Space` opens a focused Composer window where I type a Note and hit Cmd+Enter to save.
- `Ctrl+Opt+Cmd+C` reads the current clipboard, detects what's in it, and saves it as the correct kind of Capture (Link / Clip / Shot / File) with no further interaction.

Captures land in a local SQLite store managed entirely by the Rust side of the app. The Svelte UI never sees SQL. The Dock widget, Inbox window, and drag-and-drop overlay described in the broader product vision are explicitly deferred to v1.0 — v0.1 is a tracer bullet through the write path only, verifiable by a small CLI script that reads the store.

## User Stories

1. As a user, I want to press a global shortcut from any app and immediately see a focused Composer window, so that I can write down a fleeting thought without switching contexts.
2. As a user, I want the Composer to open centered and frame-grabbing focus, so that I can start typing instantly without clicking.
3. As a user, I want pressing ESC in the Composer to close it without saving, so that I can abort a half-typed thought without leaving residue.
4. As a user, I want Cmd+Enter in the Composer to save what I typed as a Note Capture and close the window, so that the save action is a single keystroke.
5. As a user, I want the Composer to clear its text area between sessions, so that previous content never bleeds into a new capture by accident.
6. As a user, I want a second global shortcut that captures whatever is currently on my clipboard in one keystroke, so that I never need to open a UI when I have already copied something.
7. As a user, I want clipboard text that looks like a URL to be saved as a Link Capture, so that links stay distinguishable from plain text downstream.
8. As a user, I want clipboard text that is not a URL to be saved as a Clip Capture, so that arbitrary snippets are preserved verbatim.
9. As a user, I want clipboard images (e.g. from Cmd+Ctrl+Shift+4 screenshots) to be saved as a Shot Capture with the image bytes written to the blob directory, so that screenshots are preserved without me thinking about where they go.
10. As a user, I want clipboard file references to be saved as Capture rows pointing at the original file paths, with image mimes producing Shot and everything else producing File, so that drag-copied files are filed correctly without me classifying them.
11. As a user, I want every Capture to get a globally unique, time-sortable ULID, so that future export, sync, or processing tools can rely on stable identifiers and natural creation ordering.
12. As a user, I want Captures to be immutable after creation, so that I never have to reason about edit history; if I capture the wrong thing I delete and recapture.
13. As a user, I want the SQLite store and blob directory to live under the standard macOS app-data location, so that the app's data is sandbox-friendly and isolated from my documents.
14. As a user, I want clipboard capture to work without me having to grant per-app Automation permissions, so that the workflow does not break when I open a new app for the first time.
15. As a user, I want both default shortcuts to use a four-modifier prefix (Ctrl+Opt+Cmd), so that they won't collide with macOS or any other app I run.
16. As a developer of this app, I want a small CLI script that lists the most recent Captures from the store, so that I can verify v0.1 is saving rows correctly before any Inbox UI exists.
17. As a user, I want failed captures (e.g. clipboard empty, write error) to fail silently or with a minimal log line during v0.1, so that the MVP does not need to design an error surface yet.
18. As a user, I want the app to start a single instance on launch with both shortcuts registered, so that I do not have to think about whether the daemon is running.

## Implementation Decisions

### Stack and platform
- Tauri 2 with a SvelteKit frontend built via `@sveltejs/adapter-static` (see ADR-0007). macOS only. No multi-OS scaffolding in v0.1.
- Rust owns SQLite, clipboard, blob filesystem, ULID generation, and global shortcut registration. The Svelte side calls Rust via `invoke`. See ADR-0004.
- Multiple Tauri windows (Composer in v0.1; Dock, Inbox, Settings later) all load the same built SvelteKit bundle and point at different routes (`/composer`, `/inbox`, etc.).

### Module layout (Rust)

- **`store`** — the only thing in the codebase that talks to SQLite or to the blob directory. Interface:
  - `save(input: CaptureInput) -> Capture`
  - `list(limit: u32) -> Vec<Capture>`
  - `set_star(id: Ulid, starred: bool)`
  - `soft_delete(id: Ulid)`
  
  ULID assignment, schema migrations, and blob path resolution are internal. `list` is included now so the dev CLI script (story 16) has something to call; star/soft-delete are stubbed against the schema even though no UI exercises them in v0.1, so they don't force a migration when v1.0 adds the Inbox.
- **`kind_detect`** — pure function `decide(snapshot: ClipboardSnapshot) -> CaptureInput`. No I/O. Detects URL via regex against the text variant. Maps file mime types to `Shot`/`File`.
- **`clipboard`** — trait with one method, `read() -> ClipboardSnapshot`. One real implementation backed by `arboard` (or `tauri-plugin-clipboard-manager`, decision deferred to the implementing slice). Fake implementation lives in tests.
- **`shortcuts`** — registers the two default shortcuts on app startup via `tauri-plugin-global-shortcut` and emits events `open_composer` and `capture_clipboard`.
- **`commands`** — thin Tauri command surface. v0.1 exposes:
  - `save_note(text: String) -> Result<Capture>`
  - `capture_clipboard_now() -> Result<Capture>`
  - `list_captures(limit: u32) -> Result<Vec<Capture>>` (used by the dev CLI; UI access optional)
  
  Each command composes `clipboard`, `kind_detect`, and `store`. No business logic lives here.

### Module layout (Svelte)

- **`composer-view`** — a single textarea window. ESC closes the window (cancel). Cmd+Enter calls `save_note` and closes on success. The textarea is autofocused on window show. No history, no list, no preview.
- **`app-shell`** — Rust-event listener. On `open_composer`, shows the Composer window. On `capture_clipboard`, calls `capture_clipboard_now` directly (no window shown). Triggers a system-level confirmation only in dev (e.g. log line) — no toast UI in v0.1.

### Data model

A Capture row carries: `id` (ULID string, primary key), `kind` (`Link`|`Clip`|`Shot`|`File`|`Note`), `created_at` (UTC), `payload` (JSON blob shaped per kind), `source_app` (frontmost-app bundle id at capture time, nullable), `starred` (bool, default false), `deleted_at` (UTC, nullable; tombstone).

Per-kind payload shapes:
- `Link`: `{ url, title?, raw_text }`
- `Clip`: `{ text }`
- `Shot`: `{ blob_path, mime, width?, height? }`
- `File`: `{ blob_path | source_path, mime, original_name? }`
- `Note`: `{ text }`

For dragged-file Captures (`File`/`Shot` from non-clipboard sources in v1.0), we will copy the file into `blobs/`. In v0.1 the only `File`/`Shot` path is via clipboard, and clipboard images get persisted into `blobs/` by `store` on save. Clipboard file references are stored by path for now; the copy-vs-reference decision is revisited when drag-and-drop lands.

### Defaults and locations

- Default shortcuts: `Ctrl+Opt+Cmd+Space` → Composer, `Ctrl+Opt+Cmd+C` → clipboard capture. Both registered on launch; settings UI to rebind is out of scope.
- Storage: `~/Library/Application Support/com.koko.quick-capture/captures.db` and `~/Library/Application Support/com.koko.quick-capture/blobs/`.

## Testing Decisions

A good test in this project exercises a module through its public interface, asserts observable behavior (rows saved, kinds chosen, components rendered, events emitted), and never reaches into the SQL schema, file layout, or component internals. If a refactor that preserves behavior breaks a test, the test was wrong.

Policy (see ADR-0005): every Rust module and every Svelte component ships with tests in the same slice that introduces it. No manual-smoke exception for UI or thin wiring. End-to-end browser tests (Playwright) are deferred.

Tooling:

- Rust: stdlib `#[test]` for unit tests; integration tests under `src-tauri/tests/`.
- Svelte: Vitest + `@testing-library/svelte` for component tests.

Per-module test plan:

- **`kind_detect`** — pure unit tests covering every branch: URL-looking text, plain text, image bytes, file paths with image mime, file paths with non-image mime, empty snapshot.
- **`store`** — integration tests against a real SQLite file in a temp dir. Round-trip per kind (`save` -> `list` finds it with the right shape), `set_star` flips the flag, `soft_delete` removes the row from default `list` output but leaves the tombstone present in the DB. Blob round-trip for `Shot`: bytes saved, file exists at expected path, payload references it.
- **`clipboard`** — the trait is exercised via its fake implementation in tests. Real backend is verified by manual smoke (it talks to the OS).
- **`commands`** — integration tests compose the fake `clipboard` + real `kind_detect` + real `store` against a temp DB. Each command (`save_note`, `capture_clipboard_now`, `list_captures`, `star_capture`, `delete_capture`) has at least one happy-path and one failure-path test.
- **`shortcuts`** — extract an intent registry (mapping shortcut id -> command name) and test that. The real OS binding is verified by manual smoke.
- **`composer-view` (Svelte)** — Vitest + Testing Library. Asserts: textarea is autofocused on mount, `ESC` triggers a cancel event, `Cmd+Enter` calls the injected `save_note` handler with the textarea contents and emits a close event, textarea resets between mounts.
- **`app-shell` (Svelte)** — Vitest + Testing Library. Asserts: subscribing to the `open_composer` Rust event opens the Composer view; receiving `capture_clipboard` invokes the injected `capture_clipboard_now` handler and does not mount a window.

Dev CLI (slice 03) is a thin wrapper over `store::list`; it gets a single integration test that runs the binary against a fixture DB and asserts the output line format.

There is no prior art in this repo — quick-capture is greenfield. Test patterns will be set by the first slice that lands tests for `kind_detect` and `store`.

## Out of Scope

- The Dock widget (persistent bottom-corner drop target).
- The Inbox window (recent-captures list, star/delete UI).
- Drag-and-drop of files, images, URLs, or text from other apps.
- Screenshots captured via in-app shortcut (only clipboard-resident images in v0.1).
- Frontmost-app-aware capture (e.g. AppleScript-driven "grab Chrome tab"). See ADR / Q7: clipboard-based capture is sufficient.
- Settings UI for rebinding shortcuts, repositioning Dock, etc.
- Markdown export, iCloud sync, multi-device sync, processing pipelines.
- Any non-macOS platform.
- Error UI / toasts / notification surface.
- Auto-launch at login, menu bar / tray icon, fullscreen-aware hiding.

## Further Notes

- The whole v0.1 effort is a tracer bullet through Tauri 2 + Svelte + global shortcuts + SQLite. It is allowed to look unimpressive — the deliverable is the write path, not the UI.
- The `store` interface is deliberately complete (`set_star`, `soft_delete`, `list`) even though v0.1 only exercises `save` and the dev CLI. This avoids a migration in v1.0 when the Inbox lands.
- The `clipboard` trait split is the single load-bearing testability lever in the design. If we collapse it into direct OS calls inside `commands`, `capture_clipboard_now` becomes untestable without a real keyboard event, and we lose the only way to test the integration between detection and storage in isolation.
