//! Integration tests for the Tauri command surface. We drive
//! `save_note_with_store` directly (the function the command delegates
//! to) so we do not need to spin up a Tauri runtime.

use quick_capture_lib::commands::{
    delete_capture_with_store, list_captures_with_store, save_note_with_store,
    star_capture_with_store,
};
use quick_capture_lib::store::{CaptureInput, CaptureKind, Store};
use tempfile::TempDir;
use ulid::Ulid;

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
fn star_capture_toggles_flag() {
    let (_dir, store) = temp_store();
    let saved = save_note_with_store(&store, "to star").expect("save");

    star_capture_with_store(&store, &saved.id, true).expect("star");
    let after_star = store.list(10).expect("list");
    assert_eq!(after_star.len(), 1);
    assert!(after_star[0].starred, "expected starred=true after star");

    star_capture_with_store(&store, &saved.id, false).expect("unstar");
    let after_unstar = store.list(10).expect("list");
    assert_eq!(after_unstar.len(), 1);
    assert!(
        !after_unstar[0].starred,
        "expected starred=false after unstar"
    );
}

#[test]
fn star_capture_rejects_invalid_ulid() {
    let (_dir, store) = temp_store();
    let err = star_capture_with_store(&store, "not-a-ulid", true)
        .expect_err("invalid id must error");
    assert!(
        err.to_lowercase().contains("invalid id"),
        "expected error to mention invalid id, got: {err}"
    );
}

#[test]
fn delete_capture_hides_from_list() {
    let (_dir, store) = temp_store();
    let keep = save_note_with_store(&store, "keep me").expect("save keep");
    std::thread::sleep(std::time::Duration::from_millis(2));
    let drop = save_note_with_store(&store, "drop me").expect("save drop");

    delete_capture_with_store(&store, &drop.id).expect("delete");

    // Soft-deleted row no longer surfaces in list_captures.
    let listed = list_captures_with_store(&store, None, 10).expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, keep.id);

    // But the tombstone is still in the table.
    let drop_id = Ulid::from_string(&drop.id).expect("parse id");
    let row = store
        .find_with_deleted(&drop_id)
        .expect("find_with_deleted")
        .expect("tombstone row exists");
    assert!(
        row.deleted_at.is_some(),
        "expected deleted_at to be stamped on the tombstone"
    );
}

#[test]
fn delete_capture_rejects_invalid_ulid() {
    let (_dir, store) = temp_store();
    let err = delete_capture_with_store(&store, "not-a-ulid")
        .expect_err("invalid id must error");
    assert!(
        err.to_lowercase().contains("invalid id"),
        "expected error to mention invalid id, got: {err}"
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
