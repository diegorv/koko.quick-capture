Status: ready-for-agent

# Inbox detail pane and Open actions

## Parent

[v1-0 PRD](../PRD.md)

## What to build

Replace the slice-02 detail-pane stub with a real per-kind detail view, and add the "Open" actions. After this slice every Inbox row gives the user a one-click path back to the source.

### Rust additions

Introduce a `Shell` trait + `SystemShell` real impl + `FakeShell` for tests (mirror the slice-04 `Clipboard` pattern). One method per Open action:

```
trait Shell {
    fn open_in_browser(&self, url: &str) -> Result<(), ShellError>;
    fn reveal_in_finder(&self, path: &Path) -> Result<(), ShellError>;
    fn open_path(&self, path: &Path) -> Result<(), ShellError>; // for bytes-flavor Shot blobs
}
```

`SystemShell` is backed by `tauri-plugin-shell` for `open_in_browser` and macOS-specific `Command::new("open").arg(...)` (and `open -R` for reveal-in-Finder) for the local paths.

New Tauri commands:

- `open_link(url: String) -> Result<(), String>`
- `reveal_capture(id: String) -> Result<(), String>` — looks up the Capture in the store, picks the right path field (`source_path` for `File`/`Shot { Path }`, `blob_path` for `Shot { Bytes }`), and reveals in Finder OR opens the blob depending on kind.

Composes `Shell` + `Store`. Integration tests use `FakeShell` to assert the right method is called with the right argument per kind.

### SvelteKit additions

- `src/lib/inbox/InboxDetail.svelte` — pure presentational component. Props: `capture: Capture | null` and per-action callbacks. Renders by kind:
  - `Link`: full URL (link styled, not clickable directly), `raw_text`, `title` (null in v0.1), big "Open in Browser" button -> `onOpenLink(url)`.
  - `File`: original name, mime, source path, "Reveal in Finder" button -> `onReveal(id)`.
  - `Shot { source_path }` (path-flavor): preview image loaded from `convertFileSrc(source_path)`, source path text, "Reveal in Finder" button -> `onReveal(id)`.
  - `Shot { blob_path }` (bytes-flavor): preview image loaded from `convertFileSrc(blob_path)`, blob path text, "Open Image" button -> `onReveal(id)` (which routes to `open_path` in Rust).
  - `Clip`: scrollable read-only `<pre>` of `text`.
  - `Note`: scrollable read-only `<pre>` of `text`.
  - `null`: "Select a Capture" placeholder.
- `/inbox/+page.svelte` wires the detail pane to the selected Capture, and the `onOpen` callback from slice 03's keyboard nav now calls `onOpenLink` / `onReveal` based on kind.

### Tests (per ADR-0005)

Rust:

- `commands::open_link` test using `FakeShell` — assert it forwards the URL.
- `commands::reveal_capture` test using `FakeShell` + temp store with one Capture of each path-bearing kind. Assert the right method is called with the right argument per kind.

Svelte:

- `InboxDetail.test.ts`:
  - Mount with each kind in turn (Link, File, Shot-path, Shot-bytes, Clip, Note, null). Assert the right text and action button appear.
  - Click the action button; assert the right callback fires with the right args.

## Acceptance criteria

- [ ] Selecting an Inbox row shows the full payload in the detail pane, formatted per kind.
- [ ] `Enter` on a selected row triggers the same Open action as clicking the action button.
- [ ] `Link` rows open the URL in the user's default browser.
- [ ] `File` and path-flavor `Shot` rows reveal the source path in Finder.
- [ ] Bytes-flavor `Shot` rows open the blob in the default image viewer (Preview).
- [ ] `Clip` and `Note` rows show the full text scrollable, read-only.
- [ ] `Shell` trait abstracts every OS call; tests run against `FakeShell` only.
- [ ] All four gates green. Slice committed per ADR-0006.

## Blocked by

- 03-inbox-actions-star-delete-keyboard-nav
