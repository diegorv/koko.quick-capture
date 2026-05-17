//! Integration tests for the capture store. Each test opens a fresh
//! SQLite file inside a tempdir so they cannot collide and never touch
//! the real `~/Library/Application Support` location.

use std::path::PathBuf;
use std::str::FromStr;

use quick_capture_lib::store::{
    CaptureInput, CaptureKind, ShotSource, Store, SETTING_LAST_INBOX_OPEN_ID,
};
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
fn save_shot_bytes_writes_blob_under_ulid_name() {
    let dir = tempfile::tempdir().expect("create tempdir");
    let db_path = dir.path().join("captures.db");
    let store = Store::open(&db_path).expect("open store");
    let bytes = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0xFF, 0xEE];

    let saved = store
        .save(CaptureInput::Shot {
            source: ShotSource::Bytes {
                bytes: bytes.clone(),
                mime: "image/png".into(),
            },
            width: Some(64),
            height: Some(48),
        })
        .expect("save shot");

    assert_eq!(saved.kind, CaptureKind::Shot);
    assert_eq!(
        saved.payload.get("mime").and_then(|v| v.as_str()),
        Some("image/png")
    );
    assert_eq!(saved.payload.get("width").and_then(|v| v.as_u64()), Some(64));
    assert_eq!(
        saved.payload.get("height").and_then(|v| v.as_u64()),
        Some(48)
    );
    let blob_path = saved
        .payload
        .get("blob_path")
        .and_then(|v| v.as_str())
        .expect("blob_path must be present");
    let blob_path = PathBuf::from(blob_path);

    assert!(blob_path.exists(), "expected blob file at {blob_path:?}");
    assert_eq!(
        blob_path.parent().expect("blob parent"),
        dir.path().join("blobs"),
        "blobs/ must sit next to the db file"
    );
    assert_eq!(
        blob_path.file_name().and_then(|n| n.to_str()),
        Some(format!("{}.png", saved.id).as_str()),
        "blob filename stem must be the capture ULID"
    );
    let on_disk = std::fs::read(&blob_path).expect("read blob");
    assert_eq!(on_disk, bytes, "blob bytes must match input bytes");
}

#[test]
fn save_shot_path_does_not_copy_file() {
    let dir = tempfile::tempdir().expect("create tempdir");
    let db_path = dir.path().join("captures.db");
    let store = Store::open(&db_path).expect("open store");

    let saved = store
        .save(CaptureInput::Shot {
            source: ShotSource::Path {
                source_path: PathBuf::from("/Users/me/screenshot.png"),
                mime: "image/png".into(),
            },
            width: None,
            height: None,
        })
        .expect("save shot path");

    assert_eq!(saved.kind, CaptureKind::Shot);
    assert_eq!(
        saved.payload.get("source_path").and_then(|v| v.as_str()),
        Some("/Users/me/screenshot.png")
    );
    assert_eq!(
        saved.payload.get("mime").and_then(|v| v.as_str()),
        Some("image/png")
    );
    assert!(
        saved.payload.get("blob_path").is_none(),
        "path-flavor Shot must not record a blob_path"
    );
    let blobs_dir = dir.path().join("blobs");
    assert!(
        !blobs_dir.exists() || std::fs::read_dir(&blobs_dir).map(|r| r.count()).unwrap_or(0) == 0,
        "no blob file must be created when source is a path"
    );
}

#[test]
fn save_file_records_source_path_and_original_name() {
    let (_dir, store) = temp_store();

    let saved = store
        .save(CaptureInput::File {
            source_path: PathBuf::from("/Users/me/notes.pdf"),
            mime: "application/pdf".into(),
            original_name: Some("notes.pdf".into()),
        })
        .expect("save file");

    assert_eq!(saved.kind, CaptureKind::File);
    assert_eq!(
        saved.payload.get("source_path").and_then(|v| v.as_str()),
        Some("/Users/me/notes.pdf")
    );
    assert_eq!(
        saved.payload.get("mime").and_then(|v| v.as_str()),
        Some("application/pdf")
    );
    assert_eq!(
        saved.payload.get("original_name").and_then(|v| v.as_str()),
        Some("notes.pdf")
    );
}

#[test]
fn list_before_pages_through_cursor_in_descending_order() {
    let (_dir, store) = temp_store();

    // Seed 60 Notes. The ULID crate (v1) does not guarantee monotonic
    // ids within the same millisecond, so we sleep 2ms between saves
    // to force strictly increasing timestamps and a deterministic
    // descending order at read time.
    let mut ids = Vec::with_capacity(60);
    for i in 0..60 {
        let saved = store
            .save(CaptureInput::Note {
                text: format!("note {i}"),
            })
            .expect("save note");
        ids.push(saved.id);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    ids.reverse(); // now ids[0] is the newest

    // First page: cursor=None, limit=50 -> 50 newest in descending order.
    let first = store.list_before(None, 50).expect("first page");
    assert_eq!(first.len(), 50);
    for (i, row) in first.iter().enumerate() {
        assert_eq!(row.id, ids[i], "first page must be newest-first");
    }

    // Default ordering matches `list`.
    let default_list = store.list(50).expect("list");
    let first_ids: Vec<&String> = first.iter().map(|c| &c.id).collect();
    let default_ids: Vec<&String> = default_list.iter().map(|c| &c.id).collect();
    assert_eq!(first_ids, default_ids, "list must mirror list_before(None)");

    // Second page: cursor = last id of first page, limit=50 -> remaining 10.
    let last_first = Ulid::from_str(&first.last().expect("non-empty first page").id)
        .expect("parse ulid");
    let second = store
        .list_before(Some(last_first), 50)
        .expect("second page");
    assert_eq!(second.len(), 10, "second page must hold the remaining 10");
    for (i, row) in second.iter().enumerate() {
        assert_eq!(row.id, ids[50 + i], "second page continues descending");
    }

    // Past the end: cursor = last id of second page returns an empty page.
    let last_second = Ulid::from_str(&second.last().expect("non-empty second page").id)
        .expect("parse ulid");
    let third = store
        .list_before(Some(last_second), 50)
        .expect("third page");
    assert!(third.is_empty(), "no more rows after the second page");
}

#[test]
fn list_before_omits_soft_deleted_rows() {
    let (_dir, store) = temp_store();

    // Sleep 2ms between saves so ids are strictly increasing despite
    // ULID v1 not guaranteeing intra-millisecond monotonicity.
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

    // Soft-delete the middle row.
    let b_id = Ulid::from_str(&b.id).expect("parse ulid");
    store.soft_delete(&b_id).expect("soft delete");

    let listed = store.list_before(None, 50).expect("list");
    let ids: Vec<&str> = listed.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(
        ids,
        vec![c.id.as_str(), a.id.as_str()],
        "tombstones must not surface in list_before"
    );
}

#[test]
fn settings_get_returns_none_for_missing_key() {
    let (_dir, store) = temp_store();
    let value = store.settings_get("never_written").expect("get");
    assert!(value.is_none(), "missing key must read as None");
}

#[test]
fn settings_set_then_get_round_trip() {
    let (_dir, store) = temp_store();
    store
        .settings_set(SETTING_LAST_INBOX_OPEN_ID, "01HQXY1234567890ABCDEFGHJK")
        .expect("set");
    let value = store
        .settings_get(SETTING_LAST_INBOX_OPEN_ID)
        .expect("get")
        .expect("value present");
    assert_eq!(value, "01HQXY1234567890ABCDEFGHJK");
}

#[test]
fn settings_set_overwrites_existing_value() {
    let (_dir, store) = temp_store();
    store.settings_set("k", "first").expect("first set");
    store.settings_set("k", "second").expect("second set");
    let value = store.settings_get("k").expect("get").expect("present");
    assert_eq!(value, "second", "second set must overwrite first");
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

#[test]
fn save_writes_dump_json_next_to_db() {
    let (dir, store) = temp_store();
    let saved = store
        .save(CaptureInput::Note {
            text: "dump me".into(),
        })
        .expect("save");

    let dump_path = dir.path().join("dumps").join(format!("{}.json", saved.id));
    assert!(dump_path.exists(), "dump file must exist at {dump_path:?}");
    let json = std::fs::read_to_string(&dump_path).expect("read dump");
    let value: serde_json::Value = serde_json::from_str(&json).expect("parse dump");
    assert_eq!(value.get("id").and_then(|v| v.as_str()), Some(saved.id.as_str()));
    assert_eq!(value.get("kind").and_then(|v| v.as_str()), Some("Note"));
    assert!(value.get("deleted_at").map(|v| v.is_null()).unwrap_or(false));
}

#[test]
fn soft_delete_keeps_dump_and_stamps_deleted_at() {
    let (dir, store) = temp_store();
    let saved = store
        .save(CaptureInput::Note { text: "kill".into() })
        .expect("save");
    let id = Ulid::from_string(&saved.id).expect("parse");
    store.soft_delete(&id).expect("soft delete");

    let dump_path = dir.path().join("dumps").join(format!("{}.json", saved.id));
    assert!(dump_path.exists(), "dump must survive a soft-delete");
    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&dump_path).expect("read")).expect("parse");
    assert!(
        value.get("deleted_at").and_then(|v| v.as_str()).is_some(),
        "deleted_at must be a timestamp string post-soft-delete, got: {value}"
    );
}

#[test]
fn set_star_refreshes_dump() {
    let (dir, store) = temp_store();
    let saved = store
        .save(CaptureInput::Note { text: "x".into() })
        .expect("save");
    let id = Ulid::from_string(&saved.id).expect("parse");
    store.set_star(&id, true).expect("star");

    let dump_path = dir.path().join("dumps").join(format!("{}.json", saved.id));
    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&dump_path).expect("read")).expect("parse");
    assert_eq!(value.get("starred").and_then(|v| v.as_bool()), Some(true));
}

#[test]
fn mark_read_refreshes_dump() {
    let (dir, store) = temp_store();
    let saved = store
        .save(CaptureInput::Note { text: "r".into() })
        .expect("save");
    let id = Ulid::from_string(&saved.id).expect("parse");
    let flipped = store.mark_read(&id).expect("mark read");
    assert!(flipped);

    let dump_path = dir.path().join("dumps").join(format!("{}.json", saved.id));
    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&dump_path).expect("read")).expect("parse");
    assert!(
        value.get("read_at").and_then(|v| v.as_str()).is_some(),
        "read_at must be stamped, got: {value}"
    );
}
