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
use crate::store::{
    Capture, CaptureContext, CaptureInput, CaptureKind, Destination, DestinationKind, Store,
    SETTING_WIKILINK_SOURCE_FOLDER,
};
use crate::wikilink::{read_people_dir, validate_folder, FolderError, PersonEntry};

use crate::events::{
    CAPTURES_CHANGED as CAPTURES_CHANGED_EVENT, DESTINATIONS_CHANGED as DESTINATIONS_CHANGED_EVENT,
    DOCK_PULSE as DOCK_PULSE_EVENT, DOCK_UNREAD_CHANGED as DOCK_UNREAD_CHANGED_EVENT, OPEN_COMPOSER,
    VIEW_OPEN_ARCHIVE, VIEW_OPEN_INBOX,
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
pub fn save_note_with_store(
    store: &Store,
    text: &str,
    ctx: CaptureContext,
) -> Result<Capture, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("note text is empty".to_string());
    }
    store
        .save_with_context(
            CaptureInput::Note {
                text: text.to_string(),
            },
            ctx,
        )
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_note(
    text: String,
    app: AppHandle,
    store: State<'_, Store>,
) -> Result<Capture, String> {
    // Resolve the app that was frontmost when the Composer was
    // summoned (`PrevFrontmostPid` is `peek()`ed so the dismiss
    // path's reactivation target stays intact). The browser context
    // (title + URL) is fetched via AppleScript with that bundle as
    // target — Chrome / Safari respond even when not frontmost, so
    // we get the tab the user was actually looking at when they
    // summoned the Composer.
    let bundle = {
        let pid = app.state::<PrevFrontmostPid>().peek();
        bundle_id_for_pid(pid)
    };
    let ctx = resolve_context_for_bundle(bundle.as_deref());
    let capture = save_note_with_store(&store, &text, ctx)?;
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
    ctx: CaptureContext,
) -> Result<Vec<Capture>, String> {
    let snapshot = clipboard.read().map_err(|e| e.to_string())?;
    let inputs = decide(snapshot).map_err(|e| e.to_string())?;
    // Track "latest stored" as we go so a multi-paste with two
    // identical items in a row still dedupes the second one.
    let mut latest: Option<Capture> = store
        .list_before(None, 1)
        .map_err(|e| e.to_string())?
        .into_iter()
        .next();
    let mut out = Vec::with_capacity(inputs.len());
    for input in inputs {
        if latest
            .as_ref()
            .map(|prev| is_clipboard_duplicate(prev, &input))
            .unwrap_or(false)
        {
            continue;
        }
        let saved = store
            .save_with_context(input, ctx.clone())
            .map_err(|e| e.to_string())?;
        latest = Some(saved.clone());
        out.push(saved);
    }
    Ok(out)
}

/// True when `new` carries the same payload as the most-recent
/// stored capture `prev`. Used to suppress re-saving when the user
/// triggers the clipboard shortcut with the same content twice.
///
/// Comparison is per-kind and string-based; binary `Shot { Bytes }`
/// is never deduped because we have no cheap way to compare the new
/// bytes against the stored blob without re-reading the file.
pub fn is_clipboard_duplicate(prev: &Capture, new: &CaptureInput) -> bool {
    use crate::store::ShotSource;
    match (prev.kind, new) {
        (CaptureKind::Clip, CaptureInput::Clip { text }) => {
            prev.payload.get("text").and_then(|v| v.as_str()) == Some(text.as_str())
        }
        (CaptureKind::Note, CaptureInput::Note { text }) => {
            prev.payload.get("text").and_then(|v| v.as_str()) == Some(text.as_str())
        }
        (CaptureKind::Link, CaptureInput::Link { url, .. }) => {
            prev.payload.get("url").and_then(|v| v.as_str()) == Some(url.as_str())
        }
        (CaptureKind::File, CaptureInput::File { source_path, .. }) => {
            prev.payload.get("source_path").and_then(|v| v.as_str())
                == Some(source_path.to_string_lossy().as_ref())
        }
        (
            CaptureKind::Shot,
            CaptureInput::Shot {
                source: ShotSource::Path { source_path, .. },
                ..
            },
        ) => {
            prev.payload.get("source_path").and_then(|v| v.as_str())
                == Some(source_path.to_string_lossy().as_ref())
        }
        _ => false,
    }
}

#[tauri::command]
pub fn capture_clipboard_now(
    app: AppHandle,
    store: State<'_, Store>,
) -> Result<Vec<Capture>, String> {
    let ctx = resolve_context_for_bundle(frontmost_bundle_id().as_deref());
    let captures =
        capture_clipboard_now_with(&SystemClipboard::new(), &store, ctx)?;
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
    mention: Option<&str>,
    limit: u32,
) -> Result<Vec<Capture>, String> {
    let parsed = match cursor {
        Some(s) => Some(Ulid::from_str(s).map_err(|e| format!("invalid cursor: {e}"))?),
        None => None,
    };
    store
        .list_before_filtered(parsed, mention, limit)
        .map_err(|e| e.to_string())
}

/// Full-text search across all non-deleted captures. Thin wrapper
/// around `Store::search` — the real query sanitisation + index
/// definition lives in the store module.
pub fn search_captures_with_store(
    store: &Store,
    query: &str,
    mention: Option<&str>,
    limit: u32,
) -> Result<Vec<Capture>, String> {
    store
        .search_filtered(query, mention, limit)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn search_captures(
    query: String,
    mention: Option<String>,
    limit: u32,
    store: State<'_, Store>,
) -> Result<Vec<Capture>, String> {
    search_captures_with_store(&store, &query, mention.as_deref(), limit)
}

#[tauri::command]
pub fn list_captures(
    cursor: Option<String>,
    mention: Option<String>,
    limit: u32,
    store: State<'_, Store>,
) -> Result<Vec<Capture>, String> {
    list_captures_with_store(&store, cursor.as_deref(), mention.as_deref(), limit)
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

/// Read the macOS clipboard, decide a capture kind, persist, and
/// broadcast the change to every interested surface. Lives on the
/// command surface so the global-shortcut handler in `lib.rs` is a
/// thin dispatcher rather than re-implementing the
/// capture+emit ceremony inline.
///
/// Multi-file pastes turn into N captures — we emit `CAPTURES_CHANGED`
/// + `DOCK_PULSE` per row so the Inbox can prepend each new row live
/// and the Dock pulses once per save.
pub fn capture_clipboard_and_broadcast(app: &AppHandle) {
    let store = app.state::<Store>();
    // The global shortcut fires while the user's prior app is still
    // frontmost (we are Accessory and never auto-activate), so
    // NSWorkspace.frontmostApplication is exactly the "source app"
    // we want to stamp on the new captures.
    let ctx = resolve_context_for_bundle(frontmost_bundle_id().as_deref());
    match capture_clipboard_now_with(&SystemClipboard::new(), &store, ctx) {
        Ok(captures) => {
            for capture in &captures {
                let _ = app.emit(CAPTURES_CHANGED_EVENT, capture);
                let _ = app.emit(DOCK_PULSE_EVENT, ());
            }
        }
        Err(e) => {
            eprintln!("capture_clipboard_now failed: {e}");
        }
    }
}

/// One place that knows how to bring the Composer to screen. Used by
/// the global shortcut, the tray menu, the Dock click invoke, and any
/// future entry point. Records the prior frontmost macOS app FIRST
/// so `dismiss_composer` can hand focus back, then hops to the main
/// thread for `show + set_focus` (macOS requires both on main).
pub fn show_composer(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        record_prev_frontmost(&handle);
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
/// handler) reverts the policy. After the window is on screen, emits
/// `VIEW_OPEN_INBOX` so the active route navigates to `/inbox` if it
/// is currently on `/archive`.
pub fn show_inbox(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        crate::set_inbox_activation_policy(&handle, true);
        if let Some(window) = handle.get_webview_window("inbox") {
            let _ = window.show();
            let _ = window.set_focus();
        }
        let _ = handle.emit(VIEW_OPEN_INBOX, ());
    });
}

/// Bring the main window to screen on the Archive view. Mirrors
/// `show_inbox` but emits `VIEW_OPEN_ARCHIVE` so the active route
/// navigates to `/archive` once the window is up.
pub fn show_archive(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        crate::set_inbox_activation_policy(&handle, true);
        if let Some(window) = handle.get_webview_window("inbox") {
            let _ = window.show();
            let _ = window.set_focus();
        }
        let _ = handle.emit(VIEW_OPEN_ARCHIVE, ());
    });
}

/// One place that knows how to bring the Settings window to screen.
/// Mirrors `show_inbox` but does not touch the activation policy
/// because Settings is meant as a transient configuration popover;
/// Cmd+Tab still surfaces the app via whatever the Inbox last set.
pub fn show_settings(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(window) = handle.get_webview_window("settings") {
            let _ = window.show();
            let _ = window.set_focus();
        }
    });
}

/// Tauri-managed slot that holds the macOS PID of whichever app was
/// frontmost just before quick-capture summoned the Composer.
/// `dismiss_composer` reads + clears the slot to hand focus back to
/// that exact app. Lives in `app.manage` (registered in lib::run
/// setup) rather than a process-global static so the seam is
/// inspectable in tests and follows ADR-0004's "shared state goes
/// through Tauri" pattern.
#[derive(Default)]
pub struct PrevFrontmostPid(pub std::sync::atomic::AtomicI32);

impl PrevFrontmostPid {
    pub fn new() -> Self {
        Self(std::sync::atomic::AtomicI32::new(-1))
    }

    pub fn store(&self, pid: i32) {
        self.0.store(pid, std::sync::atomic::Ordering::SeqCst);
    }

    /// Atomically read the stored PID and reset to -1.
    pub fn take(&self) -> i32 {
        self.0.swap(-1, std::sync::atomic::Ordering::SeqCst)
    }

    /// Read the stored PID without resetting it. Used by `save_note`
    /// to stamp `source_app` on the saved Capture while leaving the
    /// dismiss path's reactivation target intact.
    pub fn peek(&self) -> i32 {
        self.0.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// Snapshot the macOS frontmost app PID via NSWorkspace into the
/// app-managed `PrevFrontmostPid` slot. Called from every
/// Composer-summon path BEFORE we show the Composer, while the
/// user's real prior app is still frontmost. Our own PID is
/// filtered out so a re-summon while the Composer is already up
/// does not record us as the "prior" app.
#[cfg(target_os = "macos")]
pub fn record_prev_frontmost(app: &AppHandle) {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
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
            app.state::<PrevFrontmostPid>().store(pid);
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn record_prev_frontmost(_app: &AppHandle) {}

/// Read the bundle identifier of whatever macOS app is currently
/// frontmost. Used by clipboard capture (the shortcut handler is
/// invoked while the user's prior app is still frontmost — our app
/// stays Accessory and does not steal activation) to stamp
/// `source_app` on the saved Capture.
#[cfg(target_os = "macos")]
pub fn frontmost_bundle_id() -> Option<String> {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    use objc2_foundation::NSString;
    unsafe {
        let workspace: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
        if workspace.is_null() {
            return None;
        }
        let app: *mut AnyObject = msg_send![workspace, frontmostApplication];
        if app.is_null() {
            return None;
        }
        let bundle: *mut NSString = msg_send![app, bundleIdentifier];
        if bundle.is_null() {
            return None;
        }
        Some((*bundle).to_string())
    }
}

#[cfg(not(target_os = "macos"))]
pub fn frontmost_bundle_id() -> Option<String> {
    None
}

/// Resolve an NSRunningApplication by PID and return its bundle
/// identifier. Used by the Composer save path: the user's prior app
/// was snapshotted on summon via `PrevFrontmostPid`, and this is
/// how we turn the stored PID into the human-recognisable
/// `source_app` field.
#[cfg(target_os = "macos")]
pub fn bundle_id_for_pid(pid: i32) -> Option<String> {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    use objc2_foundation::NSString;
    if pid <= 0 {
        return None;
    }
    unsafe {
        let cls: *mut AnyObject =
            msg_send![class!(NSRunningApplication), runningApplicationWithProcessIdentifier: pid];
        if cls.is_null() {
            return None;
        }
        let bundle: *mut NSString = msg_send![cls, bundleIdentifier];
        if bundle.is_null() {
            return None;
        }
        Some((*bundle).to_string())
    }
}

#[cfg(not(target_os = "macos"))]
pub fn bundle_id_for_pid(_pid: i32) -> Option<String> {
    None
}

/// Build a `CaptureContext` from a macOS bundle id. Looks up the
/// active tab title + URL for known browsers (Chrome, Safari) via
/// AppleScript; returns app-only context for anything else. `None`
/// bundle id (resolution failed) yields all-`None` context.
pub fn resolve_context_for_bundle(bundle_id: Option<&str>) -> CaptureContext {
    let Some(bid) = bundle_id else {
        return CaptureContext::default();
    };
    let (title, url) = match bid {
        // Chrome (stable / Canary / Beta / Dev) all share the same
        // AppleScript dictionary; targeting by bundle id makes them
        // all work without a per-build branch.
        b if b.starts_with("com.google.Chrome") => browser_active_tab_chrome(b),
        b @ ("com.apple.Safari" | "com.apple.SafariTechnologyPreview") => {
            safari_active_tab(b)
        }
        _ => (None, None),
    };
    CaptureContext {
        source_app: Some(bid.to_string()),
        source_title: title,
        source_url: url,
    }
}

#[cfg(target_os = "macos")]
fn browser_active_tab_chrome(bundle: &str) -> (Option<String>, Option<String>) {
    let script = format!(
        r#"tell application id "{bundle}"
            if (count of windows) = 0 then return ""
            set t to active tab of front window
            return (URL of t) & "
" & (title of t)
        end tell"#
    );
    parse_url_then_title(run_osascript(&script))
}

#[cfg(not(target_os = "macos"))]
fn browser_active_tab_chrome(_bundle: &str) -> (Option<String>, Option<String>) {
    (None, None)
}

#[cfg(target_os = "macos")]
fn safari_active_tab(bundle: &str) -> (Option<String>, Option<String>) {
    let script = format!(
        r#"tell application id "{bundle}"
            if (count of documents) = 0 then return ""
            return (URL of front document) & "
" & (name of front document)
        end tell"#
    );
    parse_url_then_title(run_osascript(&script))
}

#[cfg(not(target_os = "macos"))]
fn safari_active_tab(_bundle: &str) -> (Option<String>, Option<String>) {
    (None, None)
}

/// Run an AppleScript snippet via `osascript -e`. Returns the
/// trimmed stdout on success, or `None` on spawn failure / non-zero
/// exit / empty output. macOS will prompt the user the first time
/// the app tries to script another app (Apple Events permission).
#[cfg(target_os = "macos")]
fn run_osascript(script: &str) -> Option<String> {
    let out = std::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?;
    let trimmed = s.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Parse a two-line "url\ntitle" string into (title, url). Both
/// fields are independently optional — a partial response from the
/// browser still surfaces whatever it managed to return.
fn parse_url_then_title(out: Option<String>) -> (Option<String>, Option<String>) {
    let Some(text) = out else {
        return (None, None);
    };
    let mut lines = text.lines();
    let url = lines
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let title = lines
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    (title, url)
}

/// Activate the app whose PID was last recorded by
/// `record_prev_frontmost`. Takes the stored PID (resetting to -1)
/// so a subsequent unrelated dismiss does not re-trigger.
#[cfg(target_os = "macos")]
fn activate_prev_app(app: &AppHandle) {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    let pid = app.state::<PrevFrontmostPid>().take();
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
            activate_prev_app(&app_handle);
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

/// Resolve the on-disk path of the SQLite captures DB. Used by the
/// Settings page to show the path and to power "Reveal in Finder".
#[tauri::command]
pub fn get_db_path() -> Result<String, String> {
    crate::store::default_db_path()
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(|e| e.to_string())
}

/// Reveal the captures DB in macOS Finder (the parent folder opens
/// with the file selected). Settings page button.
#[tauri::command]
pub fn reveal_db_in_finder() -> Result<(), String> {
    let path = crate::store::default_db_path().map_err(|e| e.to_string())?;
    SystemShell::new()
        .reveal_in_finder(&path)
        .map_err(|e| e.to_string())
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

// ── Destinations + routing (ADR-0010) ─────────────────────────────

/// List live Destinations, alpha-sorted. Settings + triage picker +
/// Archive filter bar all consume this surface; soft-deleted rows
/// come back via `list_deleted_destinations`.
pub fn list_destinations_with_store(store: &Store) -> Result<Vec<Destination>, String> {
    store.destinations_list_live().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_destinations(store: State<'_, Store>) -> Result<Vec<Destination>, String> {
    list_destinations_with_store(&store)
}

/// List soft-deleted Destinations. Drives the "Soft-deleted" section
/// of the Settings panel for restore.
pub fn list_deleted_destinations_with_store(store: &Store) -> Result<Vec<Destination>, String> {
    store
        .destinations_list_deleted()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_deleted_destinations(store: State<'_, Store>) -> Result<Vec<Destination>, String> {
    list_deleted_destinations_with_store(&store)
}

/// Translate the optional `kind` string from JS into a `DestinationKind`.
/// Defaults to `Label` when the field is absent so older callers keep
/// working without passing a kind.
pub fn parse_destination_kind(kind: Option<&str>) -> Result<DestinationKind, String> {
    match kind {
        None => Ok(DestinationKind::Label),
        Some(value) => DestinationKind::parse(value).map_err(|e| e.to_string()),
    }
}

/// Create a Destination. Returns the new row. Surfaces conflict +
/// blank-name errors as string messages so the UI can prompt the
/// user without parsing a typed error.
pub fn create_destination_with_store(
    store: &Store,
    name: &str,
    color: Option<&str>,
    kind: DestinationKind,
    config: Option<&str>,
) -> Result<Destination, String> {
    store
        .destination_create(name, color, kind, config)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_destination(
    name: String,
    color: Option<String>,
    kind: Option<String>,
    config: Option<String>,
    app: AppHandle,
    store: State<'_, Store>,
) -> Result<Destination, String> {
    let kind = parse_destination_kind(kind.as_deref())?;
    let created = create_destination_with_store(
        &store,
        &name,
        color.as_deref(),
        kind,
        config.as_deref(),
    )?;
    let _ = app.emit(DESTINATIONS_CHANGED_EVENT, ());
    Ok(created)
}

pub fn update_destination_with_store(
    store: &Store,
    id: &str,
    name: &str,
    color: Option<&str>,
    kind: DestinationKind,
    config: Option<&str>,
) -> Result<(), String> {
    store
        .destination_update(id, name, color, kind, config)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_destination(
    id: String,
    name: String,
    color: Option<String>,
    kind: Option<String>,
    config: Option<String>,
    app: AppHandle,
    store: State<'_, Store>,
) -> Result<(), String> {
    let kind = parse_destination_kind(kind.as_deref())?;
    update_destination_with_store(
        &store,
        &id,
        &name,
        color.as_deref(),
        kind,
        config.as_deref(),
    )?;
    let _ = app.emit(DESTINATIONS_CHANGED_EVENT, ());
    Ok(())
}

pub fn soft_delete_destination_with_store(store: &Store, id: &str) -> Result<(), String> {
    store
        .destination_soft_delete(id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn soft_delete_destination(
    id: String,
    app: AppHandle,
    store: State<'_, Store>,
) -> Result<(), String> {
    soft_delete_destination_with_store(&store, &id)?;
    let _ = app.emit(DESTINATIONS_CHANGED_EVENT, ());
    Ok(())
}

pub fn restore_destination_with_store(store: &Store, id: &str) -> Result<(), String> {
    store.destination_restore(id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn restore_destination(
    id: String,
    app: AppHandle,
    store: State<'_, Store>,
) -> Result<(), String> {
    restore_destination_with_store(&store, &id)?;
    let _ = app.emit(DESTINATIONS_CHANGED_EVENT, ());
    Ok(())
}

/// Route a Capture to a Destination. Parses the capture id as a ULID,
/// then delegates to `Store::capture_route`. The Tauri wrapper emits
/// `captures:changed` and `dock:unread:changed` so the Inbox, Archive,
/// and Dock badge all re-sync.
pub fn route_capture_with_store(
    store: &Store,
    id: &str,
    destination_id: &str,
) -> Result<(), String> {
    let parsed = Ulid::from_str(id).map_err(|e| format!("invalid id: {e}"))?;
    store
        .capture_route(&parsed, destination_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn route_capture(
    id: String,
    destination_id: String,
    app: AppHandle,
    store: State<'_, Store>,
) -> Result<(), String> {
    route_capture_with_store(&store, &id, &destination_id)?;
    // Deliberately does NOT emit DOCK_PULSE: the pulse is reserved
    // for inbound saves, conflating it with outbound triage would
    // muddy the Dock's "you just captured something" signal.
    let _ = app.emit(
        CAPTURES_CHANGED_EVENT,
        MutationNotice {
            id: &id,
            kind: "routed",
        },
    );
    if let Ok(unread) = store.count_unread() {
        let _ = app.emit(DOCK_UNREAD_CHANGED_EVENT, unread);
    }
    Ok(())
}

/// Route a Capture to a kokobrain Destination. Loads the Capture +
/// Destination, builds the `kokobrain://capture` URI, fires it through
/// the injected `opener` callable, and only then marks the Capture
/// routed. If the URI dispatch fails, the Capture stays in the Inbox
/// (the route is aborted). See ADR-0012.
///
/// `opener` is injected so tests can drive the helper without a Tauri
/// runtime; the real command below passes `OpenerExt::open_url`.
pub fn route_to_kokobrain_with_store(
    store: &Store,
    opener: &dyn Fn(&str) -> Result<(), String>,
    capture_id: &str,
    destination_id: &str,
) -> Result<String, String> {
    let parsed = Ulid::from_str(capture_id).map_err(|e| format!("invalid id: {e}"))?;
    let capture = store
        .find_with_deleted(&parsed)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("capture not found: {capture_id}"))?;
    if capture.deleted_at.is_some() {
        return Err(format!("capture is deleted: {capture_id}"));
    }
    let destination = store
        .destination_find(destination_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("destination not found: {destination_id}"))?;
    let uri = crate::kokobrain::build_capture_uri(&capture, &destination)
        .map_err(|e| e.to_string())?;
    opener(&uri)?;
    store
        .capture_route(&parsed, destination_id)
        .map_err(|e| e.to_string())?;
    Ok(uri)
}

#[tauri::command]
pub fn route_to_kokobrain(
    id: String,
    destination_id: String,
    app: AppHandle,
    store: State<'_, Store>,
) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;

    let opener_app = app.clone();
    let opener = move |uri: &str| -> Result<(), String> {
        opener_app
            .opener()
            .open_url(uri, None::<&str>)
            .map_err(|e| e.to_string())
    };
    route_to_kokobrain_with_store(&store, &opener, &id, &destination_id)?;
    let _ = app.emit(
        CAPTURES_CHANGED_EVENT,
        MutationNotice {
            id: &id,
            kind: "routed",
        },
    );
    if let Ok(unread) = store.count_unread() {
        let _ = app.emit(DOCK_UNREAD_CHANGED_EVENT, unread);
    }
    Ok(())
}

/// Un-route a Capture back to the Inbox. Read-state survives, so the
/// Dock unread count does not change; we still emit
/// `captures:changed` so both list views re-fetch.
pub fn unroute_capture_with_store(store: &Store, id: &str) -> Result<(), String> {
    let parsed = Ulid::from_str(id).map_err(|e| format!("invalid id: {e}"))?;
    store.capture_unroute(&parsed).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn unroute_capture(
    id: String,
    app: AppHandle,
    store: State<'_, Store>,
) -> Result<(), String> {
    unroute_capture_with_store(&store, &id)?;
    let _ = app.emit(
        CAPTURES_CHANGED_EVENT,
        MutationNotice {
            id: &id,
            kind: "unrouted",
        },
    );
    Ok(())
}

/// Archive listing. `destination_id = None` returns every Routed
/// Capture; passing one narrows to that Destination's bucket.
pub fn list_archive_with_store(
    store: &Store,
    destination_id: Option<&str>,
    mention: Option<&str>,
    cursor: Option<&str>,
    limit: u32,
) -> Result<Vec<Capture>, String> {
    store
        .list_archive_filtered(destination_id, mention, cursor, limit)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_archive(
    destination_id: Option<String>,
    mention: Option<String>,
    cursor: Option<String>,
    limit: u32,
    store: State<'_, Store>,
) -> Result<Vec<Capture>, String> {
    list_archive_with_store(
        &store,
        destination_id.as_deref(),
        mention.as_deref(),
        cursor.as_deref(),
        limit,
    )
}

/// Archive FTS search. Mirrors `search_captures` but scopes results
/// to Routed Captures (and optionally one Destination).
pub fn search_archive_with_store(
    store: &Store,
    query: &str,
    destination_id: Option<&str>,
    mention: Option<&str>,
    limit: u32,
) -> Result<Vec<Capture>, String> {
    store
        .search_archive_filtered(query, destination_id, mention, limit)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn search_archive(
    query: String,
    destination_id: Option<String>,
    mention: Option<String>,
    limit: u32,
    store: State<'_, Store>,
) -> Result<Vec<Capture>, String> {
    search_archive_with_store(
        &store,
        &query,
        destination_id.as_deref(),
        mention.as_deref(),
        limit,
    )
}

/// Count of un-routed, non-deleted Captures. Drives the Inbox/Archive
/// switcher's "Inbox (N)" badge.
pub fn inbox_count_with_store(store: &Store) -> Result<u64, String> {
    store.count_inbox().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn inbox_count(store: State<'_, Store>) -> Result<u64, String> {
    inbox_count_with_store(&store)
}

// ── Wikilink source folder + autocomplete (ADR-0011) ──────────────

/// Read the user-configured Wikilink source folder. Returns `None`
/// when unset (empty-string sentinel or missing row). The Settings page
/// uses this to render the current state; the Composer never calls
/// this — it goes through `list_people`.
pub fn get_wikilink_source_folder_with_store(
    store: &Store,
) -> Result<Option<String>, String> {
    let raw = store
        .settings_get(SETTING_WIKILINK_SOURCE_FOLDER)
        .map_err(|e| e.to_string())?;
    Ok(raw.filter(|s| !s.is_empty()))
}

#[tauri::command]
pub fn get_wikilink_source_folder(
    store: State<'_, Store>,
) -> Result<Option<String>, String> {
    get_wikilink_source_folder_with_store(&store)
}

/// Persist a new Wikilink source folder. `None` (or an empty string)
/// clears the setting back to "unset" — stored as `""` so callers can
/// tell "explicitly cleared" from "never written" if that ever
/// matters. A `Some(path)` is validated (exists + is a readable
/// directory) and any failure surfaces as a string the Settings page
/// can render inline.
pub fn set_wikilink_source_folder_with_store(
    store: &Store,
    path: Option<&str>,
) -> Result<(), String> {
    match path {
        None | Some("") => store
            .settings_set(SETTING_WIKILINK_SOURCE_FOLDER, "")
            .map_err(|e| e.to_string()),
        Some(p) => {
            validate_folder(std::path::Path::new(p)).map_err(|e| e.to_string())?;
            store
                .settings_set(SETTING_WIKILINK_SOURCE_FOLDER, p)
                .map_err(|e| e.to_string())
        }
    }
}

#[tauri::command]
pub fn set_wikilink_source_folder(
    path: Option<String>,
    store: State<'_, Store>,
) -> Result<(), String> {
    set_wikilink_source_folder_with_store(&store, path.as_deref())
}

/// Read top-level `.md` filenames from the configured source folder
/// for the Composer's `[[` autocomplete. Reads the folder path from
/// `app_settings` itself so the JS side never has to thread settings
/// state across the (separate) Composer / Settings windows.
///
/// Returns an empty Vec when the folder is unset or set-but-missing —
/// these are user-visible "feature dormant" / "feature broken but
/// recoverable" states (Q9b), not errors. Genuine IO failures during
/// enumeration still surface as `Err` so they can be logged.
pub fn list_people_with_store(store: &Store) -> Result<Vec<PersonEntry>, String> {
    let configured = match get_wikilink_source_folder_with_store(store)? {
        Some(p) => p,
        None => return Ok(Vec::new()),
    };
    match read_people_dir(std::path::Path::new(&configured)) {
        Ok(rows) => Ok(rows),
        Err(FolderError::NotFound) | Err(FolderError::NotADirectory) => Ok(Vec::new()),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub fn list_people(store: State<'_, Store>) -> Result<Vec<PersonEntry>, String> {
    list_people_with_store(&store)
}

/// Reveal the configured Wikilink source folder in macOS Finder.
/// Errors when no folder is set or when the configured folder no
/// longer exists on disk — the Settings UI calls this only after
/// confirming a path is present, but the disk state can drift.
pub fn reveal_wikilink_source_folder_with(
    shell: &dyn Shell,
    store: &Store,
) -> Result<(), String> {
    let folder = get_wikilink_source_folder_with_store(store)?
        .ok_or_else(|| "no folder configured".to_string())?;
    let path = std::path::Path::new(&folder);
    if !path.exists() {
        return Err("folder no longer exists on disk".to_string());
    }
    shell.reveal_in_finder(path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn reveal_wikilink_source_folder(
    store: State<'_, Store>,
) -> Result<(), String> {
    reveal_wikilink_source_folder_with(&SystemShell::new(), &store)
}

/// List the distinct `[[Name]]` mentions on a single capture,
/// alpha-sorted by lowercased name. Powers the detail pane's
/// clickable mention chips. Pure pass-through to
/// `Store::mentions_for_capture` after ULID parsing.
pub fn list_capture_mentions_with_store(
    store: &Store,
    id: &str,
) -> Result<Vec<String>, String> {
    let parsed = Ulid::from_str(id).map_err(|e| format!("invalid id: {e}"))?;
    store
        .mentions_for_capture(&parsed)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_capture_mentions(
    id: String,
    store: State<'_, Store>,
) -> Result<Vec<String>, String> {
    list_capture_mentions_with_store(&store, &id)
}

/// Open the native folder picker and return the chosen path, or
/// `None` if the user cancelled. Kept as a Rust command (not a
/// direct JS call to the dialog plugin) to keep ADR-0004 ("Rust
/// owns all data and system access") clean and self-consistent.
///
/// Uses the async callback API + an mpsc channel + a blocking
/// thread to await the result. The `blocking_pick_folder` variant
/// deadlocks here on macOS — the dialog must run on the main
/// thread and `blocking_pick_folder` blocks the caller waiting for
/// it; when the caller IS the main thread the dialog never gets a
/// chance to show. The async command + spawn_blocking pattern
/// keeps the wait off both the main thread and the tokio worker.
#[tauri::command]
pub async fn pick_wikilink_source_folder(
    app: AppHandle,
) -> Result<Option<String>, String> {
    use std::sync::mpsc;
    use tauri_plugin_dialog::DialogExt;

    let (tx, rx) = mpsc::channel();
    app.dialog()
        .file()
        .set_title("Choose Wikilink source folder")
        .pick_folder(move |path| {
            let _ = tx.send(path);
        });
    let chosen = tauri::async_runtime::spawn_blocking(move || {
        rx.recv().unwrap_or(None)
    })
    .await
    .map_err(|e| e.to_string())?;
    Ok(chosen.map(|fp| fp.to_string()))
}
