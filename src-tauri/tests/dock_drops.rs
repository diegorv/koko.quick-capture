//! Integration tests for `save_dropped_files_with_store`. Drives the
//! helper directly (the function the `save_dropped_files` Tauri command
//! delegates to) against a temp `Store` so we exercise the
//! `drag_drop::decide_dropped_files` -> `store::save` composition end to
//! end without spinning up a Tauri runtime, mirroring the
//! `capture_clipboard_now_with` test layout from slice 05.

use std::path::PathBuf;

use quick_capture_lib::commands::save_dropped_files_with_store;
use quick_capture_lib::store::{CaptureKind, Store};
use tempfile::TempDir;

fn temp_store() -> (TempDir, Store) {
    let dir = tempfile::tempdir().expect("create tempdir");
    let path = dir.path().join("captures.db");
    let store = Store::open(&path).expect("open store");
    (dir, store)
}

#[test]
fn save_dropped_files_with_store_persists_each_file() {
    let (_dir, store) = temp_store();
    let paths = vec![
        PathBuf::from("/tmp/screenshot.png"),
        PathBuf::from("/tmp/notes.pdf"),
    ];

    let saved =
        save_dropped_files_with_store(&store, paths).expect("save should succeed");

    assert_eq!(saved.len(), 2, "expected two captures, got {saved:?}");
    assert_eq!(saved[0].kind, CaptureKind::Shot);
    assert_eq!(saved[1].kind, CaptureKind::File);

    // Shot path-flavor: source_path + mime, no blob_path.
    assert_eq!(
        saved[0].payload.get("source_path").and_then(|v| v.as_str()),
        Some("/tmp/screenshot.png")
    );
    assert_eq!(
        saved[0].payload.get("mime").and_then(|v| v.as_str()),
        Some("image/png")
    );
    assert!(
        saved[0].payload.get("blob_path").is_none(),
        "Shot from a Finder drop must reference source_path, never copy into blobs/"
    );

    // File: source_path + mime + original_name.
    assert_eq!(
        saved[1].payload.get("source_path").and_then(|v| v.as_str()),
        Some("/tmp/notes.pdf")
    );
    assert_eq!(
        saved[1].payload.get("mime").and_then(|v| v.as_str()),
        Some("application/pdf")
    );
    assert_eq!(
        saved[1].payload.get("original_name").and_then(|v| v.as_str()),
        Some("notes.pdf")
    );

    let listed = store.list(10).expect("list");
    assert_eq!(listed.len(), 2, "two rows must land in the store");
    // Two `Ulid::new()` calls inside the same millisecond are not
    // strictly time-sortable, so assert set-membership rather than the
    // list ordering. Input-order preservation in `saved` is the
    // load-bearing contract here and is asserted by the kind / payload
    // checks above.
    let listed_ids: std::collections::HashSet<&str> =
        listed.iter().map(|c| c.id.as_str()).collect();
    assert!(listed_ids.contains(saved[0].id.as_str()));
    assert!(listed_ids.contains(saved[1].id.as_str()));
}

#[test]
fn save_dropped_files_with_store_empty_list_errors_and_writes_nothing() {
    let (_dir, store) = temp_store();

    let err = save_dropped_files_with_store(&store, vec![])
        .expect_err("empty list must error");
    assert!(
        err.to_lowercase().contains("empty"),
        "expected error to mention empty, got: {err}"
    );

    let listed = store.list(10).expect("list");
    assert!(listed.is_empty(), "no row must be written, got {listed:?}");
}
