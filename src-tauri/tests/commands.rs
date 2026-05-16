//! Integration tests for the Tauri command surface. We drive
//! `save_note_with_store` directly (the function the command delegates
//! to) so we do not need to spin up a Tauri runtime.

use quick_capture_lib::commands::save_note_with_store;
use quick_capture_lib::store::{CaptureKind, Store};
use tempfile::TempDir;

fn temp_store() -> (TempDir, Store) {
    let dir = tempfile::tempdir().expect("create tempdir");
    let path = dir.path().join("captures.db");
    let store = Store::open(&path).expect("open store");
    (dir, store)
}

#[test]
fn save_note_persists_a_row() {
    let (_dir, store) = temp_store();

    let saved = save_note_with_store(&store, "hello note").expect("save note");
    assert_eq!(saved.kind, CaptureKind::Note);
    assert_eq!(
        saved.payload.get("text").and_then(|v| v.as_str()),
        Some("hello note")
    );

    let listed = store.list(10).expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, saved.id);
}

#[test]
fn save_note_rejects_empty_text_and_writes_nothing() {
    let (_dir, store) = temp_store();

    let err = save_note_with_store(&store, "   \n\t").expect_err("empty text must error");
    assert!(
        err.to_lowercase().contains("empty"),
        "expected error message to mention empty, got: {err}"
    );

    let listed = store.list(10).expect("list");
    assert!(
        listed.is_empty(),
        "no row must be written on empty-text rejection, got {listed:?}"
    );
}
