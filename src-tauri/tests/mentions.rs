//! Integration tests for the `[[Name]]` mentions index. Covers the
//! save-time hook in `save_with_context` and the one-shot backfill
//! scan that fires at `Store::open()` when the index is empty but
//! captures exist.

use std::str::FromStr;

use quick_capture_lib::commands::{
    create_destination_with_store, list_archive_with_store, list_capture_mentions_with_store,
    list_captures_with_store, route_capture_with_store, search_archive_with_store,
    search_captures_with_store,
};
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

// ── slice 2: query path ───────────────────────────────────────────

#[test]
fn list_capture_mentions_round_trips_via_the_command() {
    let (_dir, store) = temp_store();
    let id = save_note(&store, "[[Ana]] and [[Diego]]");
    let names = list_capture_mentions_with_store(&store, &id.to_string()).unwrap();
    assert_eq!(names, vec!["Ana", "Diego"]);
}

#[test]
fn list_capture_mentions_rejects_invalid_id() {
    let (_dir, store) = temp_store();
    let err = list_capture_mentions_with_store(&store, "not-a-ulid").unwrap_err();
    assert!(err.to_lowercase().contains("invalid id"));
}

#[test]
fn inbox_list_filter_narrows_to_captures_mentioning_a_person() {
    let (_dir, store) = temp_store();
    let with_diego = save_note(&store, "ping [[Diego]] tonight");
    std::thread::sleep(std::time::Duration::from_millis(2));
    let _without = save_note(&store, "no wikilink here");
    std::thread::sleep(std::time::Duration::from_millis(2));
    let with_ana = save_note(&store, "[[Ana]] meets [[Diego]]");

    let by_diego = list_captures_with_store(&store, None, Some("diego"), 10).unwrap();
    let ids: Vec<&str> = by_diego.iter().map(|c| c.id.as_str()).collect();
    // Newest-first: with_ana then with_diego.
    assert_eq!(
        ids,
        vec![with_ana.to_string().as_str(), with_diego.to_string().as_str()]
    );

    // Unrelated person yields no rows.
    let by_unknown = list_captures_with_store(&store, None, Some("nobody"), 10).unwrap();
    assert!(by_unknown.is_empty());
}

#[test]
fn inbox_search_filter_intersects_query_with_mention() {
    let (_dir, store) = temp_store();
    let _routine =
        save_note(&store, "routine standup notes, no wikilink");
    let _with_diego_no_alpha =
        save_note(&store, "ping [[Diego]] about beta release");
    std::thread::sleep(std::time::Duration::from_millis(2));
    let alpha_diego = save_note(&store, "alpha review with [[Diego]]");
    std::thread::sleep(std::time::Duration::from_millis(2));
    let _alpha_no_one =
        save_note(&store, "alpha review with nobody mentioned");

    // Search for "alpha" filtered by mention=Diego → only the
    // capture that has both wins.
    let hits =
        search_captures_with_store(&store, "alpha", Some("Diego"), 10).unwrap();
    let ids: Vec<&str> = hits.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, vec![alpha_diego.to_string().as_str()]);
}

#[test]
fn archive_list_filter_narrows_to_routed_captures_mentioning_a_person() {
    let (_dir, store) = temp_store();
    let dest = create_destination_with_store(&store, "Todoist", None).unwrap();

    let routed_diego = save_note(&store, "follow up with [[Diego]]");
    let routed_no_one = save_note(&store, "follow up with nobody");
    let inbox_diego = save_note(&store, "fresh note with [[Diego]]");

    route_capture_with_store(&store, &routed_diego.to_string(), &dest.id).unwrap();
    route_capture_with_store(&store, &routed_no_one.to_string(), &dest.id).unwrap();

    let by_diego =
        list_archive_with_store(&store, None, Some("diego"), None, 10).unwrap();
    let ids: Vec<&str> = by_diego.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, vec![routed_diego.to_string().as_str()]);
    // The Inbox row that also mentions Diego must not leak into the
    // Archive filter.
    assert!(!ids.contains(&inbox_diego.to_string().as_str()));
}

#[test]
fn archive_search_filter_intersects_query_destination_and_mention() {
    let (_dir, store) = temp_store();
    let dest_a = create_destination_with_store(&store, "A", None).unwrap();
    let dest_b = create_destination_with_store(&store, "B", None).unwrap();

    let want = save_note(&store, "alpha review with [[Diego]]");
    let routed_b_alpha_diego =
        save_note(&store, "alpha sync with [[Diego]]");
    let routed_a_other = save_note(&store, "alpha with [[Ana]]");
    let routed_a_no_alpha = save_note(&store, "beta with [[Diego]]");

    route_capture_with_store(&store, &want.to_string(), &dest_a.id).unwrap();
    route_capture_with_store(&store, &routed_b_alpha_diego.to_string(), &dest_b.id).unwrap();
    route_capture_with_store(&store, &routed_a_other.to_string(), &dest_a.id).unwrap();
    route_capture_with_store(&store, &routed_a_no_alpha.to_string(), &dest_a.id).unwrap();

    let hits = search_archive_with_store(
        &store,
        "alpha",
        Some(&dest_a.id),
        Some("Diego"),
        10,
    )
    .unwrap();
    let ids: Vec<&str> = hits.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, vec![want.to_string().as_str()]);
}

#[test]
fn mention_filter_is_case_insensitive_on_both_sides() {
    let (_dir, store) = temp_store();
    save_note(&store, "[[Diego]] one");

    // Query with all-uppercase still finds the lowercased name in
    // the index.
    let hits = list_captures_with_store(&store, None, Some("DIEGO"), 10).unwrap();
    assert_eq!(hits.len(), 1);

    // Mixed case capture name + mixed case query also match.
    save_note(&store, "[[diego]] two");
    let hits = list_captures_with_store(&store, None, Some("Diego"), 10).unwrap();
    assert_eq!(hits.len(), 2);
}
