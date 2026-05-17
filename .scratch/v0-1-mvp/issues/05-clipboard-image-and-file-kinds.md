Status: done

# Clipboard image and file kinds

## Parent

[v0-1-mvp PRD](../PRD.md)

## What to build

Extend clipboard capture to handle the non-text variants of the clipboard. After this slice, the `Ctrl+Opt+Cmd+C` shortcut covers every clipboard payload type produced by macOS in normal use: text, screenshots-as-image, and file references (single or multiple).

This slice extends:

- `clipboard::ClipboardSnapshot` to fully implement the `Image { bytes: Vec<u8>, mime: String }` and `Files(Vec<PathBuf>)` variants, with a real backend that returns them when the system clipboard holds those types. The fake implementation is updated to allow tests to construct each variant.
- `kind_detect::decide` to handle the image and file branches: an `Image` snapshot becomes a `Shot` Capture; a `Files` snapshot becomes one Capture per file (or a single Capture if multi-file UX is decided otherwise — see notes), with image-mime files becoming `Shot` and non-image-mime files becoming `File`.
- `store::save` to persist image bytes for `Shot` Captures: write the bytes to `~/Library/Application Support/com.koko.quick-capture/blobs/<ulid>.<ext>` and record `payload.blob_path` plus `payload.mime`. The `blobs/` directory is created on first need.
- For `File` Captures coming from clipboard file references, store the original path under `payload.source_path` and the mime under `payload.mime`. Do NOT copy the file into `blobs/` in this slice — the copy-vs-reference policy is revisited when drag-and-drop lands in v1.0.

Multi-file behavior: if the user copies N files at once, produce N Captures, one per file. (Open the file enumeration question with the user during implementation only if the chosen clipboard backend cannot enumerate multi-file selections; the default plan is N rows.)

Tests (per ADR-0005):

Rust:

- `kind_detect` unit tests for the image and file branches, including image-mime vs non-image-mime path splits.
- A `store` integration test that saves a `Shot` Capture from an in-memory byte buffer, asserts a row exists referencing a real file under `blobs/`, asserts the file bytes match, and that the file is named after the ULID.
- A `capture_clipboard_now` integration test using the fake clipboard's `Image` and `Files` variants to assert end-to-end behavior, including the single-multi-file expansion into N Captures.
- A `clipboard` fake-implementation test confirming it surfaces `Image` and `Files` variants correctly.

## Acceptance criteria

- [ ] Taking a screenshot with `Cmd+Ctrl+Shift+4` (or `+3`) and then pressing `Ctrl+Opt+Cmd+C` produces a `Shot` Capture with the image bytes stored under `blobs/<ulid>.<ext>` and `payload.blob_path`/`payload.mime` set.
- [ ] Copying a file in Finder and pressing the shortcut produces a `File` (or `Shot` if image mime) Capture pointing at the original file path via `payload.source_path` and `payload.mime`.
- [ ] Copying multiple files at once produces one Capture per file.
- [ ] The `blobs/` directory is created automatically on first image save.
- [ ] `kind_detect` unit tests cover image, image-mime file, and non-image-mime file branches.
- [ ] `store` integration test verifies image blob round-trip: bytes saved, file exists at expected path, payload contains the path.
- [ ] `capture_clipboard_now` integration test covers `Image` and `Files` variants end-to-end, including N-files-to-N-Captures expansion.
- [ ] All clipboard reads remain behind the `clipboard` trait. No image/file APIs leak into `commands`.
- [ ] All checks green: `cargo test`, `cargo check`, `pnpm test`, `pnpm check`. Slice committed in one commit using Conventional Commits per ADR-0006.

## Blocked by

- 04-clipboard-text-capture-tracer
