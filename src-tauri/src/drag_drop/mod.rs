//! Pure decision function for Finder file drops onto the Dock.
//!
//! Mirrors the `kind_detect::decide` shape: takes the paths Tauri's
//! native drag-drop handler delivered and returns one `CaptureInput`
//! per path. Per ADR-0004 this module performs no I/O — it only looks
//! up mime types via `mime_guess` (the same static table `kind_detect`
//! uses for the clipboard's `Files` snapshot) and splits image mimes
//! into `Shot { Path }` and the rest into `File`.
//!
//! Per the revised ADR-0008 the Dock only handles file drops in v1.0;
//! URL / text / image-bytes drags are deferred until Tauri exposes a
//! custom drag-drop handler. `DropError::Empty` is the only error this
//! surface can produce.
//!
//! The mime split here is the same canonical rule used by
//! `clipboard::ClipboardSnapshot::Files` in `kind_detect`: image mime ->
//! `Shot { Path { source_path, mime } }`, non-image -> `File { source_path,
//! mime, original_name }`. Keeping the split logic in a separate function
//! rather than reusing `kind_detect::decide_files` directly keeps each
//! module's error surface scoped to its own input shape.

use std::path::{Path, PathBuf};

use crate::store::{CaptureInput, ShotSource};

#[derive(Debug)]
pub enum DropError {
    /// The drag-drop handler delivered an empty path list. macOS' native
    /// drag-drop will only fire `Drop` when at least one path is present,
    /// but we still defend against it so the public surface has a single,
    /// total contract.
    Empty,
}

impl std::fmt::Display for DropError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DropError::Empty => write!(f, "dropped path list is empty"),
        }
    }
}

impl std::error::Error for DropError {}

/// Decide which `CaptureInput`s a list of dropped paths becomes.
///
/// Pure: no I/O, no clock; only depends on the input paths and the
/// static `mime_guess` table.
pub fn decide_dropped_files(paths: Vec<PathBuf>) -> Result<Vec<CaptureInput>, DropError> {
    if paths.is_empty() {
        return Err(DropError::Empty);
    }
    Ok(paths.into_iter().map(decide_one_path).collect())
}

fn decide_one_path(path: PathBuf) -> CaptureInput {
    let mime = guess_mime(&path);
    if mime.starts_with("image/") {
        CaptureInput::Shot {
            source: ShotSource::Path {
                source_path: path,
                mime,
            },
            width: None,
            height: None,
        }
    } else {
        let original_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned());
        CaptureInput::File {
            source_path: path,
            mime,
            original_name,
        }
    }
}

fn guess_mime(path: &Path) -> String {
    mime_guess::from_path(path)
        .first()
        .map(|m| m.essence_str().to_string())
        .unwrap_or_else(|| "application/octet-stream".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decide_dropped_files_image_mime_path_becomes_shot() {
        let mut out = decide_dropped_files(vec![PathBuf::from("/tmp/screenshot.png")])
            .expect("decide");
        assert_eq!(out.len(), 1);
        match out.pop().unwrap() {
            CaptureInput::Shot { source, width, height } => {
                assert!(width.is_none());
                assert!(height.is_none());
                match source {
                    ShotSource::Path { source_path, mime } => {
                        assert_eq!(source_path, PathBuf::from("/tmp/screenshot.png"));
                        assert_eq!(mime, "image/png");
                    }
                    ShotSource::Bytes { .. } => panic!("expected Path shot, got Bytes"),
                }
            }
            other => panic!("expected Shot, got {other:?}"),
        }
    }

    #[test]
    fn decide_dropped_files_non_image_mime_path_becomes_file() {
        let mut out = decide_dropped_files(vec![PathBuf::from("/tmp/notes.pdf")])
            .expect("decide");
        assert_eq!(out.len(), 1);
        match out.pop().unwrap() {
            CaptureInput::File {
                source_path,
                mime,
                original_name,
            } => {
                assert_eq!(source_path, PathBuf::from("/tmp/notes.pdf"));
                assert_eq!(mime, "application/pdf");
                assert_eq!(original_name.as_deref(), Some("notes.pdf"));
            }
            other => panic!("expected File, got {other:?}"),
        }
    }

    #[test]
    fn decide_dropped_files_mixed_list_preserves_order() {
        let outs = decide_dropped_files(vec![
            PathBuf::from("/tmp/a.png"),
            PathBuf::from("/tmp/b.pdf"),
            PathBuf::from("/tmp/c.jpg"),
        ])
        .expect("decide");
        assert_eq!(outs.len(), 3);
        assert!(
            matches!(&outs[0], CaptureInput::Shot { .. }),
            "first must be Shot, got {:?}",
            outs[0]
        );
        assert!(
            matches!(&outs[1], CaptureInput::File { .. }),
            "second must be File, got {:?}",
            outs[1]
        );
        assert!(
            matches!(&outs[2], CaptureInput::Shot { .. }),
            "third must be Shot, got {:?}",
            outs[2]
        );
    }

    #[test]
    fn decide_dropped_files_empty_vec_errors() {
        let err = decide_dropped_files(vec![]).expect_err("empty must error");
        assert!(matches!(err, DropError::Empty));
    }
}
