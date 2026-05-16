//! Integration test for the dev_list binary. Seeds a temp captures.db
//! with 3 Note Captures and runs the compiled binary against it via the
//! `--db <path>` flag, asserting the output shape and `--limit` behavior.
//!
//! The `--db` flag exists for hermetic testing: without it the binary
//! would only read the macOS app-data path, which the test can't safely
//! point elsewhere. See the slice file for the scope note.

use std::process::Command;

use quick_capture_lib::store::{CaptureInput, Store};
use tempfile::TempDir;

fn seed_three_notes(dir: &TempDir) -> std::path::PathBuf {
    let path = dir.path().join("captures.db");
    let store = Store::open(&path).expect("open store");
    for text in ["one", "two", "three"] {
        store
            .save(CaptureInput::Note {
                text: text.to_string(),
            })
            .expect("save note");
        // ULIDs include a time component; sleeping briefly guarantees
        // the lexicographic id order matches insertion order even when
        // the clock has millisecond resolution.
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    path
}

fn run_dev_list(db_path: &std::path::Path, extra: &[&str]) -> (String, String, bool) {
    let exe = env!("CARGO_BIN_EXE_dev_list");
    let mut cmd = Command::new(exe);
    cmd.arg("--db").arg(db_path);
    for a in extra {
        cmd.arg(a);
    }
    let out = cmd.output().expect("spawn dev_list");
    (
        String::from_utf8(out.stdout).expect("stdout utf8"),
        String::from_utf8(out.stderr).expect("stderr utf8"),
        out.status.success(),
    )
}

fn split_columns(line: &str) -> Vec<&str> {
    // Columns are separated by two-or-more spaces. The payload preview
    // itself uses single spaces, so this split is unambiguous.
    line.split("  ").filter(|s| !s.is_empty()).collect()
}

#[test]
fn lists_seeded_notes_newest_first() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = seed_three_notes(&dir);

    let (stdout, stderr, ok) = run_dev_list(&db_path, &[]);
    assert!(ok, "dev_list failed; stderr={stderr}");

    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines.len(),
        3,
        "expected 3 output lines, got {}: {stdout:?}",
        lines.len()
    );

    // Newest first: last-inserted "three" should be on line 0.
    assert!(
        lines[0].contains("three"),
        "expected newest 'three' first, got: {}",
        lines[0]
    );
    assert!(
        lines[2].contains("one"),
        "expected oldest 'one' last, got: {}",
        lines[2]
    );

    for line in &lines {
        let cols = split_columns(line);
        assert!(
            cols.len() >= 4,
            "expected >=4 columns in line {line:?}, got {cols:?}"
        );
        // First column: short ULID (8 chars).
        assert_eq!(
            cols[0].len(),
            8,
            "expected short-ulid (8 chars) in first column of line {line:?}"
        );
        // Second column: kind label.
        assert_eq!(cols[1], "Note", "expected Note kind in line {line:?}");
    }
}

#[test]
fn limit_flag_truncates_output() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = seed_three_notes(&dir);

    let (stdout, stderr, ok) = run_dev_list(&db_path, &["--limit", "1"]);
    assert!(ok, "dev_list --limit 1 failed; stderr={stderr}");

    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines.len(),
        1,
        "expected exactly 1 line with --limit 1, got {}: {stdout:?}",
        lines.len()
    );
    assert!(
        lines[0].contains("three"),
        "expected newest 'three' with --limit 1, got: {}",
        lines[0]
    );
}
