//! Integration tests for the Tauri command surface. We drive
//! `save_note_with_store` directly (the function the command delegates
//! to) so we do not need to spin up a Tauri runtime.

use quick_capture_lib::commands::{
    delete_capture_with_store, is_clipboard_duplicate, list_captures_with_store, mark_read_with_store,
    save_note_with_store, search_captures_with_store, star_capture_with_store, total_count_with_store,
    unread_count_with_store,
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
fn unread_count_with_store_returns_zero_when_no_captures() {
    let (_dir, store) = temp_store();
    let n = unread_count_with_store(&store).expect("unread_count");
    assert_eq!(n, 0, "empty store reports zero unread");
}

#[test]
fn unread_count_with_store_counts_rows_with_null_read_at() {
    let (_dir, store) = temp_store();

    // Freshly-saved rows are unread by default.
    for i in 0..5 {
        save_note_with_store(&store, &format!("note {i}")).expect("save");
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(unread_count_with_store(&store).expect("count"), 5);
}

#[test]
fn unread_count_with_store_ignores_soft_deleted_and_already_read() {
    let (_dir, store) = temp_store();

    let mut ids = Vec::with_capacity(3);
    for i in 0..3 {
        let saved = save_note_with_store(&store, &format!("n{i}")).expect("save");
        ids.push(saved.id);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(unread_count_with_store(&store).expect("before"), 3);

    // Mark the middle row as read; count drops by one.
    mark_read_with_store(&store, &ids[1]).expect("mark read");
    assert_eq!(unread_count_with_store(&store).expect("after read"), 2);

    // Soft-delete an unread row; count drops by one more.
    let last = Ulid::from_string(&ids[2]).expect("parse");
    store.soft_delete(&last).expect("soft delete");
    assert_eq!(unread_count_with_store(&store).expect("after delete"), 1);
}

#[test]
fn total_count_with_store_returns_zero_when_empty() {
    let (_dir, store) = temp_store();
    let n = total_count_with_store(&store).expect("total");
    assert_eq!(n, 0);
}

#[test]
fn total_count_with_store_counts_live_rows_and_ignores_soft_deleted() {
    let (_dir, store) = temp_store();
    let mut ids = Vec::with_capacity(3);
    for i in 0..3 {
        let saved = save_note_with_store(&store, &format!("t{i}")).expect("save");
        ids.push(saved.id);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(total_count_with_store(&store).expect("total"), 3);

    let last = Ulid::from_string(&ids[2]).expect("parse");
    store.soft_delete(&last).expect("soft delete");
    assert_eq!(
        total_count_with_store(&store).expect("total after"),
        2,
        "soft-deleted row must not count toward total"
    );
}

#[test]
fn mark_read_with_store_flips_one_row_and_returns_remaining_unread() {
    let (_dir, store) = temp_store();

    let mut ids = Vec::with_capacity(3);
    for i in 0..3 {
        let saved = save_note_with_store(&store, &format!("n{i}")).expect("save");
        ids.push(saved.id);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(unread_count_with_store(&store).expect("before"), 3);

    let remaining = mark_read_with_store(&store, &ids[1]).expect("mark");
    assert_eq!(remaining, 2, "must return the live unread count after the flip");
    assert_eq!(unread_count_with_store(&store).expect("after"), 2);
}

#[test]
fn mark_read_with_store_is_idempotent() {
    let (_dir, store) = temp_store();
    let saved = save_note_with_store(&store, "only").expect("save");

    let first = mark_read_with_store(&store, &saved.id).expect("first");
    assert_eq!(first, 0);
    let second = mark_read_with_store(&store, &saved.id).expect("second");
    assert_eq!(second, 0, "second call leaves the row unchanged");
}

#[test]
fn mark_read_with_store_rejects_invalid_ulid() {
    let (_dir, store) = temp_store();
    let err = mark_read_with_store(&store, "not-a-ulid").expect_err("must reject");
    assert!(
        err.to_lowercase().contains("invalid id"),
        "expected invalid-id message, got: {err}"
    );
}

#[test]
fn is_clipboard_duplicate_matches_same_clip_text() {
    let (_dir, store) = temp_store();
    let saved = store
        .save(CaptureInput::Clip {
            text: "hello".into(),
        })
        .expect("save");

    assert!(is_clipboard_duplicate(
        &saved,
        &CaptureInput::Clip {
            text: "hello".into()
        }
    ));
    assert!(!is_clipboard_duplicate(
        &saved,
        &CaptureInput::Clip {
            text: "different".into()
        }
    ));
}

#[test]
fn is_clipboard_duplicate_is_kind_aware() {
    let (_dir, store) = temp_store();
    let saved = store
        .save(CaptureInput::Clip { text: "x".into() })
        .expect("save");

    // Same text on a different kind is NOT a duplicate.
    assert!(!is_clipboard_duplicate(
        &saved,
        &CaptureInput::Note { text: "x".into() }
    ));
}

#[test]
fn is_clipboard_duplicate_matches_same_link_url() {
    let (_dir, store) = temp_store();
    let saved = store
        .save(CaptureInput::Link {
            url: "https://example.com".into(),
            raw_text: "https://example.com".into(),
            title: None,
        })
        .expect("save");

    assert!(is_clipboard_duplicate(
        &saved,
        &CaptureInput::Link {
            url: "https://example.com".into(),
            raw_text: "(any)".into(),
            title: Some("title".into()),
        }
    ));
}

#[test]
fn search_captures_finds_indexed_note_text() {
    let (_dir, store) = temp_store();
    save_note_with_store(&store, "hello world").expect("save");
    save_note_with_store(&store, "completely unrelated").expect("save");

    let results = search_captures_with_store(&store, "hello", 10).expect("search");
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].payload.get("text").and_then(|v| v.as_str()),
        Some("hello world")
    );
}

#[test]
fn search_captures_supports_prefix_match() {
    let (_dir, store) = temp_store();
    save_note_with_store(&store, "engineering notebook").expect("save");

    let results = search_captures_with_store(&store, "engin", 10).expect("search");
    assert_eq!(results.len(), 1);
}

#[test]
fn search_captures_excludes_soft_deleted_rows() {
    let (_dir, store) = temp_store();
    let saved = save_note_with_store(&store, "secret").expect("save");
    let id = Ulid::from_string(&saved.id).expect("parse");

    assert_eq!(
        search_captures_with_store(&store, "secret", 10)
            .expect("search before")
            .len(),
        1
    );

    store.soft_delete(&id).expect("soft delete");
    assert_eq!(
        search_captures_with_store(&store, "secret", 10)
            .expect("search after")
            .len(),
        0,
        "soft-deleted rows must drop out of search results"
    );
}

#[test]
fn search_captures_empty_query_returns_no_rows() {
    let (_dir, store) = temp_store();
    save_note_with_store(&store, "hello").expect("save");
    assert!(search_captures_with_store(&store, "   ", 10)
        .expect("search")
        .is_empty());
}

#[test]
fn search_captures_sanitises_punctuation_in_query() {
    let (_dir, store) = temp_store();
    // FTS5 would reject `https://example.com` raw because of the
    // special chars; build_fts_match must strip them down to
    // alphanumeric tokens.
    save_note_with_store(&store, "see https://example.com for docs").expect("save");
    let results = search_captures_with_store(&store, "https://example.com", 10).expect("search");
    assert_eq!(results.len(), 1);
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
