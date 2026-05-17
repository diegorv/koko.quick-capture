//! Integration tests for the Tauri command surface. We drive
//! `save_note_with_store` directly (the function the command delegates
//! to) so we do not need to spin up a Tauri runtime.

use quick_capture_lib::commands::{list_captures_with_store, save_note_with_store};
use quick_capture_lib::store::{CaptureInput, CaptureKind, Store};
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
fn list_captures_pages_through_cursor() {
    let (_dir, store) = temp_store();

    // Seed 3 rows with 2ms gaps so ULID timestamps strictly increase
    // (the v1 ulid crate does not guarantee intra-millisecond
    // monotonicity, so equal-timestamp ids would sort by random bits).
    let a = store
        .save(CaptureInput::Note { text: "a".into() })
        .expect("save a");
    std::thread::sleep(std::time::Duration::from_millis(2));
    let b = store
        .save(CaptureInput::Note { text: "b".into() })
        .expect("save b");
    std::thread::sleep(std::time::Duration::from_millis(2));
    let c = store
        .save(CaptureInput::Note { text: "c".into() })
        .expect("save c");

    // First page (cursor=None): newest first.
    let first = list_captures_with_store(&store, None, 2).expect("first page");
    assert_eq!(first.len(), 2);
    assert_eq!(first[0].id, c.id);
    assert_eq!(first[1].id, b.id);

    // Second page (cursor=b.id) returns the remaining 1.
    let second = list_captures_with_store(&store, Some(&b.id), 2).expect("second page");
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].id, a.id);
}

#[test]
fn list_captures_rejects_invalid_cursor_string() {
    let (_dir, store) = temp_store();
    let err = list_captures_with_store(&store, Some("not-a-ulid"), 10)
        .expect_err("invalid cursor must error");
    assert!(
        err.to_lowercase().contains("cursor"),
        "expected error to mention cursor, got: {err}"
    );
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
