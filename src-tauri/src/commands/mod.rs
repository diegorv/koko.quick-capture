//! Tauri command surface.
//!
//! Per ADR-0004 no SQL, clipboard, or filesystem code lives here. Each
//! command is a thin shim that composes `Store`, `clipboard`, and
//! `kind_detect` and translates errors into a string surface suitable
//! for `invoke()`. The real logic is in the helper functions below so
//! tests can drive them without a Tauri runtime.

use tauri::State;

use crate::clipboard::{Clipboard, SystemClipboard};
use crate::kind_detect::decide;
use crate::store::{Capture, CaptureInput, Store};

/// Save a free-text Note. Empty / whitespace-only input is rejected.
pub fn save_note_with_store(store: &Store, text: &str) -> Result<Capture, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("note text is empty".to_string());
    }
    store
        .save(CaptureInput::Note {
            text: text.to_string(),
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_note(text: String, store: State<'_, Store>) -> Result<Capture, String> {
    save_note_with_store(&store, &text)
}

/// Read the current clipboard, detect kind, persist a Capture. Composes
/// `clipboard` -> `kind_detect` -> `store`. Empty clipboard or an
/// unsupported snapshot variant returns an error and writes no row.
///
/// The clipboard adapter is injected so integration tests can feed
/// arbitrary snapshots; the Tauri command below uses the real
/// `SystemClipboard`.
pub fn capture_clipboard_now_with(
    clipboard: &dyn Clipboard,
    store: &Store,
) -> Result<Capture, String> {
    let snapshot = clipboard.read().map_err(|e| e.to_string())?;
    let input = decide(snapshot).map_err(|e| e.to_string())?;
    store.save(input).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn capture_clipboard_now(store: State<'_, Store>) -> Result<Capture, String> {
    capture_clipboard_now_with(&SystemClipboard::new(), &store)
}
