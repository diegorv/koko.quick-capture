//! Integration tests for the capture store. Each test opens a fresh
//! SQLite file inside a tempdir so they cannot collide and never touch
//! the real `~/Library/Application Support` location.

use std::str::FromStr;

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
fn save_then_list_round_trips_a_note() {
    let (_dir, store) = temp_store();

    let saved = store
        .save(CaptureInput::Note {
            text: "first thought".to_string(),
        })
        .expect("save note");

    assert_eq!(saved.kind, CaptureKind::Note);
    assert!(!saved.id.is_empty());
    assert!(!saved.created_at.is_empty());
    assert_eq!(
        saved.payload.get("text").and_then(|v| v.as_str()),
        Some("first thought")
    );

    let rows = store.list(10).expect("list");
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row.id, saved.id);
    assert_eq!(row.kind, CaptureKind::Note);
    assert_eq!(
        row.payload.get("text").and_then(|v| v.as_str()),
        Some("first thought")
    );
    assert!(row.deleted_at.is_none());
    assert!(!row.starred);
}

#[test]
fn set_star_toggles_the_flag() {
    let (_dir, store) = temp_store();
    let saved = store
        .save(CaptureInput::Note {
            text: "starred".to_string(),
        })
        .expect("save note");
    let id = Ulid::from_str(&saved.id).expect("parse ulid");

    store.set_star(&id, true).expect("star");
    let listed = store.list(10).expect("list");
    assert!(listed[0].starred, "expected starred=true after set_star");

    store.set_star(&id, false).expect("unstar");
    let listed = store.list(10).expect("list");
    assert!(!listed[0].starred, "expected starred=false after toggle");
}

#[test]
fn soft_delete_hides_from_list_but_keeps_row() {
    let (_dir, store) = temp_store();
    let saved = store
        .save(CaptureInput::Note {
            text: "doomed".to_string(),
        })
        .expect("save note");
    let id = Ulid::from_str(&saved.id).expect("parse ulid");

    store.soft_delete(&id).expect("soft delete");

    let listed = store.list(10).expect("list");
    assert!(
        listed.is_empty(),
        "soft-deleted row must not surface in list, got {listed:?}"
    );

    let still_there = store
        .find_with_deleted(&id)
        .expect("find_with_deleted")
        .expect("row must remain in the table as a tombstone");
    assert!(
        still_there.deleted_at.is_some(),
        "expected deleted_at to be stamped on tombstone"
    );
}
