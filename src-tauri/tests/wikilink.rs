//! Integration tests for the Wikilink source-folder commands.
//!
//! Drives the `*_with_store` helpers (no Tauri runtime needed) so we
//! cover the settings round-trip, validation rejection, and the
//! "configured but missing/empty on disk" silent-empty contract.

use std::fs;

use quick_capture_lib::commands::{
    get_wikilink_source_folder_with_store, list_people_with_store,
    set_wikilink_source_folder_with_store,
};
use quick_capture_lib::store::Store;
use tempfile::{tempdir, TempDir};

fn temp_store() -> (TempDir, Store) {
    let dir = tempdir().expect("create tempdir");
    let path = dir.path().join("captures.db");
    let store = Store::open(&path).expect("open store");
    (dir, store)
}

#[test]
fn set_and_get_round_trips() {
    let (_db, store) = temp_store();
    let people = tempdir().unwrap();
    let people_str = people.path().to_string_lossy().into_owned();

    set_wikilink_source_folder_with_store(&store, Some(&people_str)).unwrap();
    let got = get_wikilink_source_folder_with_store(&store).unwrap();
    assert_eq!(got.as_deref(), Some(people_str.as_str()));
}

#[test]
fn get_returns_none_when_never_set() {
    let (_db, store) = temp_store();
    assert!(get_wikilink_source_folder_with_store(&store).unwrap().is_none());
}

#[test]
fn set_rejects_nonexistent_path() {
    let (_db, store) = temp_store();
    let err = set_wikilink_source_folder_with_store(
        &store,
        Some("/this/path/does-not-exist-anywhere"),
    )
    .unwrap_err();
    assert!(err.contains("folder does not exist") || err.contains("io error"));
}

#[test]
fn set_rejects_file_paths() {
    let (_db, store) = temp_store();
    let dir = tempdir().unwrap();
    let file = dir.path().join("not-a-folder.md");
    fs::write(&file, "").unwrap();
    let err = set_wikilink_source_folder_with_store(
        &store,
        Some(&file.to_string_lossy()),
    )
    .unwrap_err();
    assert!(err.contains("not a directory"));
}

#[test]
fn clear_writes_empty_sentinel_and_get_returns_none() {
    let (_db, store) = temp_store();
    let people = tempdir().unwrap();
    set_wikilink_source_folder_with_store(
        &store,
        Some(&people.path().to_string_lossy()),
    )
    .unwrap();

    set_wikilink_source_folder_with_store(&store, None).unwrap();
    assert!(get_wikilink_source_folder_with_store(&store).unwrap().is_none());

    set_wikilink_source_folder_with_store(
        &store,
        Some(&people.path().to_string_lossy()),
    )
    .unwrap();
    set_wikilink_source_folder_with_store(&store, Some("")).unwrap();
    assert!(get_wikilink_source_folder_with_store(&store).unwrap().is_none());
}

#[test]
fn list_people_returns_empty_when_unset() {
    let (_db, store) = temp_store();
    let rows = list_people_with_store(&store).unwrap();
    assert!(rows.is_empty());
}

#[test]
fn list_people_returns_sorted_md_files_when_set() {
    let (_db, store) = temp_store();
    let people = tempdir().unwrap();
    fs::write(people.path().join("Diego.md"), "").unwrap();
    fs::write(people.path().join("ana beatriz.md"), "").unwrap();
    fs::write(people.path().join("ignored.txt"), "").unwrap();
    fs::write(people.path().join(".DS_Store"), "").unwrap();
    fs::create_dir(people.path().join("subdir")).unwrap();

    set_wikilink_source_folder_with_store(
        &store,
        Some(&people.path().to_string_lossy()),
    )
    .unwrap();

    let rows = list_people_with_store(&store).unwrap();
    let names: Vec<_> = rows.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, vec!["ana beatriz", "Diego"]);
}

#[test]
fn list_people_silently_empty_when_folder_disappears() {
    // Q9 (option b): set-but-missing on disk → empty popup state, not
    // a hard error. The Settings page can still surface "folder
    // missing" because it shows the *configured* path separately.
    let (_db, store) = temp_store();
    let people = tempdir().unwrap();
    set_wikilink_source_folder_with_store(
        &store,
        Some(&people.path().to_string_lossy()),
    )
    .unwrap();
    drop(people); // delete folder on disk

    let rows = list_people_with_store(&store).unwrap();
    assert!(rows.is_empty());
}
