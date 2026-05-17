//! Clipboard adapter.
//!
//! Per ADR-0004 the only place in the codebase that talks to the OS
//! clipboard is this module. Per ADR-0005 the trait-plus-fake split is
//! the load-bearing testability lever: `commands::capture_clipboard_now`
//! composes against `&dyn Clipboard` so tests can feed arbitrary
//! snapshots without a real keyboard or pasteboard.
//!
//! Slice 04 only exercises the `Text` variant. The `Image` and `Files`
//! variants are declared so slice 05 can fill them in without changing
//! the public shape; the real backend returns `UnsupportedFormat` for
//! anything that is not text.

use std::path::PathBuf;

/// What `Clipboard::read` returns. A closed set so `kind_detect` can
/// match exhaustively. Slice 04 only constructs `Text`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardSnapshot {
    Text(String),
    Image { bytes: Vec<u8>, mime: String },
    Files(Vec<PathBuf>),
}

#[derive(Debug)]
pub enum ClipboardError {
    /// The clipboard is empty or contains a format we did not request.
    Empty,
    /// The clipboard holds something we cannot read yet (e.g. image or
    /// file references in slice 04). Slice 05 will replace this for the
    /// image and files paths.
    UnsupportedFormat,
    /// The underlying OS-level read failed.
    Backend(String),
}

impl std::fmt::Display for ClipboardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClipboardError::Empty => write!(f, "clipboard is empty"),
            ClipboardError::UnsupportedFormat => {
                write!(f, "clipboard holds an unsupported format")
            }
            ClipboardError::Backend(msg) => write!(f, "clipboard backend error: {msg}"),
        }
    }
}

impl std::error::Error for ClipboardError {}

/// Adapter trait. One method so tests stay trivial.
pub trait Clipboard {
    fn read(&self) -> Result<ClipboardSnapshot, ClipboardError>;
}

/// Real backend, backed by `arboard`. Image and file reads return
/// `UnsupportedFormat` until slice 05 wires them up.
pub struct SystemClipboard;

impl SystemClipboard {
    pub fn new() -> Self {
        SystemClipboard
    }
}

impl Default for SystemClipboard {
    fn default() -> Self {
        Self::new()
    }
}

impl Clipboard for SystemClipboard {
    fn read(&self) -> Result<ClipboardSnapshot, ClipboardError> {
        let mut cb = arboard::Clipboard::new()
            .map_err(|e| ClipboardError::Backend(e.to_string()))?;
        match cb.get_text() {
            Ok(text) => Ok(ClipboardSnapshot::Text(text)),
            Err(arboard::Error::ContentNotAvailable) => Err(ClipboardError::Empty),
            Err(arboard::Error::ClipboardNotSupported) => {
                Err(ClipboardError::Backend("clipboard not supported".into()))
            }
            // Anything else (image-only contents, etc.) is "we can't read this
            // as text". Slice 05 will branch on image/file formats here.
            Err(_) => Err(ClipboardError::UnsupportedFormat),
        }
    }
}

/// Test-support fake. Lives at module scope (not `#[cfg(test)]`) so
/// integration tests under `src-tauri/tests/` can reach it; integration
/// tests link against the public library and cannot see `#[cfg(test)]`
/// items. The fake is harmless in release builds — it is a plain struct
/// with no OS access.
pub struct FakeClipboard {
    result: std::sync::Mutex<Option<Result<ClipboardSnapshot, ClipboardError>>>,
}

impl FakeClipboard {
    pub fn with(snapshot: Result<ClipboardSnapshot, ClipboardError>) -> Self {
        FakeClipboard {
            result: std::sync::Mutex::new(Some(snapshot)),
        }
    }
}

impl Clipboard for FakeClipboard {
    fn read(&self) -> Result<ClipboardSnapshot, ClipboardError> {
        self.result
            .lock()
            .expect("fake clipboard mutex poisoned")
            .take()
            .expect("FakeClipboard::read called more than once")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_clipboard_surfaces_text_snapshot() {
        let fake = FakeClipboard::with(Ok(ClipboardSnapshot::Text("hello".into())));
        let got = fake.read().expect("fake should return Ok");
        assert_eq!(got, ClipboardSnapshot::Text("hello".into()));
    }

    #[test]
    fn fake_clipboard_surfaces_error() {
        let fake = FakeClipboard::with(Err(ClipboardError::Empty));
        let err = fake.read().expect_err("fake should return Err");
        assert!(matches!(err, ClipboardError::Empty));
    }
}
