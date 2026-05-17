//! Pure kind detection.
//!
//! `decide` turns a `ClipboardSnapshot` into a `CaptureInput`. Per
//! ADR-0004 it has no I/O; the only thing it does is pattern-match the
//! snapshot and run a tiny URL-prefix check on the text variant.
//!
//! Slice 04 handles only the text branches. The `Image` and `Files`
//! variants of `ClipboardSnapshot` are unimplemented stubs that return
//! `UnsupportedFormat`; slice 05 fills them in.

use crate::clipboard::ClipboardSnapshot;
use crate::store::CaptureInput;

#[derive(Debug)]
pub enum KindDetectError {
    /// Text snapshot is empty after trim.
    EmptyText,
    /// Snapshot variant not handled yet (image/files in slice 04).
    UnsupportedFormat,
}

impl std::fmt::Display for KindDetectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KindDetectError::EmptyText => write!(f, "clipboard text is empty"),
            KindDetectError::UnsupportedFormat => {
                write!(f, "clipboard snapshot variant is not supported yet")
            }
        }
    }
}

impl std::error::Error for KindDetectError {}

/// Decide which `CaptureInput` a `ClipboardSnapshot` becomes.
///
/// Pure: no clock, no I/O, no allocation that depends on anything but
/// the input.
pub fn decide(snapshot: ClipboardSnapshot) -> Result<CaptureInput, KindDetectError> {
    match snapshot {
        ClipboardSnapshot::Text(raw) => decide_text(raw),
        ClipboardSnapshot::Image { .. } | ClipboardSnapshot::Files(_) => {
            Err(KindDetectError::UnsupportedFormat)
        }
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
    if lower.starts_with("http://")
        || lower.starts_with("https://")
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
        return Some(format!("https://{trimmed}"));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(s: &str) -> ClipboardSnapshot {
        ClipboardSnapshot::Text(s.to_string())
    }

    #[test]
    fn https_text_becomes_link() {
        let input = decide(snap("https://example.com")).expect("decide");
        match input {
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
        let input = decide(snap("http://example.com")).expect("decide");
        match input {
            CaptureInput::Link { url, .. } => assert_eq!(url, "http://example.com"),
            other => panic!("expected Link, got {other:?}"),
        }
    }

    #[test]
    fn www_text_is_promoted_to_https_link() {
        let input = decide(snap("www.example.com")).expect("decide");
        match input {
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
        let input = decide(snap("mailto:a@b.com")).expect("decide");
        match input {
            CaptureInput::Link { url, .. } => assert_eq!(url, "mailto:a@b.com"),
            other => panic!("expected Link, got {other:?}"),
        }
    }

    #[test]
    fn plain_text_becomes_clip() {
        let input = decide(snap("not a url, just a thought")).expect("decide");
        match input {
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
        let input = decide(snap("https://example.com is great")).expect("decide");
        assert!(matches!(input, CaptureInput::Clip { .. }));
    }

    #[test]
    fn bare_scheme_is_not_a_link() {
        let input = decide(snap("https://")).expect("decide");
        assert!(matches!(input, CaptureInput::Clip { .. }));
    }

    #[test]
    fn image_snapshot_is_unsupported_in_slice_04() {
        let err = decide(ClipboardSnapshot::Image {
            bytes: vec![1, 2, 3],
            mime: "image/png".into(),
        })
        .expect_err("image must error");
        assert!(matches!(err, KindDetectError::UnsupportedFormat));
    }

    #[test]
    fn files_snapshot_is_unsupported_in_slice_04() {
        let err = decide(ClipboardSnapshot::Files(vec!["/tmp/a".into()]))
            .expect_err("files must error");
        assert!(matches!(err, KindDetectError::UnsupportedFormat));
    }
}
