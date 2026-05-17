//! Tauri command surface.
//!
//! Per ADR-0004 no SQL, clipboard, or filesystem code lives here. Each
//! command is a thin shim that composes `Store`, `clipboard`, and
//! `kind_detect` and translates errors into a string surface suitable
//! for `invoke()`. The real logic is in the helper functions below so
//! tests can drive them without a Tauri runtime.

use std::str::FromStr;

use tauri::menu::MenuBuilder;
use tauri::{AppHandle, Emitter, LogicalPosition, Manager, State};
use ulid::Ulid;

use crate::clipboard::{Clipboard, SystemClipboard};
use crate::dock::default_context_menu;
use crate::kind_detect::decide;
use crate::store::{Capture, CaptureInput, Store};

/// Event emitted on every successful Capture mutation (save, star,
/// soft-delete). Inbox / Dock JS subscribe to this to keep their list
/// in sync.
///
/// Payload shape (option A from the v1.0 issue 03):
/// - On `save`: the full new `Capture` (slice 02 contract, unchanged).
/// - On `star_capture` / `delete_capture`: a thin `MutationNotice`
///   with `{ id, kind: "starred" | "deleted" }` so subscribers can
///   decide whether to refetch (mutation) or prepend (new row).
pub const CAPTURES_CHANGED_EVENT: &str = "captures.changed";

/// Thin payload emitted with `captures.changed` on star / soft-delete.
/// Slice 02 emits a full `Capture` on save; slice 03 emits this shape
/// on mutations so the Inbox can tell "refetch page" from "prepend".
#[derive(Debug, Clone, serde::Serialize)]
pub struct MutationNotice<'a> {
    pub id: &'a str,
    pub kind: &'static str,
}

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
pub fn save_note(
    text: String,
    app: AppHandle,
    store: State<'_, Store>,
) -> Result<Capture, String> {
    let capture = save_note_with_store(&store, &text)?;
    let _ = app.emit(CAPTURES_CHANGED_EVENT, &capture);
    Ok(capture)
}

/// Read the current clipboard, detect kind(s), persist one or more
/// Captures. Composes `clipboard` -> `kind_detect` -> `store`. Empty
/// clipboard or an empty file list returns an error and writes no row.
///
/// Returns a `Vec` because one clipboard read can produce N rows: a
/// multi-file Finder copy expands to one Capture per file. Single-payload
/// reads (text, single image) return a one-element vec.
///
/// The clipboard adapter is injected so integration tests can feed
/// arbitrary snapshots; the Tauri command below uses the real
/// `SystemClipboard`.
pub fn capture_clipboard_now_with(
    clipboard: &dyn Clipboard,
    store: &Store,
) -> Result<Vec<Capture>, String> {
    let snapshot = clipboard.read().map_err(|e| e.to_string())?;
    let inputs = decide(snapshot).map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(inputs.len());
    for input in inputs {
        out.push(store.save(input).map_err(|e| e.to_string())?);
    }
    Ok(out)
}

#[tauri::command]
pub fn capture_clipboard_now(
    app: AppHandle,
    store: State<'_, Store>,
) -> Result<Vec<Capture>, String> {
    let captures = capture_clipboard_now_with(&SystemClipboard::new(), &store)?;
    for capture in &captures {
        let _ = app.emit(CAPTURES_CHANGED_EVENT, capture);
    }
    Ok(captures)
}

/// List captures with cursor pagination. `cursor` is the ULID string of
/// the last item from the previous page; pass `None` for the first page.
/// Parses the cursor and delegates to `Store::list_before` so tests can
/// exercise the parse + forward path without a Tauri runtime.
pub fn list_captures_with_store(
    store: &Store,
    cursor: Option<&str>,
    limit: u32,
) -> Result<Vec<Capture>, String> {
    let parsed = match cursor {
        Some(s) => Some(Ulid::from_str(s).map_err(|e| format!("invalid cursor: {e}"))?),
        None => None,
    };
    store.list_before(parsed, limit).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_captures(
    cursor: Option<String>,
    limit: u32,
    store: State<'_, Store>,
) -> Result<Vec<Capture>, String> {
    list_captures_with_store(&store, cursor.as_deref(), limit)
}

/// Toggle the `starred` flag on a capture. Parses `id` as a ULID and
/// delegates to `Store::set_star`. The Tauri command wrapper emits
/// `captures.changed` so subscribers refetch.
pub fn star_capture_with_store(
    store: &Store,
    id: &str,
    starred: bool,
) -> Result<(), String> {
    let parsed = Ulid::from_str(id).map_err(|e| format!("invalid id: {e}"))?;
    store.set_star(&parsed, starred).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn star_capture(
    id: String,
    starred: bool,
    app: AppHandle,
    store: State<'_, Store>,
) -> Result<(), String> {
    star_capture_with_store(&store, &id, starred)?;
    let _ = app.emit(
        CAPTURES_CHANGED_EVENT,
        MutationNotice {
            id: &id,
            kind: "starred",
        },
    );
    Ok(())
}

/// Soft-delete a capture. Parses `id` as a ULID and delegates to
/// `Store::soft_delete`. The Tauri command wrapper emits
/// `captures.changed` so subscribers refetch.
pub fn delete_capture_with_store(store: &Store, id: &str) -> Result<(), String> {
    let parsed = Ulid::from_str(id).map_err(|e| format!("invalid id: {e}"))?;
    store.soft_delete(&parsed).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_capture(
    id: String,
    app: AppHandle,
    store: State<'_, Store>,
) -> Result<(), String> {
    delete_capture_with_store(&store, &id)?;
    let _ = app.emit(
        CAPTURES_CHANGED_EVENT,
        MutationNotice {
            id: &id,
            kind: "deleted",
        },
    );
    Ok(())
}

/// Show + focus the Composer (main) window. The Dock JS calls this on
/// click; the same path is exercised by the `Ctrl+Alt+Cmd+Space`
/// shortcut and the Tray's "Open Composer" item, so all three entry
/// points behave identically.
///
/// macOS requires `show()` / `set_focus()` to run on the main thread
/// for the app to actually activate and grab keyboard focus. The
/// command runs on a Tauri worker thread, so we hop to main via
/// `run_on_main_thread`.
#[tauri::command]
pub fn open_composer_window(app: AppHandle) -> Result<(), String> {
    let app_handle = app.clone();
    app.run_on_main_thread(move || {
        if let Some(window) = app_handle.get_webview_window("main") {
            let _ = window.show();
            let _ = window.set_focus();
        }
        let _ = app_handle.emit("open_composer", ());
    })
    .map_err(|e| e.to_string())
}

/// Open the Dock's right-click context menu at the given Dock-window
/// coordinates. The menu shape mirrors the Tray (Open Composer, Open
/// Inbox, Quit) via `dock::default_context_menu()` — same labels and
/// event names as `tray::default_menu()`, only the `menu_id` differs
/// so the app-level `on_menu_event` dispatcher (registered in
/// `lib::run` setup) can route Dock-popup clicks separately.
///
/// `x` and `y` are in the Dock window's logical coordinate space (i.e.
/// the `event.clientX` / `event.clientY` from the contextmenu event).
#[tauri::command]
pub fn open_dock_context_menu(app: AppHandle, x: f64, y: f64) -> Result<(), String> {
    let bindings = default_context_menu();
    let mut menu = MenuBuilder::new(&app);
    for b in &bindings {
        menu = menu.text(b.menu_id, b.tray.label);
    }
    let menu = menu
        .build()
        .map_err(|e| format!("build dock context menu: {e}"))?;

    let dock = app
        .get_webview_window("dock")
        .ok_or_else(|| "dock window not found".to_string())?;
    dock.popup_menu_at(&menu, LogicalPosition::new(x, y))
        .map_err(|e| format!("popup dock context menu: {e}"))
}
