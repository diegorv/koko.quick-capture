//! Integration tests for `capture_clipboard_now`. We drive
//! `capture_clipboard_now_with` directly (the function the Tauri
//! command delegates to) with the fake `Clipboard` adapter so we do
//! not spin up a Tauri runtime, mirroring how slice 02 tested
//! `save_note_with_store`.

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

#[test]
fn https_text_persists_as_link() {
    let (_dir, store) = temp_store();
    let cb = text_clipboard("https://example.com");

    let saved = capture_clipboard_now_with(&cb, &store).expect("capture should succeed");

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

    let saved = capture_clipboard_now_with(&cb, &store).expect("capture should succeed");

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

    let saved = capture_clipboard_now_with(&cb, &store).expect("capture should succeed");
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

    let saved = capture_clipboard_now_with(&cb, &store).expect("capture should succeed");

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
