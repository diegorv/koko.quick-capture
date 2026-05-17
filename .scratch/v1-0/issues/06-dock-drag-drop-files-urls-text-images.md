Status: ready-for-agent

# Dock drag-drop: files, URLs, text, images

## Parent

[v1-0 PRD](../PRD.md)

## What to build

Make the Dock the universal drop target promised by the product. Drag anything onto it from anywhere and the right Capture lands in the store. Per ADR-0008 this uses a two-layer approach (Tauri native file-drop + HTML5 WebView drop) with de-duplication.

### Capture mappings

- Finder file path (one or many) -> one row per path. Image mime -> `Shot { source_path }`. Else -> `File { source_path, mime, original_name }`.
- Browser address bar URL drop -> `Link`.
- Plain text drop -> URL-detect (same regex as slice 04). URL-like -> `Link`. Otherwise -> `Clip`.
- Image bytes dropped from a browser (`image/*` MIME on the `DataTransfer`) -> `Shot { blob_path }` (bytes flavor; persisted under `blobs/<ulid>.<ext>`).

### Rust additions

- New `drag_drop` module:
  - `pub enum DropPayload { Files(Vec<PathBuf>), Url(String), Text(String), ImageBytes { bytes: Vec<u8>, mime: String } }`
  - `pub fn decide_drop(payload: DropPayload) -> Result<Vec<CaptureInput>, DropError>` — pure, no I/O. Reuses `kind_detect::decide_files` for the files case and the URL regex from slice 04 for the text case. Returns `Vec` so multi-file drops stay one event.
- Tauri commands:
  - `save_dropped_files(paths: Vec<String>) -> Result<Vec<Capture>, String>` — wraps `decide_drop(DropPayload::Files(...))` + `store::save` per CaptureInput.
  - `save_dropped_url(url: String) -> Result<Capture, String>`.
  - `save_dropped_text(text: String) -> Result<Capture, String>`.
  - `save_dropped_image(bytes: Vec<u8>, mime: String) -> Result<Capture, String>`.
- Wire Tauri's native file-drop event on the Dock window. On `DragDropEvent::Drop { paths, .. }`, call `save_dropped_files` and ignore any HTML5 event that fires for the same drop within ~250ms (the de-dup window). The Dock window state holds a small `Mutex<Option<Instant>>` that records the last native drop.

### SvelteKit additions

- `/dock/+page.svelte` adds HTML5 `dragover` / `drop` listeners on the root element. On drop:
  - Inspect `DataTransfer.files` -> if any AND no recent native event, skip (Tauri layer handles files).
  - Inspect `DataTransfer.items` for `text/uri-list` or fallback `text/plain` -> classify and dispatch the right command.
  - Inspect for `image/*` items -> read bytes via `FileReader` / `Blob.arrayBuffer()` and call `save_dropped_image`.
  - `event.preventDefault()` on dragover so the WebView accepts the drop.
- The visual Dock widget grows / glows during a `dragover` event for visual confirmation; resets on `dragleave` / `drop`.
- After every successful drop save the Rust side emits `captures.changed` so the open Inbox updates and the Dock pulse fires.

### Tests (per ADR-0005)

Rust:

- `drag_drop::decide_drop` unit tests covering every payload variant: file paths (image vs non-image), URL string, plain-text URL vs Clip, raw image bytes -> Shot bytes.
- Integration tests for each of the four new commands against a temp store. For the image bytes case, build a tiny PNG in-memory via the `image` crate (already a dev-dep in slice 05 of v0.1).

Svelte:

- `Dock.test.ts` extended:
  - `dragover` -> visual "drag active" state class is applied.
  - `drop` with a synthetic `DataTransfer.files` -> the test injection layer skips the call (Tauri layer would handle).
  - `drop` with a synthetic `text/uri-list` -> `onDropUrl` callback fires with the URL string.
  - `drop` with `text/plain` URL -> `onDropUrl` fires.
  - `drop` with `text/plain` non-URL -> `onDropText` fires.
  - `drop` with `image/png` bytes -> `onDropImage` fires with the right `bytes` + `mime`.

## Acceptance criteria

- [ ] Dragging one or more files from Finder onto the Dock saves one Capture per file with the right kind and `source_path` payload. No blob copy for these.
- [ ] Dragging a URL from a browser address bar onto the Dock saves a `Link` Capture with the URL.
- [ ] Dragging selected text from any app onto the Dock saves a `Clip` Capture (or `Link` if the text is a URL).
- [ ] Dragging an image from a browser onto the Dock saves a `Shot` Capture whose `blob_path` references a file written under `blobs/<ulid>.<ext>` matching the dropped mime.
- [ ] When a Finder file drop fires on both the Tauri layer and the WebView layer, only one Capture is saved per file (de-dup wins for the Tauri layer).
- [ ] `drag_drop::decide_drop` is pure (no I/O) and covered by unit tests for every payload variant.
- [ ] All four gates green. Slice committed per ADR-0006.

## Blocked by

- 05-dock-window-scaffold-click-to-composer
