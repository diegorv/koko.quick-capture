//! Integration tests for `open_link_with` and `reveal_capture_with`.
//! We drive the helper functions directly (the functions the Tauri
//! commands delegate to) against a `FakeShell` + temp `Store` so we
//! exercise the kind-routing logic without spawning a real `open(1)`
//! subprocess, mirroring how slice 05 tested clipboard composition.

use std::path::PathBuf;

use quick_capture_lib::commands::{open_link_with, reveal_capture_with};
use quick_capture_lib::shell::{FakeShell, ShellCall};
use quick_capture_lib::store::{CaptureInput, ShotSource, Store};
use tempfile::TempDir;

fn temp_store() -> (TempDir, Store) {
    let dir = tempfile::tempdir().expect("create tempdir");
    let path = dir.path().join("captures.db");
    let store = Store::open(&path).expect("open store");
    (dir, store)
}

#[test]
fn open_link_with_calls_open_in_browser_with_url() {
    let shell = FakeShell::new();
    open_link_with(&shell, "https://example.com").expect("open_link should succeed");
    assert_eq!(
        shell.calls(),
        vec![ShellCall::OpenInBrowser("https://example.com".into())]
    );
}

#[test]
fn reveal_capture_with_routes_link_to_browser() {
    let (_dir, store) = temp_store();
    let saved = store
        .save(CaptureInput::Link {
            url: "https://example.com/page".into(),
            raw_text: "https://example.com/page".into(),
            title: None,
        })
        .expect("save link");

    let shell = FakeShell::new();
    reveal_capture_with(&shell, &store, &saved.id).expect("reveal should succeed");

    assert_eq!(
        shell.calls(),
        vec![ShellCall::OpenInBrowser(
            "https://example.com/page".into()
        )]
    );
}

#[test]
fn reveal_capture_with_routes_file_to_finder() {
    let (_dir, store) = temp_store();
    let saved = store
        .save(CaptureInput::File {
            source_path: PathBuf::from("/tmp/notes.pdf"),
            mime: "application/pdf".into(),
            original_name: Some("notes.pdf".into()),
        })
        .expect("save file");

    let shell = FakeShell::new();
    reveal_capture_with(&shell, &store, &saved.id).expect("reveal should succeed");

    assert_eq!(
        shell.calls(),
        vec![ShellCall::RevealInFinder(PathBuf::from("/tmp/notes.pdf"))]
    );
}

#[test]
fn reveal_capture_with_routes_path_shot_to_finder() {
    let (_dir, store) = temp_store();
    let saved = store
        .save(CaptureInput::Shot {
            source: ShotSource::Path {
                source_path: PathBuf::from("/tmp/screenshot.png"),
                mime: "image/png".into(),
            },
            width: None,
            height: None,
        })
        .expect("save path-shot");

    let shell = FakeShell::new();
    reveal_capture_with(&shell, &store, &saved.id).expect("reveal should succeed");

    assert_eq!(
        shell.calls(),
        vec![ShellCall::RevealInFinder(PathBuf::from(
            "/tmp/screenshot.png"
        ))]
    );
}

#[test]
fn reveal_capture_with_routes_bytes_shot_to_open_path() {
    let (_dir, store) = temp_store();
    // 8-byte PNG signature is enough; the store does not decode bytes,
    // it just writes them to `blobs/<ulid>.png`.
    let saved = store
        .save(CaptureInput::Shot {
            source: ShotSource::Bytes {
                bytes: vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A],
                mime: "image/png".into(),
            },
            width: None,
            height: None,
        })
        .expect("save bytes-shot");

    let expected_blob = saved
        .payload
        .get("blob_path")
        .and_then(|v| v.as_str())
        .map(PathBuf::from)
        .expect("bytes-flavor Shot must have a blob_path");

    let shell = FakeShell::new();
    reveal_capture_with(&shell, &store, &saved.id).expect("reveal should succeed");

    assert_eq!(shell.calls(), vec![ShellCall::OpenPath(expected_blob)]);
}

#[test]
fn reveal_capture_with_rejects_clip_and_note() {
    let (_dir, store) = temp_store();
    let clip = store
        .save(CaptureInput::Clip {
            text: "some clipboard text".into(),
        })
        .expect("save clip");
    let note = store
        .save(CaptureInput::Note {
            text: "a note".into(),
        })
        .expect("save note");

    let shell = FakeShell::new();
    let err = reveal_capture_with(&shell, &store, &clip.id)
        .expect_err("clip must be rejected");
    assert!(
        err.to_lowercase().contains("clip"),
        "expected error to mention clip, got: {err}"
    );

    let err = reveal_capture_with(&shell, &store, &note.id)
        .expect_err("note must be rejected");
    assert!(
        err.to_lowercase().contains("note"),
        "expected error to mention note, got: {err}"
    );

    assert!(
        shell.calls().is_empty(),
        "no Shell call must be issued for Clip / Note, got {:?}",
        shell.calls()
    );
}

#[test]
fn reveal_capture_with_rejects_invalid_ulid() {
    let (_dir, store) = temp_store();
    let shell = FakeShell::new();

    let err = reveal_capture_with(&shell, &store, "not-a-ulid")
        .expect_err("invalid id must error");
    assert!(
        err.to_lowercase().contains("invalid id"),
        "expected error to mention invalid id, got: {err}"
    );
    assert!(shell.calls().is_empty(), "no Shell call must be issued");
}
