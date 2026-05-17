//! Tauri command surface.
//!
//! Per ADR-0004 no SQL, clipboard, or filesystem code lives here. Each
//! command is a thin shim that composes `Store`, `clipboard`, and
//! `kind_detect` and translates errors into a string surface suitable
//! for `invoke()`. The real logic is in the helper functions below so
//! tests can drive them without a Tauri runtime.

use std::path::{Path, PathBuf};
use std::str::FromStr;

use tauri::menu::MenuBuilder;
use tauri::{AppHandle, Emitter, LogicalPosition, Manager, State};
use ulid::Ulid;

use crate::clipboard::{Clipboard, SystemClipboard};
use crate::dock::default_context_menu;
use crate::drag_drop::decide_dropped_files;
use crate::kind_detect::decide;
use crate::shell::{Shell, SystemShell};
use crate::store::{Capture, CaptureInput, CaptureKind, Store, SETTING_LAST_INBOX_OPEN_ID, ULID_MIN};

/// Event emitted on every successful Capture mutation (save, star,
/// soft-delete). Inbox / Dock JS subscribe to this to keep their list
/// in sync.
///
/// Payload shape (option A from the v1.0 issue 03):
/// - On `save`: the full new `Capture` (slice 02 contract, unchanged).
/// - On `star_capture` / `delete_capture`: a thin `MutationNotice`
///   with `{ id, kind: "starred" | "deleted" }` so subscribers can
///   decide whether to refetch (mutation) or prepend (new row).
pub const CAPTURES_CHANGED_EVENT: &str = "captures:changed";

/// Event emitted alongside `captures.changed` on every successful save
/// (Note, clipboard, dropped files). The Dock subscribes to this to
/// trigger its one-shot pulse animation. Kept distinct from
/// `captures.changed` so the badge increment (driven by `captures.changed`)
/// and the pulse animation (driven by this event) can be reasoned about
/// independently — e.g. star / soft-delete must NOT pulse but DO emit
/// `captures.changed`.
pub const DOCK_PULSE_EVENT: &str = "dock:pulse";

/// Event emitted when the Inbox window is shown (shortcut, tray menu,
/// Dock context-menu, or any future entry point). The Dock JS sets its
/// badge state to 0 on receipt. Rust also persists the new
/// `last_inbox_open_id` so the cleared state survives a restart.
pub const DOCK_BADGE_CLEARED_EVENT: &str = "dock:badge:cleared";

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
    let _ = app.emit(DOCK_PULSE_EVENT, ());
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
        let _ = app.emit(DOCK_PULSE_EVENT, ());
    }
    Ok(captures)
}

/// Persist one Capture per dropped file. Composes
/// `drag_drop::decide_dropped_files` + `store::save`. Tests drive this
/// helper directly; the Tauri command below is the thin wrapper the
/// Dock's native drag-drop handler calls.
///
/// Returns a `Vec` because a single drop gesture can carry N paths
/// (multi-select in Finder), one Capture per path.
pub fn save_dropped_files_with_store(
    store: &Store,
    paths: Vec<PathBuf>,
) -> Result<Vec<Capture>, String> {
    let inputs = decide_dropped_files(paths).map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(inputs.len());
    for input in inputs {
        out.push(store.save(input).map_err(|e| e.to_string())?);
    }
    Ok(out)
}

#[tauri::command]
pub fn save_dropped_files(
    paths: Vec<String>,
    app: AppHandle,
    store: State<'_, Store>,
) -> Result<Vec<Capture>, String> {
    let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    let captures = save_dropped_files_with_store(&store, paths)?;
    for capture in &captures {
        let _ = app.emit(CAPTURES_CHANGED_EVENT, capture);
        let _ = app.emit(DOCK_PULSE_EVENT, ());
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

/// Count of non-deleted captures created since the user last opened the
/// Inbox. Reads `last_inbox_open_id` from `app_settings` (default: ULID
/// min when never written) and runs the count against the live store —
/// per PRD note #133 the badge is computed on demand, never cached.
pub fn unread_count_with_store(store: &Store) -> Result<u64, String> {
    let cursor = store
        .settings_get(SETTING_LAST_INBOX_OPEN_ID)
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| ULID_MIN.to_string());
    store.count_after(&cursor).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn unread_count(store: State<'_, Store>) -> Result<u64, String> {
    unread_count_with_store(&store)
}

/// Mark the Inbox as having just been opened: compute the count being
/// cleared, then advance `last_inbox_open_id` to the id of the newest
/// existing capture. Empty store is a no-op (returns 0, leaves the
/// setting unwritten) so re-opening an empty Inbox does not poison the
/// cursor with a value the next save could not exceed.
pub fn mark_inbox_opened_with_store(store: &Store) -> Result<u64, String> {
    let cleared = unread_count_with_store(store)?;
    let newest = store.list_before(None, 1).map_err(|e| e.to_string())?;
    if let Some(latest) = newest.first() {
        store
            .settings_set(SETTING_LAST_INBOX_OPEN_ID, &latest.id)
            .map_err(|e| e.to_string())?;
    }
    Ok(cleared)
}

#[tauri::command]
pub fn mark_inbox_opened(store: State<'_, Store>) -> Result<u64, String> {
    mark_inbox_opened_with_store(&store)
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

/// Open a URL in the user's default browser. Pure pass-through to
/// `Shell::open_in_browser` — the Inbox detail pane calls this with
/// the `Link` Capture's `url` field directly so we do not pay a store
/// round-trip for a payload the JS already has in hand.
pub fn open_link_with(shell: &dyn Shell, url: &str) -> Result<(), String> {
    shell.open_in_browser(url).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_link(url: String) -> Result<(), String> {
    open_link_with(&SystemShell::new(), &url)
}

/// Route a Capture's id to the right Shell action, picking the right
/// path field per kind. Used by the Inbox detail pane for `File`,
/// path-flavor `Shot`, and bytes-flavor `Shot`. `Clip` and `Note` have
/// no reveal target and are rejected here as a programming bug — the
/// Inbox JS must not call this for those kinds. `Link` is routed to
/// `open_in_browser` as a defensive fallback, though the Inbox JS
/// prefers `open_link` directly so it can pass the URL it already has.
///
/// Uses `Store::find_with_deleted` so a soft-deleted Capture the user
/// still has on screen can still be revealed.
pub fn reveal_capture_with(
    shell: &dyn Shell,
    store: &Store,
    id: &str,
) -> Result<(), String> {
    let parsed = Ulid::from_str(id).map_err(|e| format!("invalid id: {e}"))?;
    let capture = store
        .find_with_deleted(&parsed)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("capture not found: {id}"))?;

    match capture.kind {
        CaptureKind::Link => {
            let url = capture
                .payload
                .get("url")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Link capture missing url".to_string())?;
            shell.open_in_browser(url).map_err(|e| e.to_string())
        }
        CaptureKind::File => {
            let path = capture
                .payload
                .get("source_path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "File capture missing source_path".to_string())?;
            shell
                .reveal_in_finder(Path::new(path))
                .map_err(|e| e.to_string())
        }
        CaptureKind::Shot => {
            // Path-flavor Shot has `source_path`; bytes-flavor has
            // `blob_path`. Reveal the on-disk file the user dropped
            // (source_path) but open the persisted blob in the default
            // image viewer (blob_path) — the user never put the blob
            // on disk themselves, so "reveal in Finder" is the wrong
            // intent.
            if let Some(p) = capture.payload.get("source_path").and_then(|v| v.as_str()) {
                shell
                    .reveal_in_finder(Path::new(p))
                    .map_err(|e| e.to_string())
            } else if let Some(p) = capture.payload.get("blob_path").and_then(|v| v.as_str()) {
                shell.open_path(Path::new(p)).map_err(|e| e.to_string())
            } else {
                Err("Shot capture missing both source_path and blob_path".to_string())
            }
        }
        CaptureKind::Clip | CaptureKind::Note => {
            Err(format!("cannot reveal {:?} capture", capture.kind))
        }
    }
}

#[tauri::command]
pub fn reveal_capture(id: String, store: State<'_, Store>) -> Result<(), String> {
    reveal_capture_with(&SystemShell::new(), &store, &id)
}
