//! Integration tests for the `[[Name]]` mentions index. Covers the
//! save-time hook in `save_with_context` and the one-shot backfill
//! scan that fires at `Store::open()` when the index is empty but
//! captures exist.

use std::str::FromStr;

use quick_capture_lib::store::{CaptureInput, Store};
use tempfile::{tempdir, TempDir};
use ulid::Ulid;

fn temp_store() -> (TempDir, Store) {
    let dir = tempdir().expect("create tempdir");
    let path = dir.path().join("captures.db");
    let store = Store::open(&path).expect("open store");
    (dir, store)
}

fn save_note(store: &Store, text: &str) -> Ulid {
    let saved = store
        .save(CaptureInput::Note {
            text: text.to_string(),
        })
        .expect("save note");
    Ulid::from_str(&saved.id).expect("parse id")
}

#[test]
fn save_note_with_one_mention_indexes_it() {
    let (_dir, store) = temp_store();
    let id = save_note(&store, "ping [[Diego]] later");
    assert_eq!(store.mentions_for_capture(&id).unwrap(), vec!["Diego"]);
}

#[test]
fn save_note_with_no_brackets_indexes_nothing() {
    let (_dir, store) = temp_store();
    let id = save_note(&store, "just a thought");
    assert!(store.mentions_for_capture(&id).unwrap().is_empty());
}

#[test]
fn save_note_dedupes_case_insensitively_across_save() {
    let (_dir, store) = temp_store();
    let id = save_note(&store, "[[Diego]] and [[diego]]");
    // Only one row stored (PRIMARY KEY on capture_id + normalized).
    assert_eq!(store.mentions_for_capture(&id).unwrap(), vec!["Diego"]);
}

#[test]
fn save_note_stores_multiple_mentions_alpha_sorted() {
    let (_dir, store) = temp_store();
    let id = save_note(&store, "[[Diego]] meets [[Ana]] later");
    // mentions_for_capture sorts by lowercase name.
    let names = store.mentions_for_capture(&id).unwrap();
    assert_eq!(names, vec!["Ana", "Diego"]);
}

#[test]
fn other_kinds_get_no_mention_rows() {
    let (_dir, store) = temp_store();
    let saved = store
        .save(CaptureInput::Clip {
            text: "clipboard [[Diego]] text".to_string(),
        })
        .expect("save clip");
    let id = Ulid::from_str(&saved.id).unwrap();
    // Clip text is searchable via FTS but should not feed the
    // wikilink mention index — only Note carries typed wikilinks.
    assert!(store.mentions_for_capture(&id).unwrap().is_empty());
}

#[test]
fn backfill_rebuilds_index_when_reopening_with_empty_table() {
    let dir = tempdir().expect("create tempdir");
    let db_path = dir.path().join("captures.db");

    // First open: save a Note, confirm it indexed.
    let id = {
        let store = Store::open(&db_path).expect("open store");
        let id = save_note(&store, "see [[Diego]] tonight");
        assert_eq!(store.mentions_for_capture(&id).unwrap(), vec!["Diego"]);
        id
    };

    // Wipe the mentions table directly to simulate a DB written by
    // an older build (or a deliberately cleared index).
    {
        let conn = rusqlite::Connection::open(&db_path).expect("reopen conn");
        conn.execute("DELETE FROM capture_mentions", [])
            .expect("clear");
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM capture_mentions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
    }

    // Reopen via Store::open — maybe_backfill_mentions should
    // detect (empty index + Note rows present) and rebuild.
    let store = Store::open(&db_path).expect("reopen store");
    assert_eq!(store.mentions_for_capture(&id).unwrap(), vec!["Diego"]);
}

#[test]
fn backfill_does_nothing_on_a_brand_new_db() {
    // No captures, no mentions — backfill is a no-op and the index
    // stays empty.
    let (_dir, store) = temp_store();
    let id = Ulid::new();
    assert!(store.mentions_for_capture(&id).unwrap().is_empty());
}
