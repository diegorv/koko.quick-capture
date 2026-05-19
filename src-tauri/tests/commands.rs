//! Integration tests for the Tauri command surface. We drive
//! `save_note_with_store` directly (the function the command delegates
//! to) so we do not need to spin up a Tauri runtime.

use quick_capture_lib::commands::{
    create_destination_with_store, delete_capture_with_store, inbox_count_with_store,
    is_clipboard_duplicate, list_archive_with_store, list_captures_with_store,
    list_deleted_destinations_with_store, list_destinations_with_store, mark_read_with_store,
    restore_destination_with_store, route_capture_with_store, route_to_kokobrain_with_store,
    save_note_with_store, search_archive_with_store, search_captures_with_store,
    soft_delete_destination_with_store, star_capture_with_store, total_count_with_store,
    unread_count_with_store, unroute_capture_with_store, update_destination_with_store,
};
use quick_capture_lib::store::{CaptureContext, CaptureInput, CaptureKind, DestinationKind, Store};
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

    let saved = save_note_with_store(&store, "hello note", CaptureContext::default()).expect("save note");
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
    let first = list_captures_with_store(&store, None, None, 2).expect("first page");
    assert_eq!(first.len(), 2);
    assert_eq!(first[0].id, c.id);
    assert_eq!(first[1].id, b.id);

    // Second page (cursor=b.id) returns the remaining 1.
    let second =
        list_captures_with_store(&store, Some(&b.id), None, 2).expect("second page");
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].id, a.id);
}

#[test]
fn list_captures_rejects_invalid_cursor_string() {
    let (_dir, store) = temp_store();
    let err = list_captures_with_store(&store, Some("not-a-ulid"), None, 10)
        .expect_err("invalid cursor must error");
    assert!(
        err.to_lowercase().contains("cursor"),
        "expected error to mention cursor, got: {err}"
    );
}

#[test]
fn star_capture_toggles_flag() {
    let (_dir, store) = temp_store();
    let saved = save_note_with_store(&store, "to star", CaptureContext::default()).expect("save");

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
    let keep = save_note_with_store(&store, "keep me", CaptureContext::default()).expect("save keep");
    std::thread::sleep(std::time::Duration::from_millis(2));
    let drop = save_note_with_store(&store, "drop me", CaptureContext::default()).expect("save drop");

    delete_capture_with_store(&store, &drop.id).expect("delete");

    // Soft-deleted row no longer surfaces in list_captures.
    let listed = list_captures_with_store(&store, None, None, 10).expect("list");
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
        save_note_with_store(&store, &format!("note {i}"), CaptureContext::default()).expect("save");
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(unread_count_with_store(&store).expect("count"), 5);
}

#[test]
fn unread_count_with_store_ignores_soft_deleted_and_already_read() {
    let (_dir, store) = temp_store();

    let mut ids = Vec::with_capacity(3);
    for i in 0..3 {
        let saved = save_note_with_store(&store, &format!("n{i}"), CaptureContext::default()).expect("save");
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
        let saved = save_note_with_store(&store, &format!("n{i}"), CaptureContext::default()).expect("save");
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
        let saved = save_note_with_store(&store, &format!("n{i}"), CaptureContext::default()).expect("save");
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
    let saved = save_note_with_store(&store, "only", CaptureContext::default()).expect("save");

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
    save_note_with_store(&store, "hello world", CaptureContext::default()).expect("save");
    save_note_with_store(&store, "completely unrelated", CaptureContext::default()).expect("save");

    let results = search_captures_with_store(&store, "hello", None, 10).expect("search");
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].payload.get("text").and_then(|v| v.as_str()),
        Some("hello world")
    );
}

#[test]
fn search_captures_supports_prefix_match() {
    let (_dir, store) = temp_store();
    save_note_with_store(&store, "engineering notebook", CaptureContext::default()).expect("save");

    let results = search_captures_with_store(&store, "engin", None, 10).expect("search");
    assert_eq!(results.len(), 1);
}

#[test]
fn search_captures_excludes_soft_deleted_rows() {
    let (_dir, store) = temp_store();
    let saved = save_note_with_store(&store, "secret", CaptureContext::default()).expect("save");
    let id = Ulid::from_string(&saved.id).expect("parse");

    assert_eq!(
        search_captures_with_store(&store, "secret", None, 10)
            .expect("search before")
            .len(),
        1
    );

    store.soft_delete(&id).expect("soft delete");
    assert_eq!(
        search_captures_with_store(&store, "secret", None, 10)
            .expect("search after")
            .len(),
        0,
        "soft-deleted rows must drop out of search results"
    );
}

#[test]
fn search_captures_empty_query_returns_no_rows() {
    let (_dir, store) = temp_store();
    save_note_with_store(&store, "hello", CaptureContext::default()).expect("save");
    assert!(search_captures_with_store(&store, "   ", None, 10)
        .expect("search")
        .is_empty());
}

#[test]
fn search_captures_sanitises_punctuation_in_query() {
    let (_dir, store) = temp_store();
    // FTS5 would reject `https://example.com` raw because of the
    // special chars; build_fts_match must strip them down to
    // alphanumeric tokens.
    save_note_with_store(&store, "see https://example.com for docs", CaptureContext::default()).expect("save");
    let results =
        search_captures_with_store(&store, "https://example.com", None, 10).expect("search");
    assert_eq!(results.len(), 1);
}

#[test]
fn save_note_rejects_empty_text_and_writes_nothing() {
    let (_dir, store) = temp_store();

    let err = save_note_with_store(&store, "   \n\t", CaptureContext::default()).expect_err("empty text must error");
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

// ── Destinations + routing (ADR-0010) ─────────────────────────────

#[test]
fn create_destination_command_returns_row_and_lists_it() {
    let (_dir, store) = temp_store();
    let created = create_destination_with_store(&store, "Todoist", Some("red"), DestinationKind::Label, None).expect("create");
    assert_eq!(created.name, "Todoist");
    assert_eq!(created.color.as_deref(), Some("red"));
    assert!(created.deleted_at.is_none());

    let listed = list_destinations_with_store(&store).expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, created.id);
}

#[test]
fn create_destination_command_surfaces_blank_name_error() {
    let (_dir, store) = temp_store();
    let err = create_destination_with_store(&store, "   ", None, DestinationKind::Label, None)
        .expect_err("blank name should error");
    assert!(
        err.to_lowercase().contains("blank")
            || err.to_lowercase().contains("invalid"),
        "expected blank-name error, got: {err}"
    );
}

#[test]
fn create_destination_command_surfaces_name_conflict() {
    let (_dir, store) = temp_store();
    create_destination_with_store(&store, "Todoist", None, DestinationKind::Label, None).expect("first");
    let err = create_destination_with_store(&store, "Todoist", None, DestinationKind::Label, None)
        .expect_err("dup must error");
    assert!(
        err.to_lowercase().contains("already in use")
            || err.to_lowercase().contains("conflict"),
        "expected conflict error, got: {err}"
    );
}

#[test]
fn update_destination_command_renames_and_recolors() {
    let (_dir, store) = temp_store();
    let created = create_destination_with_store(&store, "Old", Some("red"), DestinationKind::Label, None).expect("create");
    update_destination_with_store(&store, &created.id, "New", Some("blue"), DestinationKind::Label, None).expect("update");

    let listed = list_destinations_with_store(&store).expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, "New");
    assert_eq!(listed[0].color.as_deref(), Some("blue"));
}

#[test]
fn soft_delete_destination_command_hides_live_and_surfaces_in_deleted_list() {
    let (_dir, store) = temp_store();
    let created = create_destination_with_store(&store, "Old", None, DestinationKind::Label, None).expect("create");
    soft_delete_destination_with_store(&store, &created.id).expect("soft delete");

    let live = list_destinations_with_store(&store).expect("live");
    assert!(live.is_empty());
    let deleted = list_deleted_destinations_with_store(&store).expect("deleted");
    assert_eq!(deleted.len(), 1);
    assert_eq!(deleted[0].id, created.id);
}

#[test]
fn restore_destination_command_brings_back_when_no_conflict() {
    let (_dir, store) = temp_store();
    let created = create_destination_with_store(&store, "Ref", None, DestinationKind::Label, None).expect("create");
    soft_delete_destination_with_store(&store, &created.id).expect("soft delete");
    restore_destination_with_store(&store, &created.id).expect("restore");

    let live = list_destinations_with_store(&store).expect("live");
    assert_eq!(live.len(), 1);
    assert_eq!(live[0].id, created.id);
}

#[test]
fn restore_destination_command_surfaces_conflict_when_name_taken() {
    let (_dir, store) = temp_store();
    let old = create_destination_with_store(&store, "Reading", None, DestinationKind::Label, None).expect("create old");
    soft_delete_destination_with_store(&store, &old.id).expect("soft delete");
    create_destination_with_store(&store, "Reading", None, DestinationKind::Label, None).expect("re-create");

    let err = restore_destination_with_store(&store, &old.id)
        .expect_err("restore must conflict");
    assert!(
        err.to_lowercase().contains("already in use"),
        "expected conflict error, got: {err}"
    );
}

#[test]
fn route_capture_command_moves_capture_out_of_inbox_and_into_archive() {
    let (_dir, store) = temp_store();
    let saved = save_note_with_store(&store, "to route", CaptureContext::default()).expect("save");
    let dest = create_destination_with_store(&store, "Todoist", None, DestinationKind::Label, None).expect("create dest");

    route_capture_with_store(&store, &saved.id, &dest.id).expect("route");

    let inbox = list_captures_with_store(&store, None, None, 10).expect("inbox");
    assert!(inbox.is_empty(), "routed row must leave inbox");
    let archive = list_archive_with_store(&store, None, None, None, 10).expect("archive");
    assert_eq!(archive.len(), 1);
    assert_eq!(archive[0].id, saved.id);
    assert_eq!(archive[0].destination_id.as_deref(), Some(dest.id.as_str()));
    assert!(archive[0].routed_at.is_some());
    // Routing implies read.
    assert!(archive[0].read_at.is_some());
}

#[test]
fn route_capture_command_rejects_invalid_capture_id() {
    let (_dir, store) = temp_store();
    let dest = create_destination_with_store(&store, "Todoist", None, DestinationKind::Label, None).expect("create dest");
    let err = route_capture_with_store(&store, "not-a-ulid", &dest.id)
        .expect_err("invalid id must error");
    assert!(err.to_lowercase().contains("invalid id"));
}

#[test]
fn route_capture_command_rejects_soft_deleted_destination() {
    let (_dir, store) = temp_store();
    let saved = save_note_with_store(&store, "x", CaptureContext::default()).expect("save");
    let dest = create_destination_with_store(&store, "Old", None, DestinationKind::Label, None).expect("create dest");
    soft_delete_destination_with_store(&store, &dest.id).expect("soft delete dest");

    let err = route_capture_with_store(&store, &saved.id, &dest.id)
        .expect_err("must reject soft-deleted dest");
    assert!(
        err.to_lowercase().contains("soft-deleted")
            || err.to_lowercase().contains("invalid argument"),
        "got: {err}"
    );
}

#[test]
fn unroute_capture_command_returns_capture_to_inbox() {
    let (_dir, store) = temp_store();
    let saved = save_note_with_store(&store, "y", CaptureContext::default()).expect("save");
    let dest = create_destination_with_store(&store, "Readwise", None, DestinationKind::Label, None).expect("create dest");
    route_capture_with_store(&store, &saved.id, &dest.id).expect("route");
    unroute_capture_with_store(&store, &saved.id).expect("unroute");

    let inbox = list_captures_with_store(&store, None, None, 10).expect("inbox");
    assert_eq!(inbox.len(), 1);
    assert_eq!(inbox[0].id, saved.id);
    assert!(inbox[0].destination_id.is_none());
    assert!(inbox[0].routed_at.is_none());
}

#[test]
fn search_archive_command_excludes_inbox_results() {
    let (_dir, store) = temp_store();
    let kept = save_note_with_store(&store, "alpha kept", CaptureContext::default()).expect("kept");
    let routed = save_note_with_store(&store, "alpha routed", CaptureContext::default()).expect("routed");
    let dest = create_destination_with_store(&store, "Todoist", None, DestinationKind::Label, None).expect("create dest");
    route_capture_with_store(&store, &routed.id, &dest.id).expect("route");

    let hits = search_archive_with_store(&store, "alpha", None, None, 10).expect("search");
    let ids: Vec<&str> = hits.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, vec![routed.id.as_str()]);
    let _ = kept; // referenced for clarity above
}

#[test]
fn list_archive_command_filters_by_destination() {
    let (_dir, store) = temp_store();
    let dest_a = create_destination_with_store(&store, "A", None, DestinationKind::Label, None).expect("a");
    let dest_b = create_destination_with_store(&store, "B", None, DestinationKind::Label, None).expect("b");
    let a = save_note_with_store(&store, "a", CaptureContext::default()).expect("a save");
    let b = save_note_with_store(&store, "b", CaptureContext::default()).expect("b save");
    route_capture_with_store(&store, &a.id, &dest_a.id).expect("route a");
    route_capture_with_store(&store, &b.id, &dest_b.id).expect("route b");

    let only_a =
        list_archive_with_store(&store, Some(&dest_a.id), None, None, 10).expect("filter a");
    assert_eq!(only_a.len(), 1);
    assert_eq!(only_a[0].id, a.id);
    let only_b =
        list_archive_with_store(&store, Some(&dest_b.id), None, None, 10).expect("filter b");
    assert_eq!(only_b.len(), 1);
    assert_eq!(only_b[0].id, b.id);
}

#[test]
fn inbox_count_command_excludes_routed_and_deleted_rows() {
    let (_dir, store) = temp_store();
    save_note_with_store(&store, "stays", CaptureContext::default()).expect("a");
    let routed = save_note_with_store(&store, "routed", CaptureContext::default()).expect("b");
    let dropped = save_note_with_store(&store, "dropped", CaptureContext::default()).expect("c");
    let dest = create_destination_with_store(&store, "Todoist", None, DestinationKind::Label, None).expect("dest");
    route_capture_with_store(&store, &routed.id, &dest.id).expect("route");
    delete_capture_with_store(&store, &dropped.id).expect("delete");

    assert_eq!(inbox_count_with_store(&store).expect("inbox count"), 1);
}

// ── route_to_kokobrain (ADR-0012) ─────────────────────────────────

fn kokobrain_dest(store: &Store, name: &str, vault: &str) -> quick_capture_lib::store::Destination {
    create_destination_with_store(
        store,
        name,
        None,
        DestinationKind::Kokobrain,
        Some(&format!(r#"{{"vault":"{vault}"}}"#)),
    )
    .expect("create kokobrain dest")
}

#[test]
fn route_to_kokobrain_fires_uri_and_marks_routed() {
    use std::sync::Mutex;
    let (_dir, store) = temp_store();
    let saved =
        save_note_with_store(&store, "hello brain", CaptureContext::default()).expect("save");
    let dest = kokobrain_dest(&store, "Reading List", "Personal");

    let calls: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let opener = |uri: &str| -> Result<(), String> {
        calls.lock().unwrap().push(uri.to_string());
        Ok(())
    };
    let uri = route_to_kokobrain_with_store(&store, &opener, &saved.id, &dest.id)
        .expect("route to kokobrain");

    let observed = calls.lock().unwrap();
    assert_eq!(observed.len(), 1, "opener fires exactly once");
    assert_eq!(observed[0], uri);
    assert!(uri.starts_with("kokobrain://capture?"));
    assert!(uri.contains("vault=Personal"));
    assert!(uri.contains("content=hello+brain"));
    assert!(uri.contains("tags=reading-list"));

    let archive = list_archive_with_store(&store, None, None, None, 10).expect("archive");
    assert_eq!(archive.len(), 1);
    assert_eq!(archive[0].id, saved.id);
    assert_eq!(archive[0].destination_id.as_deref(), Some(dest.id.as_str()));
}

#[test]
fn route_to_kokobrain_keeps_capture_in_inbox_when_opener_fails() {
    let (_dir, store) = temp_store();
    let saved =
        save_note_with_store(&store, "abort me", CaptureContext::default()).expect("save");
    let dest = kokobrain_dest(&store, "Brain", "Personal");

    let opener = |_uri: &str| -> Result<(), String> { Err("opener exploded".into()) };
    let err = route_to_kokobrain_with_store(&store, &opener, &saved.id, &dest.id)
        .expect_err("opener failure aborts route");
    assert!(err.contains("opener exploded"));

    let inbox = list_captures_with_store(&store, None, None, 10).expect("inbox");
    assert_eq!(inbox.len(), 1, "capture must stay in inbox");
    assert_eq!(inbox[0].id, saved.id);
    assert!(inbox[0].destination_id.is_none());
    assert!(inbox[0].routed_at.is_none());
}

#[test]
fn route_to_kokobrain_rejects_label_destination() {
    let (_dir, store) = temp_store();
    let saved = save_note_with_store(&store, "x", CaptureContext::default()).expect("save");
    let dest = create_destination_with_store(&store, "Todoist", None, DestinationKind::Label, None)
        .expect("create label dest");

    let called = std::sync::Mutex::new(false);
    let opener = |_uri: &str| -> Result<(), String> {
        *called.lock().unwrap() = true;
        Ok(())
    };
    let err = route_to_kokobrain_with_store(&store, &opener, &saved.id, &dest.id)
        .expect_err("must reject label dest");
    assert!(err.contains("not a kokobrain destination"));
    assert!(!*called.lock().unwrap(), "opener must not fire when destination kind is wrong");
}

#[test]
fn route_to_kokobrain_rejects_deleted_capture() {
    let (_dir, store) = temp_store();
    let saved = save_note_with_store(&store, "doomed", CaptureContext::default()).expect("save");
    delete_capture_with_store(&store, &saved.id).expect("delete");
    let dest = kokobrain_dest(&store, "Brain", "Personal");

    let opener = |_uri: &str| -> Result<(), String> { Ok(()) };
    let err = route_to_kokobrain_with_store(&store, &opener, &saved.id, &dest.id)
        .expect_err("must reject deleted capture");
    assert!(err.contains("deleted"));
}

#[test]
fn route_to_kokobrain_rejects_missing_capture() {
    let (_dir, store) = temp_store();
    let dest = kokobrain_dest(&store, "Brain", "Personal");
    let missing_id = ulid::Ulid::new().to_string();

    let opener = |_uri: &str| -> Result<(), String> { Ok(()) };
    let err = route_to_kokobrain_with_store(&store, &opener, &missing_id, &dest.id)
        .expect_err("missing capture must error");
    assert!(err.contains("capture not found"));
}
