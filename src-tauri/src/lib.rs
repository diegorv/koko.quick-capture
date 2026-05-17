pub mod clipboard;
pub mod commands;
pub mod dock;
pub mod drag_drop;
pub mod kind_detect;
pub mod shell;
pub mod shortcuts;
pub mod store;
pub mod tray;

use std::str::FromStr;

use tauri::{
    menu::MenuBuilder,
    tray::TrayIconBuilder,
    DragDropEvent, Emitter, Listener, LogicalPosition, LogicalSize, Manager, PhysicalPosition,
    WebviewUrl, WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_global_shortcut::{
    Builder as ShortcutBuilder, Shortcut, ShortcutState,
};

use crate::clipboard::SystemClipboard;
use crate::commands::{
    capture_clipboard_now_with, save_dropped_files_with_store, CAPTURES_CHANGED_EVENT,
    DOCK_PULSE_EVENT,
};
use crate::dock::{default_context_menu, FullscreenObserver};
use crate::shortcuts::{default_registry, ShortcutBinding, ShortcutId};
use crate::store::Store;
use crate::tray::{default_menu, TrayMenuItem};

/// Event emitted by the Dock window's drag-drop handler when a drag
/// gesture enters the Dock surface. The Dock JS subscribes to it to
/// toggle the `drag-active` visual class.
pub const DOCK_DRAG_ENTER_EVENT: &str = "dock:drag:enter";

/// Event emitted by the Dock window's drag-drop handler when the drag
/// gesture leaves the Dock (cancelled, drop fired, or cursor moved out).
pub const DOCK_DRAG_LEAVE_EVENT: &str = "dock:drag:leave";

/// Lucide `brain-circuit` SVG source, recoloured to pure black for
/// macOS template-mode treatment. macOS re-tints the icon per menubar
/// theme; the colour values here only matter for the alpha channel.
const TRAY_ICON_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 5a3 3 0 1 0-5.997.125 4 4 0 0 0-2.526 5.77 4 4 0 0 0 .556 6.588A4 4 0 1 0 12 18Z"/><path d="M9 13a4.5 4.5 0 0 0 3-4"/><path d="M6.003 5.125A3 3 0 0 0 6.401 6.5"/><path d="M3.477 10.896a4 4 0 0 1 .585-.396"/><path d="M6 18a4 4 0 0 1-1.967-.516"/><path d="M12 13h4"/><path d="M12 18h6a2 2 0 0 1 2 2v1"/><path d="M12 8h8"/><path d="M16 8V5a2 2 0 0 1 2-2"/><circle cx="16" cy="13" r=".5" fill="black"/><circle cx="18" cy="3" r=".5" fill="black"/><circle cx="20" cy="21" r=".5" fill="black"/><circle cx="20" cy="8" r=".5" fill="black"/></svg>"##;

// Menu-item icons are NOT template-mode treated by macOS (only the
// tray icon itself gets that auto-recolour). Tauri 2 / muda's
// `MenuBuilder.icon` renders the bytes as-is. Templates carry a
// `{STROKE}` placeholder swapped at build time for `white` (dark
// menubar) or `black` (light menubar); see `current_menu_stroke`.

/// Lucide `square-pen` (compose / new note), stroke parameterised.
const SQUARE_PEN_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="{STROKE}" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/><path d="M18.375 2.625a1 1 0 0 1 3 3l-9.013 9.014a2 2 0 0 1-.853.505l-2.873.84a.5.5 0 0 1-.62-.62l.84-2.873a2 2 0 0 1 .506-.852z"/></svg>"##;

/// Lucide `inbox`, stroke parameterised.
const INBOX_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="{STROKE}" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="22 12 16 12 14 15 10 15 8 12 2 12"/><path d="M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z"/></svg>"##;

/// Lucide `x` (close / quit), stroke parameterised.
const X_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="{STROKE}" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg>"##;

/// Pick the menu-item stroke colour based on the current macOS
/// appearance. Dark menubar -> white; light menubar -> black. Detected
/// once at menu-build time. Live theme switching while the app is
/// running is not handled here (follow-up); the user would need to
/// reopen the app to pick up a flipped appearance.
#[cfg(target_os = "macos")]
pub(crate) fn current_menu_stroke() -> &'static str {
    if macos_appearance::is_dark() {
        "white"
    } else {
        "black"
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn current_menu_stroke() -> &'static str {
    "white"
}

#[cfg(target_os = "macos")]
mod macos_appearance {
    //! Read `[NSApp effectiveAppearance].name` to decide whether the
    //! menubar is dark. macOS publishes several dark-flavour appearance
    //! names (DarkAqua, VibrantDark, plus a11y high-contrast variants);
    //! any name containing "Dark" is treated as dark.
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    use objc2_foundation::NSString;

    /// True when the current effective appearance is one of the dark
    /// variants. Defaults to true on any failure path so the icon stays
    /// visible on the typical dark menubar.
    pub fn is_dark() -> bool {
        unsafe {
            let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
            if app.is_null() {
                return true;
            }
            let appearance: *mut AnyObject = msg_send![app, effectiveAppearance];
            if appearance.is_null() {
                return true;
            }
            let name: *mut NSString = msg_send![appearance, name];
            if name.is_null() {
                return true;
            }
            (*name).to_string().contains("Dark")
        }
    }
}

/// Rasterise a Lucide SVG string into a square RGBA buffer at the
/// given pixel size. macOS reads only the alpha channel for tray /
/// menu icons so the source stroke colour does not matter, but we
/// fix it to black for clarity.
fn rasterise_svg(svg: &str, size: u32) -> tauri::image::Image<'static> {
    let opt = usvg::Options::default();
    let tree = usvg::Tree::from_str(svg, &opt).expect("parse svg");
    let mut pixmap = tiny_skia::Pixmap::new(size, size).expect("alloc pixmap");
    let scale = size as f32 / 24.0;
    let transform = tiny_skia::Transform::from_scale(scale, scale);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    tauri::image::Image::new_owned(pixmap.take(), size, size)
}

/// Rasterise the Lucide brain-circuit SVG into a 44x44 RGBA template
/// icon for the macOS Tray. 44px = 22pt @2x so it stays crisp on
/// Retina menubars. macOS reads the alpha channel and recolours.
///
/// Returns the raw RGBA8 buffer suitable for `tauri::image::Image::new_owned`.
fn build_brain_tray_icon_rgba() -> (Vec<u8>, u32, u32) {
    const SIZE: u32 = 44;
    let opt = usvg::Options::default();
    let tree = usvg::Tree::from_str(TRAY_ICON_SVG, &opt)
        .expect("parse tray icon SVG");

    let mut pixmap = tiny_skia::Pixmap::new(SIZE, SIZE)
        .expect("alloc tray icon pixmap");

    // SVG viewBox is 24x24; scale up to fill the 44x44 pixmap.
    let scale = SIZE as f32 / 24.0;
    let transform = tiny_skia::Transform::from_scale(scale, scale);
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    (pixmap.take(), SIZE, SIZE)
}

/// Tray menu items wear a per-item icon. Picks the right Lucide glyph
/// for each `TrayMenuItem`, swaps `{STROKE}` with the appearance-aware
/// stroke colour, and rasterises at 32x32 (16pt @2x).
pub(crate) fn tray_menu_item_icon(item: TrayMenuItem, stroke: &str) -> tauri::image::Image<'static> {
    let template = match item {
        TrayMenuItem::OpenComposer => SQUARE_PEN_SVG,
        TrayMenuItem::OpenInbox => INBOX_SVG,
        TrayMenuItem::Quit => X_SVG,
    };
    let svg = template.replace("{STROKE}", stroke);
    rasterise_svg(&svg, 32)
}

/// Flip the macOS activation policy between `.Regular` (Cmd+Tab and
/// system Dock icon visible) and `.Accessory` (neither). Used to
/// surface the app in Cmd+Tab only while the Inbox window is on
/// screen; closing the Inbox reverts the app to Accessory mode so the
/// system Dock icon does not stick around (see ADR-0009 for the
/// rationale behind the Accessory default).
///
/// Must run on the main thread on macOS — every caller already hops
/// via `run_on_main_thread` for the surrounding window operations.
#[cfg(target_os = "macos")]
pub(crate) fn set_inbox_activation_policy(app: &tauri::AppHandle, inbox_visible: bool) {
    let policy = if inbox_visible {
        tauri::ActivationPolicy::Regular
    } else {
        tauri::ActivationPolicy::Accessory
    };
    let _ = app.set_activation_policy(policy);
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn set_inbox_activation_policy(_app: &tauri::AppHandle, _inbox_visible: bool) {}

/// Intercept a window's native close gesture (red traffic-light /
/// Cmd+W with default chrome / system menu Close) so it hides instead
/// of destroying the window. macOS destruction would invalidate
/// subsequent `get_webview_window(label)` lookups, so every window in
/// this app is meant to live for the life of the process. The
/// `on_hide` closure runs after the hide and lets each surface attach
/// per-window cleanup (e.g. the Inbox flips the macOS activation
/// policy back to Accessory).
fn intercept_close_as_hide<F>(window: &tauri::WebviewWindow, on_hide: F)
where
    F: Fn() + Send + Sync + 'static,
{
    let target = window.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            let _ = target.hide();
            on_hide();
        }
    });
}

/// Hide the Inbox window and revert the macOS activation policy to
/// `.Accessory`. Mirrors the `CloseRequested` handler so the JS
/// "Esc / Cmd+W" path produces the same end state as clicking the
/// red traffic-light button — otherwise the JS-driven hide would
/// leave the app in `.Regular` mode and the system Dock icon would
/// linger until the next manual close.
#[tauri::command]
fn hide_inbox(app: tauri::AppHandle) -> Result<(), String> {
    let app_handle = app.clone();
    app.run_on_main_thread(move || {
        if let Some(window) = app_handle.get_webview_window("inbox") {
            let _ = window.hide();
        }
        set_inbox_activation_policy(&app_handle, false);
    })
    .map_err(|e| e.to_string())
}

/// Tray menu items show their keyboard shortcut on the right side of
/// the menu, matching the rest of the macOS app convention. For Open
/// Composer / Open Inbox these mirror the global shortcuts registered
/// via `tauri-plugin-global-shortcut`; macOS treats the menu
/// accelerator as a hint so we do not double-dispatch. `Cmd+Q` is the
/// standard Quit accelerator and only fires while the menu is open.
fn tray_menu_item_accelerator(item: TrayMenuItem) -> &'static str {
    match item {
        TrayMenuItem::OpenComposer => "Ctrl+Alt+Cmd+Space",
        TrayMenuItem::OpenInbox => "Ctrl+Alt+Cmd+I",
        TrayMenuItem::Quit => "Cmd+Q",
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let registry = default_registry();

    // Parse each accelerator once so the OS handler can dispatch by
    // comparing the `Shortcut` instance the plugin hands us back
    // against the one we registered. We cannot key on string form: the
    // `HotKey` Display impl normalizes (`control+alt+super+space`) but
    // our registry uses the user-facing `Ctrl+Opt+Cmd+Space` spelling.
    let parsed: Vec<(Shortcut, ShortcutBinding)> = registry
        .iter()
        .map(|b| {
            let s = Shortcut::from_str(b.accelerator)
                .expect("invalid accelerator string in default_registry");
            (s, b.clone())
        })
        .collect();

    let dispatch_table = parsed.clone();
    let mut builder = ShortcutBuilder::new().with_handler(move |app, shortcut, evt| {
        if evt.state() != ShortcutState::Pressed {
            return;
        }
        let Some((_, binding)) = dispatch_table.iter().find(|(s, _)| s == shortcut) else {
            return;
        };
        match binding.id {
            ShortcutId::OpenComposer => {
                commands::show_composer(app);
            }
            ShortcutId::CaptureClipboard => {
                let store = app.state::<Store>();
                match capture_clipboard_now_with(&SystemClipboard::new(), &store) {
                    Ok(captures) => {
                        // Emit the full batch so future UI surfaces can
                        // count N rows (e.g. for a multi-file copy).
                        let _ = app.emit(binding.event, &captures);
                        // Emit one captures.changed + dock.pulse per row
                        // so the Inbox can prepend each new Capture live
                        // and the Dock can pulse per row.
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
            ShortcutId::OpenInbox => {
                // The `binding.event` ("open_inbox") is intentionally
                // not emitted here — the Inbox window is shown
                // directly and nothing in JS listens for it.
                commands::show_inbox(app);
            }
        }
    });
    for (shortcut, _) in &parsed {
        builder = builder
            .with_shortcut(*shortcut)
            .expect("failed to register accelerator");
    }

    tauri::Builder::default()
        .plugin(builder.build())
        // Window-state plugin persists each window's size + position
        // across launches. Restrict the saved flags to SIZE + POSITION
        // only: restoring `VISIBLE` would force the Inbox and Composer
        // to appear on every relaunch (they live as hidden-until-
        // summoned windows), and restoring `DECORATIONS` would
        // overwrite the Composer's intentional `decorations(false)`.
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(
                    tauri_plugin_window_state::StateFlags::SIZE
                        | tauri_plugin_window_state::StateFlags::POSITION,
                )
                .build(),
        )
        .setup(|app| {
            // Accessory mode: no Dock icon, no system menu bar (see
            // ADR-0009). The app lives in the Tray and is summoned by
            // shortcuts.
            #[cfg(target_os = "macos")]
            {
                let _ = app
                    .handle()
                    .set_activation_policy(tauri::ActivationPolicy::Accessory);
            }

            let store = Store::open_default()
                .expect("failed to open capture store at the default path");
            app.manage(store);

            // Inbox (main) window is declared in tauri.conf.json with
            // label "inbox" and url "/inbox". It is the app shell;
            // future product screens (Settings, search, etc.) live as
            // routes inside it (ADR-0009). Intercept the close-button
            // so the window hides rather than being destroyed, which
            // would make subsequent `get_webview_window("inbox")`
            // return None.
            if let Some(inbox_window) = app.get_webview_window("inbox") {
                let close_app = app.handle().clone();
                intercept_close_as_hide(&inbox_window, move || {
                    // Revert to Accessory so the system Dock icon and
                    // Cmd+Tab entry disappear once the Inbox is no
                    // longer on screen.
                    set_inbox_activation_policy(&close_app, false);
                });
            }

            // Composer popover window: small, hidden at startup,
            // summoned by the global shortcut for Raycast-style
            // single-Note capture. Lives as its own Tauri window
            // because it must pop over any frontmost app.
            let composer_window = WebviewWindowBuilder::new(
                app,
                "composer",
                WebviewUrl::App("/composer".into()),
            )
            .visible(false)
            .title("")
            .inner_size(600.0, 240.0)
            .decorations(false)
            .transparent(true)
            .resizable(false)
            .skip_taskbar(true)
            .shadow(true)
            .center()
            .build()?;
            intercept_close_as_hide(&composer_window, || {});

            // Tray "Open Inbox" emits `tray.open_inbox` (see
            // `tray::default_menu`). Show + focus the Inbox window on
            // the main thread, mirroring the shortcut path. As with
            // the shortcut handler above, the unread cursor is left
            // alone here — `mark_inbox_opened` runs from the Inbox JS
            // on the first row interaction so a glance-and-dismiss
            // open does not silently clear pending captures.
            let inbox_app = app.handle().clone();
            app.listen("tray:open_inbox", move |_evt| {
                commands::show_inbox(&inbox_app);
            });

            // Tray menu: build from the testable registry so the
            // visible order and event names match `default_menu()`.
            // Each item gets an icon + an accelerator hint shown on
            // the right of the menu (the global shortcuts for Open
            // Composer / Open Inbox are already registered by
            // tauri-plugin-global-shortcut; the menu accelerator is
            // display-only on those two — macOS still shows the
            // shortcut next to the label even if the menu does not
            // re-dispatch).
            let menu_items = default_menu();
            let menu = {
                let stroke = current_menu_stroke();
                let mut menu = MenuBuilder::new(app);
                for binding in &menu_items {
                    let icon = tray_menu_item_icon(binding.item, stroke);
                    let accel = tray_menu_item_accelerator(binding.item);
                    let item = tauri::menu::IconMenuItem::with_id(
                        app,
                        binding.menu_id,
                        binding.label,
                        true,
                        Some(icon),
                        Some(accel),
                    )?;
                    menu = menu.item(&item);
                }
                menu.build()?
            };

            let dispatch: Vec<crate::tray::TrayMenuBinding> = menu_items.clone();
            let (brain_rgba, brain_w, brain_h) = build_brain_tray_icon_rgba();
            let brain_icon = tauri::image::Image::new_owned(brain_rgba, brain_w, brain_h);
            let _tray = TrayIconBuilder::new()
                .icon(brain_icon)
                .icon_as_template(true)
                .menu(&menu)
                .on_menu_event(move |app, event| {
                    let id = event.id().as_ref();
                    let Some(binding) = dispatch.iter().find(|b| b.menu_id == id) else {
                        return;
                    };
                    match binding.item {
                        TrayMenuItem::OpenComposer => {
                            commands::show_composer(app);
                        }
                        TrayMenuItem::OpenInbox => {
                            // The Inbox window subscribes to
                            // `tray:open_inbox` via `app.listen` in
                            // `setup`; emitting on the bus is enough.
                            let _ = app.emit(binding.event, ());
                        }
                        TrayMenuItem::Quit => {
                            app.exit(0);
                        }
                    }
                })
                .build(app)?;

            // Dock window. A small, frameless, always-on-top,
            // non-activating widget pinned to the bottom-left of the
            // active monitor. macOS NSPanel-like behavior is requested
            // via `focus(false) + accept_first_mouse(true)`. ADR-0008
            // expects this window to grow drag-drop wiring in slice 06.
            let dock_window = WebviewWindowBuilder::new(
                app,
                "dock",
                WebviewUrl::App("/dock".into()),
            )
            .title("")
            // Window is 96x96, but the visible Dock disc is 80x80 and
            // centered inside the window. The extra 16px ring around
            // the disc gives the unread-count badge room to overflow
            // the top-right corner without being clipped by the
            // window's own bounds.
            .inner_size(96.0, 96.0)
            .resizable(false)
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .focused(false)
            .accept_first_mouse(true)
            .visible(true)
            .build()?;

            // Position at bottom-left of the primary monitor with a
            // 16px margin from both edges. `primary_monitor()` returns
            // physical pixels; normalize through the monitor's scale
            // factor so the same `(16, 16)` margin lands correctly on
            // Retina and non-Retina displays.
            if let Some(monitor) = dock_window.primary_monitor()? {
                let scale = monitor.scale_factor();
                let m_pos = monitor.position();
                let m_size = monitor.size();
                // Top-left of monitor in logical coords:
                let monitor_logical_x = m_pos.x as f64 / scale;
                let monitor_logical_y = m_pos.y as f64 / scale;
                let monitor_logical_h = m_size.height as f64 / scale;
                // Window 96x96 (80x80 disc + 16px breathing room for
                // the unread badge), margin 16 from monitor edges,
                // anchored bottom-left. The disc still sits ~16px from
                // both edges because of the centered offset inside the
                // window.
                let x = monitor_logical_x + 16.0;
                let y = monitor_logical_y + monitor_logical_h - 96.0 - 16.0;
                dock_window.set_position(LogicalPosition::new(x, y))?;
            } else {
                // Fallback: place at a sane physical default so the
                // window is at least visible on first launch.
                let _ = dock_window
                    .set_position(PhysicalPosition::new(16i32, 16i32));
            }
            // Ensure the size took (some platforms reset on first show).
            let _ = dock_window.set_size(LogicalSize::new(96.0, 96.0));

            // Dock drag-drop: handle Finder file drops via Tauri's
            // native drag-drop channel (ADR-0008). Tauri 2.11 routes the
            // drag-drop callback from wry into the `WindowEvent::DragDrop`
            // synthesized variant when the webview's `kind` is
            // `WindowContent`, which is what `WebviewWindowBuilder`
            // builds without the `unstable` feature. So we listen on
            // `WebviewWindow::on_window_event` and match on
            // `WindowEvent::DragDrop(...)` (see
            // `tauri-runtime-wry/src/lib.rs` around line 4887). The
            // `Drop` save must run on the main thread to keep the SQLite
            // write off the Tauri event loop, mirroring the existing
            // `OpenComposer` / Tray pattern.
            let drag_drop_app = app.handle().clone();
            dock_window.on_window_event(move |event| {
                let WindowEvent::DragDrop(drag) = event else {
                    return;
                };
                match drag {
                    DragDropEvent::Enter { .. } => {
                        let _ = drag_drop_app.emit(DOCK_DRAG_ENTER_EVENT, ());
                    }
                    DragDropEvent::Leave => {
                        let _ = drag_drop_app.emit(DOCK_DRAG_LEAVE_EVENT, ());
                    }
                    DragDropEvent::Drop { paths, .. } => {
                        let app_handle = drag_drop_app.clone();
                        let paths = paths.clone();
                        let _ = drag_drop_app.run_on_main_thread(move || {
                            let store = app_handle.state::<Store>();
                            match save_dropped_files_with_store(&store, paths) {
                                Ok(captures) => {
                                    for capture in &captures {
                                        let _ =
                                            app_handle.emit(CAPTURES_CHANGED_EVENT, capture);
                                        let _ = app_handle.emit(DOCK_PULSE_EVENT, ());
                                    }
                                }
                                Err(e) => {
                                    eprintln!("save_dropped_files (drag-drop) failed: {e}");
                                }
                            }
                            // Reset the Dock's visual hover state once
                            // the drop has been processed; Tauri does
                            // not synthesize a `Leave` after `Drop`.
                            let _ = app_handle.emit(DOCK_DRAG_LEAVE_EVENT, ());
                        });
                    }
                    DragDropEvent::Over { .. } => {}
                    // `DragDropEvent` is `#[non_exhaustive]` — any
                    // future variant is ignored on this surface.
                    _ => {}
                }
            });

            // App-level menu event dispatcher for the Dock's
            // right-click popup menu. The popup is built and shown
            // per-invocation in `commands::open_dock_context_menu`,
            // but the click on a menu item lands here. The same item
            // intents are mirrored from the Tray (Open Composer, Open
            // Inbox, Quit) via `dock::default_context_menu()`; only
            // the `menu_id` prefix differs (`dock.*` vs `tray.*`).
            let dock_dispatch = default_context_menu();
            app.on_menu_event(move |app, event| {
                let id = event.id().as_ref();
                let Some(binding) = dock_dispatch.iter().find(|b| b.menu_id == id) else {
                    return;
                };
                match binding.tray.item {
                    TrayMenuItem::OpenComposer => {
                        commands::show_composer(app);
                    }
                    TrayMenuItem::OpenInbox => {
                        let _ = app.emit(binding.tray.event, ());
                    }
                    TrayMenuItem::Quit => {
                        app.exit(0);
                    }
                }
            });

            // Start the macOS fullscreen observer. The handle holds
            // the NSWorkspace observer token; dropping it (e.g. on
            // app exit) unregisters the notification. We stash it in
            // app state so it lives for the app's lifetime.
            let observer = FullscreenObserver::start(app.handle().clone());
            app.manage(observer);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::save_note,
            commands::capture_clipboard_now,
            commands::save_dropped_files,
            commands::list_captures,
            commands::star_capture,
            commands::delete_capture,
            commands::unread_count,
            commands::total_count,
            commands::mark_read,
            commands::open_composer_window,
            commands::dismiss_composer,
            hide_inbox,
            commands::open_dock_context_menu,
            commands::open_link,
            commands::reveal_capture
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
