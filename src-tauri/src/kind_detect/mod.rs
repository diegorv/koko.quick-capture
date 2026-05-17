//! Pure kind detection.
//!
//! `decide` turns a `ClipboardSnapshot` into one or more `CaptureInput`s.
//! Per ADR-0004 it has no I/O; the only thing it does is pattern-match
//! the snapshot, run a URL-prefix check on text, and look up mime types
//! for file paths.
//!
//! Slice 05 expanded the return type from a single `CaptureInput` to a
//! `Vec`. The `Files` variant of `ClipboardSnapshot` produces N
//! captures (one per copied file), so the vec is the common shape; the
//! single-input branches just return a one-element vec.

use std::path::Path;

use crate::clipboard::ClipboardSnapshot;
use crate::store::{CaptureInput, ShotSource};

#[derive(Debug)]
pub enum KindDetectError {
    /// Text snapshot is empty after trim.
    EmptyText,
    /// Files snapshot carries an empty list.
    EmptyFiles,
}

impl std::fmt::Display for KindDetectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KindDetectError::EmptyText => write!(f, "clipboard text is empty"),
            KindDetectError::EmptyFiles => write!(f, "clipboard file list is empty"),
        }
    }
}

impl std::error::Error for KindDetectError {}

/// Decide which `CaptureInput`s a `ClipboardSnapshot` becomes.
///
/// Pure: no clock, no I/O, no allocation that depends on anything but
/// the input (and the static mime-type table inside `mime_guess`).
pub fn decide(snapshot: ClipboardSnapshot) -> Result<Vec<CaptureInput>, KindDetectError> {
    match snapshot {
        ClipboardSnapshot::Text(raw) => decide_text(raw).map(|c| vec![c]),
        ClipboardSnapshot::Image { bytes, mime } => Ok(vec![CaptureInput::Shot {
            source: ShotSource::Bytes { bytes, mime },
            width: None,
            height: None,
        }]),
        ClipboardSnapshot::Files(paths) => decide_files(paths),
    }
}

fn decide_text(raw: String) -> Result<CaptureInput, KindDetectError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(KindDetectError::EmptyText);
    }

    if let Some(url) = url_from_trimmed(trimmed) {
        Ok(CaptureInput::Link {
            url,
            raw_text: raw,
            title: None,
        })
    } else {
        // Preserve verbatim: only the URL branch trims; a Clip keeps
        // whatever the user copied.
        Ok(CaptureInput::Clip { text: raw })
    }
}

fn decide_files(paths: Vec<std::path::PathBuf>) -> Result<Vec<CaptureInput>, KindDetectError> {
    if paths.is_empty() {
        return Err(KindDetectError::EmptyFiles);
    }
    Ok(paths.into_iter().map(decide_one_path).collect())
}

fn decide_one_path(path: std::path::PathBuf) -> CaptureInput {
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

/// URL detection. Hand-rolled against a fixed prefix set to avoid
/// pulling `regex` in as a direct dependency. Matches:
///
/// - `http://...`
/// - `https://...`
/// - `mailto:...`
/// - `www....` (promoted to `https://www....`)
///
/// Match is case-insensitive on the scheme/`www.` prefix; the rest of
/// the URL is preserved as-is. Returns `Some(url)` with the canonical
/// form to store in `payload.url`.
fn url_from_trimmed(trimmed: &str) -> Option<String> {
    // No URL has whitespace mid-string; if there is any, the user
    // copied a sentence that happens to start with something URL-ish.
    if trimmed.chars().any(char::is_whitespace) {
        return None;
    }

    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("http://") // privacy-ok: URL prefix detection on user input, not a network call
        || lower.starts_with("https://") // privacy-ok: URL prefix detection on user input, not a network call
        || lower.starts_with("mailto:")
    {
        // Require at least one character after the scheme delimiter so
        // "https://" alone is not treated as a URL.
        let scheme_end = lower.find(':').expect("scheme contains ':'");
        // For http(s) we need to also skip the "//".
        let body_start = if lower.starts_with("mailto:") {
            scheme_end + 1
        } else {
            scheme_end + 3
        };
        if trimmed.len() > body_start {
            return Some(trimmed.to_string());
        }
        return None;
    }
    if lower.starts_with("www.") && trimmed.len() > 4 {
        return Some(format!("https://{trimmed}")); // privacy-ok: prepends scheme to bare host, not a network call
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn snap(s: &str) -> ClipboardSnapshot {
        ClipboardSnapshot::Text(s.to_string())
    }

    fn one(snapshot: ClipboardSnapshot) -> CaptureInput {
        let mut out = decide(snapshot).expect("decide");
        assert_eq!(out.len(), 1, "expected one CaptureInput, got {out:?}");
        out.pop().unwrap()
    }

    #[test]
    fn https_text_becomes_link() {
        match one(snap("https://example.com")) {
            CaptureInput::Link {
                url,
                raw_text,
                title,
            } => {
                assert_eq!(url, "https://example.com");
                assert_eq!(raw_text, "https://example.com");
                assert!(title.is_none());
            }
            other => panic!("expected Link, got {other:?}"),
        }
    }

    #[test]
    fn http_text_becomes_link() {
        match one(snap("http://example.com")) {
            CaptureInput::Link { url, .. } => assert_eq!(url, "http://example.com"),
            other => panic!("expected Link, got {other:?}"),
        }
    }

    #[test]
    fn www_text_is_promoted_to_https_link() {
        match one(snap("www.example.com")) {
            CaptureInput::Link {
                url, raw_text, ..
            } => {
                assert_eq!(url, "https://www.example.com");
                assert_eq!(
                    raw_text, "www.example.com",
                    "raw_text must keep the original copy"
                );
            }
            other => panic!("expected Link, got {other:?}"),
        }
    }

    #[test]
    fn mailto_text_becomes_link() {
        match one(snap("mailto:a@b.com")) {
            CaptureInput::Link { url, .. } => assert_eq!(url, "mailto:a@b.com"),
            other => panic!("expected Link, got {other:?}"),
        }
    }

    #[test]
    fn plain_text_becomes_clip() {
        match one(snap("not a url, just a thought")) {
            CaptureInput::Clip { text } => {
                assert_eq!(text, "not a url, just a thought");
            }
            other => panic!("expected Clip, got {other:?}"),
        }
    }

    #[test]
    fn empty_text_errors() {
        let err = decide(snap("")).expect_err("empty must error");
        assert!(matches!(err, KindDetectError::EmptyText));
    }

    #[test]
    fn whitespace_only_text_errors() {
        let err = decide(snap("   \n\t  ")).expect_err("whitespace must error");
        assert!(matches!(err, KindDetectError::EmptyText));
    }

    #[test]
    fn text_with_url_followed_by_words_is_not_a_link() {
        // "https://example.com is great" has internal whitespace; we
        // treat the whole thing as a Clip rather than guess where the
        // URL ends.
        let input = one(snap("https://example.com is great"));
        assert!(matches!(input, CaptureInput::Clip { .. }));
    }

    #[test]
    fn bare_scheme_is_not_a_link() {
        let input = one(snap("https://")); // privacy-ok: bare-scheme test fixture, not a network call
        assert!(matches!(input, CaptureInput::Clip { .. }));
    }

    #[test]
    fn image_snapshot_becomes_shot_bytes() {
        match one(ClipboardSnapshot::Image {
            bytes: vec![1, 2, 3, 4],
            mime: "image/png".into(),
        }) {
            CaptureInput::Shot { source, .. } => match source {
                ShotSource::Bytes { bytes, mime } => {
                    assert_eq!(bytes, vec![1, 2, 3, 4]);
                    assert_eq!(mime, "image/png");
                }
                ShotSource::Path { .. } => panic!("expected Bytes shot, got Path"),
            },
            other => panic!("expected Shot, got {other:?}"),
        }
    }

    #[test]
    fn image_extension_file_becomes_shot_path() {
        match one(ClipboardSnapshot::Files(vec![PathBuf::from(
            "/tmp/screenshot.png",
        )])) {
            CaptureInput::Shot { source, .. } => match source {
                ShotSource::Path { source_path, mime } => {
                    assert_eq!(source_path, PathBuf::from("/tmp/screenshot.png"));
                    assert_eq!(mime, "image/png");
                }
                ShotSource::Bytes { .. } => panic!("expected Path shot, got Bytes"),
            },
            other => panic!("expected Shot, got {other:?}"),
        }
    }

    #[test]
    fn non_image_extension_file_becomes_file() {
        match one(ClipboardSnapshot::Files(vec![PathBuf::from(
            "/tmp/notes.pdf",
        )])) {
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
    fn mixed_files_expand_in_order() {
        let outs = decide(ClipboardSnapshot::Files(vec![
            PathBuf::from("/tmp/a.png"),
            PathBuf::from("/tmp/b.pdf"),
            PathBuf::from("/tmp/c.jpg"),
        ]))
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
    fn empty_files_errors() {
        let err = decide(ClipboardSnapshot::Files(vec![]))
            .expect_err("empty files must error");
        assert!(matches!(err, KindDetectError::EmptyFiles));
    }

    #[test]
    fn unknown_extension_file_falls_back_to_octet_stream() {
        match one(ClipboardSnapshot::Files(vec![PathBuf::from(
            "/tmp/no-ext",
        )])) {
            CaptureInput::File { mime, .. } => {
                assert_eq!(mime, "application/octet-stream");
            }
            other => panic!("expected File, got {other:?}"),
        }
    }
}
