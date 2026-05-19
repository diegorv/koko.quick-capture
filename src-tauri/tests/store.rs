//! Integration tests for the capture store. Each test opens a fresh
//! SQLite file inside a tempdir so they cannot collide and never touch
//! the real `~/Library/Application Support` location.

use std::path::PathBuf;
use std::str::FromStr;

use quick_capture_lib::store::{
    cursor_for_archive, CaptureInput, CaptureKind, DestinationKind, ShotSource, Store, StoreError,
    SETTING_LAST_INBOX_OPEN_ID,
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

// ── Destinations + routing (ADR-0010) ─────────────────────────────

#[test]
fn destination_create_then_list_round_trips_name_and_color() {
    let (_dir, store) = temp_store();

    let todoist = store
        .destination_create("Todoist", Some("red"), DestinationKind::Label, None)
        .expect("create todoist");
    let readwise = store
        .destination_create("Readwise", Some("teal"), DestinationKind::Label, None)
        .expect("create readwise");
    let reference = store
        .destination_create("Reference", None, DestinationKind::Label, None)
        .expect("create reference");

    let listed = store.destinations_list_live().expect("list");
    let names: Vec<&str> = listed.iter().map(|d| d.name.as_str()).collect();
    // Alpha-sorted, case-insensitive.
    assert_eq!(names, vec!["Readwise", "Reference", "Todoist"]);

    let by_id: std::collections::HashMap<_, _> =
        listed.iter().map(|d| (d.id.clone(), d.clone())).collect();
    assert_eq!(by_id[&todoist.id].color.as_deref(), Some("red"));
    assert_eq!(by_id[&readwise.id].color.as_deref(), Some("teal"));
    assert_eq!(by_id[&reference.id].color, None);
}

#[test]
fn destination_create_trims_whitespace_and_rejects_blank_name() {
    let (_dir, store) = temp_store();

    let created = store
        .destination_create("  Todoist  ", Some(" red "), DestinationKind::Label, None)
        .expect("create");
    assert_eq!(created.name, "Todoist");
    assert_eq!(created.color.as_deref(), Some("red"));

    let err = store
        .destination_create("   ", None, DestinationKind::Label, None)
        .expect_err("blank name should fail");
    assert!(matches!(err, StoreError::InvalidArgument(_)));
}

#[test]
fn destination_create_kokobrain_round_trips_vault_config() {
    let (_dir, store) = temp_store();

    let dest = store
        .destination_create(
            "Personal Brain",
            Some("teal"),
            DestinationKind::Kokobrain,
            Some(r#"{"vault":"Personal"}"#),
        )
        .expect("create kokobrain dest");

    assert_eq!(dest.kind, DestinationKind::Kokobrain);
    let config = dest.config.as_deref().expect("config persisted");
    let parsed: serde_json::Value = serde_json::from_str(config).expect("config is json");
    assert_eq!(parsed["vault"], "Personal");

    let listed = store.destinations_list_live().expect("list");
    let round_trip = listed
        .iter()
        .find(|d| d.id == dest.id)
        .expect("listed back");
    assert_eq!(round_trip.kind, DestinationKind::Kokobrain);
    assert_eq!(round_trip.config.as_deref(), dest.config.as_deref());
}

#[test]
fn destination_create_kokobrain_requires_vault_config() {
    let (_dir, store) = temp_store();

    let err = store
        .destination_create("Brain", None, DestinationKind::Kokobrain, None)
        .expect_err("missing config should fail");
    assert!(matches!(err, StoreError::InvalidArgument(_)));

    let err = store
        .destination_create(
            "Brain",
            None,
            DestinationKind::Kokobrain,
            Some(r#"{"vault":""}"#),
        )
        .expect_err("blank vault should fail");
    assert!(matches!(err, StoreError::InvalidArgument(_)));

    let err = store
        .destination_create("Brain", None, DestinationKind::Kokobrain, Some("not json"))
        .expect_err("bad json should fail");
    assert!(matches!(err, StoreError::InvalidArgument(_)));
}

#[test]
fn destination_create_label_rejects_config() {
    let (_dir, store) = temp_store();

    let err = store
        .destination_create(
            "Todoist",
            None,
            DestinationKind::Label,
            Some(r#"{"vault":"Personal"}"#),
        )
        .expect_err("label with config should fail");
    assert!(matches!(err, StoreError::InvalidArgument(_)));
}

#[test]
fn destination_update_can_swap_kind_to_kokobrain() {
    let (_dir, store) = temp_store();
    let created = store
        .destination_create("Brain", None, DestinationKind::Label, None)
        .expect("create label");

    store
        .destination_update(
            &created.id,
            "Brain",
            None,
            DestinationKind::Kokobrain,
            Some(r#"{"vault":"Work"}"#),
        )
        .expect("upgrade to kokobrain");

    let after = store
        .destination_find(&created.id)
        .expect("find")
        .expect("present");
    assert_eq!(after.kind, DestinationKind::Kokobrain);
    let parsed: serde_json::Value =
        serde_json::from_str(after.config.as_deref().expect("config")).expect("json");
    assert_eq!(parsed["vault"], "Work");
}

#[test]
fn destination_create_rejects_duplicate_live_name() {
    let (_dir, store) = temp_store();
    store
        .destination_create("Todoist", None, DestinationKind::Label, None)
        .expect("first create");
    let err = store
        .destination_create("Todoist", None, DestinationKind::Label, None)
        .expect_err("dup should fail");
    match err {
        StoreError::DestinationNameConflict(name) => assert_eq!(name, "Todoist"),
        other => panic!("expected conflict, got {other:?}"),
    }
}

#[test]
fn destination_update_renames_and_recolors() {
    let (_dir, store) = temp_store();
    let created = store
        .destination_create("Todoist", Some("red"), DestinationKind::Label, None)
        .expect("create");

    store
        .destination_update(&created.id, "Todoist Inbox", Some("blue"), DestinationKind::Label, None)
        .expect("update");

    let after = store
        .destination_find(&created.id)
        .expect("find")
        .expect("present");
    assert_eq!(after.name, "Todoist Inbox");
    assert_eq!(after.color.as_deref(), Some("blue"));
}

#[test]
fn destination_update_rejects_conflict_with_other_live_name() {
    let (_dir, store) = temp_store();
    store
        .destination_create("Todoist", None, DestinationKind::Label, None)
        .expect("create todoist");
    let readwise = store
        .destination_create("Readwise", None, DestinationKind::Label, None)
        .expect("create readwise");
    let err = store
        .destination_update(&readwise.id, "Todoist", None, DestinationKind::Label, None)
        .expect_err("conflict expected");
    assert!(matches!(err, StoreError::DestinationNameConflict(_)));
}

#[test]
fn destination_soft_delete_hides_from_live_list_but_keeps_row() {
    let (_dir, store) = temp_store();
    let created = store
        .destination_create("Old Project", None, DestinationKind::Label, None)
        .expect("create");
    store
        .destination_soft_delete(&created.id)
        .expect("soft delete");

    let live = store.destinations_list_live().expect("live list");
    assert!(live.is_empty(), "live list must hide soft-deleted");

    let deleted = store
        .destinations_list_deleted()
        .expect("deleted list");
    assert_eq!(deleted.len(), 1);
    assert!(deleted[0].deleted_at.is_some());
    assert_eq!(deleted[0].id, created.id);
}

#[test]
fn destination_soft_delete_then_create_same_name_succeeds() {
    let (_dir, store) = temp_store();
    let first = store
        .destination_create("Reading", None, DestinationKind::Label, None)
        .expect("first create");
    store
        .destination_soft_delete(&first.id)
        .expect("soft delete");
    // Live unique index excludes deleted rows.
    let second = store
        .destination_create("Reading", None, DestinationKind::Label, None)
        .expect("re-create same name");
    assert_ne!(first.id, second.id);
}

#[test]
fn destination_restore_brings_back_when_no_conflict() {
    let (_dir, store) = temp_store();
    let created = store
        .destination_create("Ref", None, DestinationKind::Label, None)
        .expect("create");
    store
        .destination_soft_delete(&created.id)
        .expect("soft delete");
    store.destination_restore(&created.id).expect("restore");

    let live = store.destinations_list_live().expect("live");
    assert_eq!(live.len(), 1);
    assert_eq!(live[0].id, created.id);
    assert!(live[0].deleted_at.is_none());
}

#[test]
fn destination_restore_conflicts_when_name_taken_by_live() {
    let (_dir, store) = temp_store();
    let old = store
        .destination_create("Reading", None, DestinationKind::Label, None)
        .expect("create old");
    store.destination_soft_delete(&old.id).expect("soft delete");
    // New live destination grabs the freed name.
    store
        .destination_create("Reading", None, DestinationKind::Label, None)
        .expect("re-create same name");
    let err = store
        .destination_restore(&old.id)
        .expect_err("restore should conflict");
    assert!(matches!(err, StoreError::DestinationNameConflict(_)));
}

#[test]
fn capture_route_moves_capture_from_inbox_to_archive() {
    let (_dir, store) = temp_store();
    let saved = store
        .save(CaptureInput::Note {
            text: "thought".into(),
        })
        .expect("save");
    let id = Ulid::from_string(&saved.id).expect("parse ulid");
    let dest = store
        .destination_create("Todoist", None, DestinationKind::Label, None)
        .expect("create dest");

    store.capture_route(&id, &dest.id).expect("route");

    let inbox = store.list(10).expect("inbox list");
    assert!(
        inbox.is_empty(),
        "routed capture must leave the Inbox: {inbox:?}"
    );
    let archive = store.list_archive(None, None, 10).expect("archive list");
    assert_eq!(archive.len(), 1);
    assert_eq!(archive[0].id, saved.id);
    assert_eq!(archive[0].destination_id.as_deref(), Some(dest.id.as_str()));
    assert!(archive[0].routed_at.is_some());
    // Routing implies the user interacted with the row.
    assert!(
        archive[0].read_at.is_some(),
        "route must stamp read_at when previously unread"
    );
}

#[test]
fn capture_unroute_returns_capture_to_inbox() {
    let (_dir, store) = temp_store();
    let saved = store
        .save(CaptureInput::Note { text: "x".into() })
        .expect("save");
    let id = Ulid::from_string(&saved.id).expect("parse");
    let dest = store
        .destination_create("Readwise", None, DestinationKind::Label, None)
        .expect("create dest");
    store.capture_route(&id, &dest.id).expect("route");
    store.capture_unroute(&id).expect("unroute");

    let inbox = store.list(10).expect("inbox");
    assert_eq!(inbox.len(), 1);
    let archive = store.list_archive(None, None, 10).expect("archive");
    assert!(archive.is_empty());
    let row = &inbox[0];
    assert!(row.destination_id.is_none());
    assert!(row.routed_at.is_none());
    // read_at survives the unroute.
    assert!(row.read_at.is_some());
}

#[test]
fn capture_reroute_updates_destination_and_routed_at() {
    let (_dir, store) = temp_store();
    let saved = store
        .save(CaptureInput::Note { text: "x".into() })
        .expect("save");
    let id = Ulid::from_string(&saved.id).expect("parse");
    let first = store
        .destination_create("Todoist", None, DestinationKind::Label, None)
        .expect("create first");
    let second = store
        .destination_create("Readwise", None, DestinationKind::Label, None)
        .expect("create second");
    store.capture_route(&id, &first.id).expect("route first");
    let after_first = store.list_archive(None, None, 10).expect("archive")[0]
        .routed_at
        .clone()
        .expect("routed_at");
    // Sleep so the second routed_at timestamp differs.
    std::thread::sleep(std::time::Duration::from_millis(5));
    store.capture_route(&id, &second.id).expect("route second");

    let archive = store.list_archive(None, None, 10).expect("archive");
    assert_eq!(archive.len(), 1);
    assert_eq!(
        archive[0].destination_id.as_deref(),
        Some(second.id.as_str())
    );
    assert!(
        archive[0].routed_at.as_deref().unwrap() >= after_first.as_str(),
        "re-route must bump routed_at, got {:?} >= {after_first}",
        archive[0].routed_at
    );
}

#[test]
fn capture_route_rejects_soft_deleted_destination() {
    let (_dir, store) = temp_store();
    let saved = store
        .save(CaptureInput::Note { text: "x".into() })
        .expect("save");
    let id = Ulid::from_string(&saved.id).expect("parse");
    let dest = store
        .destination_create("Old", None, DestinationKind::Label, None)
        .expect("create dest");
    store.destination_soft_delete(&dest.id).expect("soft delete");

    let err = store
        .capture_route(&id, &dest.id)
        .expect_err("routing to soft-deleted dest should fail");
    assert!(matches!(err, StoreError::InvalidArgument(_)));
}

#[test]
fn capture_route_rejects_unknown_destination() {
    let (_dir, store) = temp_store();
    let saved = store
        .save(CaptureInput::Note { text: "x".into() })
        .expect("save");
    let id = Ulid::from_string(&saved.id).expect("parse");
    let err = store
        .capture_route(&id, "00000000000000000000000000")
        .expect_err("unknown dest must fail");
    assert!(matches!(err, StoreError::NotFound(_)));
}

#[test]
fn routed_capture_keeps_destination_reference_after_soft_delete() {
    let (_dir, store) = temp_store();
    let saved = store
        .save(CaptureInput::Note { text: "x".into() })
        .expect("save");
    let id = Ulid::from_string(&saved.id).expect("parse");
    let dest = store
        .destination_create("Todoist", None, DestinationKind::Label, None)
        .expect("create dest");
    store.capture_route(&id, &dest.id).expect("route");
    store.destination_soft_delete(&dest.id).expect("soft delete");

    // Orphan still surfaces in the Archive with its original ref.
    let archive = store.list_archive(None, None, 10).expect("archive");
    assert_eq!(archive.len(), 1);
    assert_eq!(archive[0].destination_id.as_deref(), Some(dest.id.as_str()));
}

#[test]
fn count_inbox_excludes_routed_and_deleted() {
    let (_dir, store) = temp_store();
    let kept = store
        .save(CaptureInput::Note { text: "kept".into() })
        .expect("save kept");
    let routed = store
        .save(CaptureInput::Note { text: "routed".into() })
        .expect("save routed");
    let dropped = store
        .save(CaptureInput::Note {
            text: "dropped".into(),
        })
        .expect("save dropped");
    let dest = store
        .destination_create("Todoist", None, DestinationKind::Label, None)
        .expect("create dest");
    let routed_id = Ulid::from_string(&routed.id).expect("parse");
    store.capture_route(&routed_id, &dest.id).expect("route");
    let dropped_id = Ulid::from_string(&dropped.id).expect("parse");
    store.soft_delete(&dropped_id).expect("soft delete");

    let _ = kept; // silence unused warning; the inbox count below covers it.
    assert_eq!(store.count_inbox().expect("count"), 1);
}

#[test]
fn search_excludes_routed_captures_from_inbox_results() {
    let (_dir, store) = temp_store();
    let kept = store
        .save(CaptureInput::Note {
            text: "alpha bravo charlie".into(),
        })
        .expect("save kept");
    let routed = store
        .save(CaptureInput::Note {
            text: "alpha bravo delta".into(),
        })
        .expect("save routed");
    let dest = store
        .destination_create("Todoist", None, DestinationKind::Label, None)
        .expect("create dest");
    let routed_id = Ulid::from_string(&routed.id).expect("parse");
    store.capture_route(&routed_id, &dest.id).expect("route");

    let inbox_hits = store.search("alpha", 10).expect("inbox search");
    let inbox_ids: Vec<&str> = inbox_hits.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(inbox_ids, vec![kept.id.as_str()]);

    let archive_hits = store
        .search_archive("alpha", None, 10)
        .expect("archive search");
    let archive_ids: Vec<&str> = archive_hits.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(archive_ids, vec![routed.id.as_str()]);
}

#[test]
fn list_archive_filters_by_destination() {
    let (_dir, store) = temp_store();
    let dest_a = store.destination_create("A", None, DestinationKind::Label, None).expect("a");
    let dest_b = store.destination_create("B", None, DestinationKind::Label, None).expect("b");
    let a1 = store
        .save(CaptureInput::Note { text: "a1".into() })
        .expect("save a1");
    let b1 = store
        .save(CaptureInput::Note { text: "b1".into() })
        .expect("save b1");
    store
        .capture_route(&Ulid::from_string(&a1.id).expect("ulid"), &dest_a.id)
        .expect("route a1");
    store
        .capture_route(&Ulid::from_string(&b1.id).expect("ulid"), &dest_b.id)
        .expect("route b1");

    let only_a = store
        .list_archive(Some(&dest_a.id), None, 10)
        .expect("filter a");
    assert_eq!(only_a.len(), 1);
    assert_eq!(only_a[0].id, a1.id);

    let only_b = store
        .list_archive(Some(&dest_b.id), None, 10)
        .expect("filter b");
    assert_eq!(only_b.len(), 1);
    assert_eq!(only_b[0].id, b1.id);

    let all = store.list_archive(None, None, 10).expect("all archive");
    assert_eq!(all.len(), 2);
}

#[test]
fn list_archive_paginates_through_routed_at_id_cursor() {
    let (_dir, store) = temp_store();
    let dest = store.destination_create("D", None, DestinationKind::Label, None).expect("dest");

    // Seed 12 captures, route each with a 2ms gap so routed_at strictly
    // increases — same trick as the inbox cursor test.
    let mut ids = Vec::with_capacity(12);
    for i in 0..12 {
        let saved = store
            .save(CaptureInput::Note { text: format!("n{i}") })
            .expect("save");
        let ulid = Ulid::from_string(&saved.id).expect("ulid");
        store.capture_route(&ulid, &dest.id).expect("route");
        ids.push(saved.id);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    ids.reverse(); // newest-routed first

    let first = store.list_archive(None, None, 5).expect("page 1");
    assert_eq!(first.len(), 5);
    assert_eq!(
        first.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
        ids[..5].iter().map(String::as_str).collect::<Vec<_>>(),
    );

    let cursor = cursor_for_archive(first.last().unwrap()).expect("cursor");
    let second = store
        .list_archive(None, Some(&cursor), 5)
        .expect("page 2");
    assert_eq!(second.len(), 5);
    assert_eq!(
        second.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
        ids[5..10].iter().map(String::as_str).collect::<Vec<_>>(),
    );

    let cursor2 = cursor_for_archive(second.last().unwrap()).expect("cursor2");
    let third = store
        .list_archive(None, Some(&cursor2), 5)
        .expect("page 3");
    // Only 2 left.
    assert_eq!(third.len(), 2);
    assert_eq!(
        third.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
        ids[10..].iter().map(String::as_str).collect::<Vec<_>>(),
    );
}

#[test]
fn list_archive_rejects_malformed_cursor() {
    let (_dir, store) = temp_store();
    let err = store
        .list_archive(None, Some("no-pipe-here"), 10)
        .expect_err("bad cursor");
    assert!(matches!(err, StoreError::Decode(_)));
}

#[test]
fn destination_id_foreign_key_rejects_unknown_id() {
    // Catch the schema FK being off (would silently allow orphan
    // routes).
    let (_dir, store) = temp_store();
    let saved = store
        .save(CaptureInput::Note { text: "x".into() })
        .expect("save");
    let id = Ulid::from_string(&saved.id).expect("parse");
    let err = store
        .capture_route(&id, "ZZZZZZZZZZZZZZZZZZZZZZZZZZ")
        .expect_err("must reject");
    // We mapped this to NotFound at the Rust layer (we look up the
    // destination first) so the SQLite FK never fires; but if the
    // lookup is ever removed, the FK is the second line of defence.
    assert!(matches!(err, StoreError::NotFound(_)));
}
