//! Integration tests for `capture_clipboard_now`. We drive
//! `capture_clipboard_now_with` directly (the function the Tauri
//! command delegates to) with the fake `Clipboard` adapter so we do
//! not spin up a Tauri runtime, mirroring how slice 02 tested
//! `save_note_with_store`.

use std::io::Cursor;
use std::path::PathBuf;

use image::{ImageBuffer, Rgba};
use quick_capture_lib::clipboard::{ClipboardError, ClipboardSnapshot, FakeClipboard};
use quick_capture_lib::commands::capture_clipboard_now_with;
use quick_capture_lib::store::{CaptureKind, Store};
use tempfile::TempDir;

fn temp_store() -> (TempDir, Store) {
    let dir = tempfile::tempdir().expect("create tempdir");
    let path = dir.path().join("captures.db");
    let store = Store::open(&path).expect("open store");
    (dir, store)
}

fn text_clipboard(s: &str) -> FakeClipboard {
    FakeClipboard::with(Ok(ClipboardSnapshot::Text(s.to_string())))
}

fn one(result: Vec<quick_capture_lib::store::Capture>) -> quick_capture_lib::store::Capture {
    assert_eq!(
        result.len(),
        1,
        "expected exactly one capture, got {result:?}"
    );
    result.into_iter().next().unwrap()
}

fn tiny_png_bytes() -> Vec<u8> {
    let buf: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_pixel(2, 2, Rgba([0x10, 0x20, 0x30, 0xFF]));
    let mut out = Vec::new();
    buf.write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
        .expect("encode png");
    out
}

#[test]
fn https_text_persists_as_link() {
    let (_dir, store) = temp_store();
    let cb = text_clipboard("https://example.com");

    let saved = one(capture_clipboard_now_with(&cb, &store).expect("capture should succeed"));

    assert_eq!(saved.kind, CaptureKind::Link);
    assert_eq!(
        saved.payload.get("url").and_then(|v| v.as_str()),
        Some("https://example.com")
    );
    assert_eq!(
        saved.payload.get("raw_text").and_then(|v| v.as_str()),
        Some("https://example.com")
    );
    assert!(
        saved.payload.get("title").is_some_and(|v| v.is_null()),
        "title must be present and null, got: {:?}",
        saved.payload.get("title")
    );

    let listed = store.list(10).expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, saved.id);
}

#[test]
fn www_text_is_promoted_to_https_link() {
    let (_dir, store) = temp_store();
    let cb = text_clipboard("www.example.com");

    let saved = one(capture_clipboard_now_with(&cb, &store).expect("capture should succeed"));

    assert_eq!(saved.kind, CaptureKind::Link);
    assert_eq!(
        saved.payload.get("url").and_then(|v| v.as_str()),
        Some("https://www.example.com")
    );
    assert_eq!(
        saved.payload.get("raw_text").and_then(|v| v.as_str()),
        Some("www.example.com"),
        "raw_text must keep the original copied form"
    );
}

#[test]
fn mailto_text_persists_as_link() {
    let (_dir, store) = temp_store();
    let cb = text_clipboard("mailto:a@b.com");

    let saved = one(capture_clipboard_now_with(&cb, &store).expect("capture should succeed"));
    assert_eq!(saved.kind, CaptureKind::Link);
    assert_eq!(
        saved.payload.get("url").and_then(|v| v.as_str()),
        Some("mailto:a@b.com")
    );
}

#[test]
fn plain_text_persists_as_clip() {
    let (_dir, store) = temp_store();
    let cb = text_clipboard("just a thought I had");

    let saved = one(capture_clipboard_now_with(&cb, &store).expect("capture should succeed"));

    assert_eq!(saved.kind, CaptureKind::Clip);
    assert_eq!(
        saved.payload.get("text").and_then(|v| v.as_str()),
        Some("just a thought I had")
    );
}

#[test]
fn empty_clipboard_errors_and_writes_nothing() {
    let (_dir, store) = temp_store();
    let cb = FakeClipboard::with(Err(ClipboardError::Empty));

    let err = capture_clipboard_now_with(&cb, &store).expect_err("empty clipboard must error");
    assert!(
        err.to_lowercase().contains("empty"),
        "expected error to mention empty, got: {err}"
    );

    let listed = store.list(10).expect("list");
    assert!(
        listed.is_empty(),
        "no row must be written on empty-clipboard rejection, got {listed:?}"
    );
}

#[test]
fn whitespace_only_clipboard_text_errors_and_writes_nothing() {
    let (_dir, store) = temp_store();
    let cb = text_clipboard("   \n\t ");

    let err =
        capture_clipboard_now_with(&cb, &store).expect_err("whitespace-only must error");
    assert!(
        err.to_lowercase().contains("empty"),
        "expected error to mention empty, got: {err}"
    );

    let listed = store.list(10).expect("list");
    assert!(listed.is_empty(), "no row must be written, got {listed:?}");
}

#[test]
fn image_snapshot_persists_as_shot_with_blob() {
    let dir = tempfile::tempdir().expect("create tempdir");
    let db_path = dir.path().join("captures.db");
    let store = Store::open(&db_path).expect("open store");
    let bytes = tiny_png_bytes();
    let cb = FakeClipboard::with(Ok(ClipboardSnapshot::Image {
        bytes: bytes.clone(),
        mime: "image/png".into(),
    }));

    let saved = one(capture_clipboard_now_with(&cb, &store).expect("capture should succeed"));

    assert_eq!(saved.kind, CaptureKind::Shot);
    assert_eq!(
        saved.payload.get("mime").and_then(|v| v.as_str()),
        Some("image/png")
    );
    let blob_path = saved
        .payload
        .get("blob_path")
        .and_then(|v| v.as_str())
        .expect("blob_path must be present");
    let blob_path = PathBuf::from(blob_path);
    assert!(blob_path.exists(), "blob file must exist at {blob_path:?}");
    let on_disk = std::fs::read(&blob_path).expect("read blob");
    assert_eq!(on_disk, bytes, "blob bytes must match clipboard bytes");
    assert_eq!(
        blob_path.file_name().and_then(|n| n.to_str()),
        Some(format!("{}.png", saved.id).as_str())
    );
    assert_eq!(
        blob_path.parent().expect("blob parent"),
        dir.path().join("blobs")
    );
}

#[test]
fn files_snapshot_with_mixed_mimes_expands_to_n_captures() {
    let (_dir, store) = temp_store();
    let paths = vec![
        PathBuf::from("/tmp/shot.png"),
        PathBuf::from("/tmp/notes.pdf"),
        PathBuf::from("/tmp/photo.jpg"),
    ];
    let cb = FakeClipboard::with(Ok(ClipboardSnapshot::Files(paths.clone())));

    let saved = capture_clipboard_now_with(&cb, &store).expect("capture should succeed");

    assert_eq!(saved.len(), 3);
    assert_eq!(saved[0].kind, CaptureKind::Shot);
    assert_eq!(saved[1].kind, CaptureKind::File);
    assert_eq!(saved[2].kind, CaptureKind::Shot);

    // Shot path flavor: source_path is set, blob_path is not.
    assert_eq!(
        saved[0].payload.get("source_path").and_then(|v| v.as_str()),
        Some("/tmp/shot.png")
    );
    assert!(saved[0].payload.get("blob_path").is_none());
    assert_eq!(
        saved[0].payload.get("mime").and_then(|v| v.as_str()),
        Some("image/png")
    );

    // File: source_path + mime + original_name.
    assert_eq!(
        saved[1].payload.get("source_path").and_then(|v| v.as_str()),
        Some("/tmp/notes.pdf")
    );
    assert_eq!(
        saved[1].payload.get("mime").and_then(|v| v.as_str()),
        Some("application/pdf")
    );
    assert_eq!(
        saved[1].payload.get("original_name").and_then(|v| v.as_str()),
        Some("notes.pdf")
    );

    let listed = store.list(10).expect("list");
    assert_eq!(listed.len(), 3, "three rows must land in the store");
}

#[test]
fn empty_files_snapshot_errors_and_writes_nothing() {
    let (_dir, store) = temp_store();
    let cb = FakeClipboard::with(Ok(ClipboardSnapshot::Files(vec![])));

    let err = capture_clipboard_now_with(&cb, &store)
        .expect_err("empty files list must error");
    assert!(
        err.to_lowercase().contains("empty"),
        "expected error to mention empty, got: {err}"
    );

    let listed = store.list(10).expect("list");
    assert!(listed.is_empty(), "no row must be written, got {listed:?}");
}
