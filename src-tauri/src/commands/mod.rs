//! Tauri command surface.
//!
//! Per ADR-0004 no SQL or filesystem code lives here. Each command is a
//! thin shim that composes `Store` (and, in later slices, `clipboard` +
//! `kind_detect`) and translates store errors into a string surface
//! suitable for `invoke()`. The real logic is in the helper functions
//! below so tests can drive them without a Tauri runtime.

use tauri::State;

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
