//! Tauri command surface.
//!
//! Per ADR-0004 no SQL, clipboard, or filesystem code lives here. Each
//! command is a thin shim that composes `Store`, `clipboard`, and
//! `kind_detect` and translates errors into a string surface suitable
//! for `invoke()`. The real logic is in the helper functions below so
//! tests can drive them without a Tauri runtime.

use std::path::{Path, PathBuf};
use std::str::FromStr;

use tauri::menu::{IconMenuItem, MenuBuilder};
use tauri::{AppHandle, Emitter, LogicalPosition, Manager, State};
use ulid::Ulid;

use crate::clipboard::{Clipboard, SystemClipboard};
use crate::dock::default_context_menu;
use crate::drag_drop::decide_dropped_files;
use crate::kind_detect::decide;
use crate::shell::{Shell, SystemShell};
use crate::store::{Capture, CaptureInput, CaptureKind, Store};

use crate::events::{
    CAPTURES_CHANGED as CAPTURES_CHANGED_EVENT, DOCK_PULSE as DOCK_PULSE_EVENT,
    DOCK_UNREAD_CHANGED as DOCK_UNREAD_CHANGED_EVENT, OPEN_COMPOSER,
};

/// Thin payload emitted with `captures:changed` on star / soft-delete.
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

/// Count of non-deleted captures the user has not yet interacted with
/// (i.e. rows with `read_at IS NULL`). Computed on demand against the
/// live store; never cached.
pub fn unread_count_with_store(store: &Store) -> Result<u64, String> {
    store.count_unread().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn unread_count(store: State<'_, Store>) -> Result<u64, String> {
    unread_count_with_store(&store)
}

/// Total non-deleted captures. Used by the Inbox status bar.
pub fn total_count_with_store(store: &Store) -> Result<u64, String> {
    store.count_all().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn total_count(store: State<'_, Store>) -> Result<u64, String> {
    total_count_with_store(&store)
}

/// Stamp `read_at` on a single capture. Returns the (recomputed) count
/// of remaining unread rows so the caller can update its UI without a
/// follow-up `unread_count` round-trip.
pub fn mark_read_with_store(store: &Store, id: &str) -> Result<u64, String> {
    let parsed = Ulid::from_string(id).map_err(|e| format!("invalid id: {e}"))?;
    store.mark_read(&parsed).map_err(|e| e.to_string())?;
    unread_count_with_store(store)
}

#[tauri::command]
pub fn mark_read(app: AppHandle, store: State<'_, Store>, id: String) -> Result<u64, String> {
    let remaining = mark_read_with_store(&store, &id)?;
    // Tell the Dock to re-render its badge from the live count rather
    // than a delta so the two stay in sync even if multiple flips race.
    let _ = app.emit(DOCK_UNREAD_CHANGED_EVENT, remaining);
    Ok(remaining)
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
    show_composer(&app);
    Ok(())
}

/// One place that knows how to bring the Composer to screen. Used by
/// the global shortcut, the tray menu, the Dock click invoke, and any
/// future entry point. Records the prior frontmost macOS app FIRST
/// so `dismiss_composer` can hand focus back, then hops to the main
/// thread for `show + set_focus` (macOS requires both on main).
pub fn show_composer(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        record_prev_frontmost();
        if let Some(window) = handle.get_webview_window("composer") {
            let _ = window.show();
            let _ = window.set_focus();
        }
        let _ = handle.emit(OPEN_COMPOSER, ());
    });
}

/// One place that knows how to bring the Inbox to screen. Flips the
/// macOS activation policy to Regular so Cmd+Tab surfaces the app
/// while the Inbox is on screen, then shows + focuses the window.
/// The close path (`hide_inbox` command + the window's CloseRequested
/// handler) reverts the policy.
pub fn show_inbox(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        crate::set_inbox_activation_policy(&handle, true);
        if let Some(window) = handle.get_webview_window("inbox") {
            let _ = window.show();
            let _ = window.set_focus();
        }
    });
}

/// Track the macOS PID that was frontmost just before quick-capture
/// summoned the Composer, so `dismiss_composer` can hand focus back
/// to that exact app. -1 means "no prior app recorded" (e.g. cold
/// start, or the Composer is being dismissed without a prior open).
#[cfg(target_os = "macos")]
static PREV_FRONTMOST_PID: std::sync::atomic::AtomicI32 =
    std::sync::atomic::AtomicI32::new(-1);

/// Snapshot the macOS frontmost app PID via NSWorkspace. Called from
/// every Composer-summon path (shortcut, tray menu item, Dock click
/// invoke) BEFORE we show the Composer, while the user's real prior
/// app is still frontmost. Our own PID is filtered out so a
/// re-summon while the Composer is already up does not record us as
/// the "prior" app.
#[cfg(target_os = "macos")]
pub fn record_prev_frontmost() {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    use std::sync::atomic::Ordering;
    unsafe {
        let workspace: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
        if workspace.is_null() {
            return;
        }
        let frontmost: *mut AnyObject = msg_send![workspace, frontmostApplication];
        if frontmost.is_null() {
            return;
        }
        let pid: i32 = msg_send![frontmost, processIdentifier];
        let our_pid = std::process::id() as i32;
        if pid > 0 && pid != our_pid {
            PREV_FRONTMOST_PID.store(pid, Ordering::SeqCst);
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn record_prev_frontmost() {}

/// Activate the app whose PID was last recorded by
/// `record_prev_frontmost`. Resets the slot to -1 on read so a
/// subsequent unrelated dismiss does not re-trigger.
#[cfg(target_os = "macos")]
fn activate_prev_app() {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    use std::sync::atomic::Ordering;
    let pid = PREV_FRONTMOST_PID.swap(-1, Ordering::SeqCst);
    if pid <= 0 {
        return;
    }
    unsafe {
        let cls: *mut AnyObject =
            msg_send![class!(NSRunningApplication), runningApplicationWithProcessIdentifier: pid];
        if cls.is_null() {
            return;
        }
        // 0 = no special options; macOS still brings the target app
        // to the foreground. The deprecated activateWithOptions: path
        // is fine here — it has been the supported call since 10.6
        // and the modern `activate` is only available on macOS 14+.
        let _: bool = msg_send![cls, activateWithOptions: 0u64];
    }
}

/// Hide the Composer popover and return keyboard focus to whichever
/// app held it before the Composer opened.
///
/// History / rationale: `window.hide()` alone leaves the app "active"
/// on macOS — focus does not return to the prior app
/// (tauri-apps/tauri#7540). A prior attempt used `[NSApp hide:nil]`,
/// which mirrors Cmd+H. That worked for focus return but put the app
/// into a fully-hidden state, so the next `window.show()` (e.g.
/// Inbox shortcut) implicitly unhid the app and macOS restored every
/// previously-visible window — the Composer would pop back on screen
/// alongside the Inbox. Current implementation: hide the Composer
/// window normally and, when no other quick-capture window is on
/// screen, explicitly reactivate the PID NSWorkspace reported as
/// frontmost at the moment the Composer was summoned. Avoids any
/// app-level hide state.
#[tauri::command]
pub fn dismiss_composer(app: AppHandle) -> Result<(), String> {
    let app_handle = app.clone();
    app.run_on_main_thread(move || {
        if let Some(composer) = app_handle.get_webview_window("composer") {
            let _ = composer.hide();
        }
        let inbox_visible = app_handle
            .get_webview_window("inbox")
            .and_then(|w| w.is_visible().ok())
            .unwrap_or(false);
        if inbox_visible {
            if let Some(inbox) = app_handle.get_webview_window("inbox") {
                let _ = inbox.set_focus();
            }
        } else {
            #[cfg(target_os = "macos")]
            activate_prev_app();
        }
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
    let stroke = crate::current_menu_stroke();
    let mut menu = MenuBuilder::new(&app);
    for b in &bindings {
        let icon = crate::tray_menu_item_icon(b.tray.item, stroke);
        // Dock popup intentionally omits the accelerator hint — only
        // the Tray menu shows it. Same Lucide glyph per item for
        // visual parity with the Tray.
        let item = IconMenuItem::with_id(
            &app,
            b.menu_id,
            b.tray.label,
            true,
            Some(icon),
            None::<&str>,
        )
        .map_err(|e| format!("build dock menu item {}: {e}", b.menu_id))?;
        menu = menu.item(&item);
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
