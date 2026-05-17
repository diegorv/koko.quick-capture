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
    capture_clipboard_now_with, mark_inbox_opened_with_store, save_dropped_files_with_store,
    CAPTURES_CHANGED_EVENT, DOCK_BADGE_CLEARED_EVENT, DOCK_PULSE_EVENT,
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

/// Build a procedural template-mode brain icon for the macOS Tray.
///
/// Two overlapping stroked circles ("hemispheres") plus a centre line
/// and a pair of fold strokes per side. Rendered at 44x44 (2x the
/// standard 22pt menubar slot) so the icon stays crisp on Retina.
/// All non-zero pixels are pure black with full alpha; macOS treats
/// the result as a template image and re-tints to match the menubar.
///
/// Returns the raw RGBA8 buffer suitable for `tauri::image::Image::new_owned`.
fn build_brain_tray_icon_rgba() -> (Vec<u8>, u32, u32) {
    use image::{ImageBuffer, Rgba};

    const SIZE: u32 = 44;
    let mut img: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_pixel(SIZE, SIZE, Rgba([0, 0, 0, 0]));
    let on = Rgba([0, 0, 0, 255]);

    // Two hemisphere outlines. Centres offset left / right of the
    // canvas centre so they overlap in the middle.
    let hemis = [(15.0_f32, 22.0_f32), (29.0_f32, 22.0_f32)];
    let outer = 10.0_f32;
    let inner = 8.2_f32;

    // Fold arcs: smaller rings near the top + bottom of each hemisphere
    // suggesting cortical folds without trying to be anatomical.
    let folds: &[(f32, f32, f32)] = &[
        (12.5, 18.0, 3.3),
        (12.5, 26.0, 3.3),
        (31.5, 18.0, 3.3),
        (31.5, 26.0, 3.3),
    ];

    for y in 0..SIZE {
        for x in 0..SIZE {
            let fx = x as f32 + 0.5;
            let fy = y as f32 + 0.5;

            // Hemisphere rings.
            let mut hit = false;
            for (cx, cy) in &hemis {
                let r = ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt();
                if r >= inner && r <= outer {
                    hit = true;
                    break;
                }
            }
            // Fold arcs.
            if !hit {
                for (cx, cy, r0) in folds {
                    let r = ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt();
                    if (r - r0).abs() <= 0.9 {
                        hit = true;
                        break;
                    }
                }
            }
            // Spine: vertical line down the centre, only where it
            // would sit inside the union of the two hemispheres.
            if !hit {
                let dx = (fx - 22.0).abs();
                let inside_either = hemis.iter().any(|(cx, cy)| {
                    ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt() <= outer + 0.2
                });
                if dx <= 0.8 && inside_either && fy > 13.0 && fy < 31.0 {
                    hit = true;
                }
            }

            if hit {
                img.put_pixel(x, y, on);
            }
        }
    }

    (img.into_raw(), SIZE, SIZE)
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
                // macOS: show()/set_focus() must run on the main thread to
                // actually activate the app and grab keyboard focus. The
                // global-hotkey plugin invokes this handler on a worker.
                let app_handle = app.clone();
                let event = binding.event;
                let _ = app.run_on_main_thread(move || {
                    if let Some(window) = app_handle.get_webview_window("composer") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                    let _ = app_handle.emit(event, ());
                });
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
                // Mirror the OpenComposer path: show + focus must run
                // on the main thread on macOS to actually grab focus.
                // Also mark the Inbox as opened (advances the unread
                // cursor) and emit `dock.badge.cleared` so the Dock JS
                // zeroes its badge immediately.
                let app_handle = app.clone();
                let event = binding.event;
                let _ = app.run_on_main_thread(move || {
                    if let Some(window) = app_handle.get_webview_window("inbox") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                    let store = app_handle.state::<Store>();
                    if let Err(e) = mark_inbox_opened_with_store(&store) {
                        eprintln!("mark_inbox_opened (shortcut) failed: {e}");
                    }
                    let _ = app_handle.emit(DOCK_BADGE_CLEARED_EVENT, ());
                    let _ = app_handle.emit(event, ());
                });
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
                let inbox_clone = inbox_window.clone();
                inbox_window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = inbox_clone.hide();
                    }
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
            {
                let composer_clone = composer_window.clone();
                composer_window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = composer_clone.hide();
                    }
                });
            }

            // Tray "Open Inbox" emits `tray.open_inbox` (see
            // `tray::default_menu`). Show + focus the Inbox window on
            // the main thread, mirroring the shortcut path. Also mark
            // the Inbox as opened so the Dock's badge clears and the
            // new cursor persists across restarts.
            let inbox_app = app.handle().clone();
            app.listen("tray:open_inbox", move |_evt| {
                let app_handle = inbox_app.clone();
                let _ = inbox_app.run_on_main_thread(move || {
                    if let Some(window) = app_handle.get_webview_window("inbox") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                    let store = app_handle.state::<Store>();
                    if let Err(e) = mark_inbox_opened_with_store(&store) {
                        eprintln!("mark_inbox_opened (tray) failed: {e}");
                    }
                    let _ = app_handle.emit(DOCK_BADGE_CLEARED_EVENT, ());
                });
            });

            // Tray menu: build from the testable registry so the
            // visible order and event names match `default_menu()`.
            let menu_items = default_menu();
            let mut menu = MenuBuilder::new(app);
            for binding in &menu_items {
                menu = menu.text(binding.menu_id, binding.label);
            }
            let menu = menu.build()?;

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
                            // Same main-thread show/focus path the
                            // OpenComposer shortcut handler uses.
                            let app_handle = app.clone();
                            let event_name = binding.event;
                            let _ = app.run_on_main_thread(move || {
                                if let Some(window) = app_handle.get_webview_window("composer") {
                                    let _ = window.show();
                                    let _ = window.set_focus();
                                }
                                let _ = app_handle.emit(event_name, ());
                            });
                        }
                        TrayMenuItem::OpenInbox => {
                            // The Inbox window subscribes to
                            // `tray.open_inbox` via `app.listen` in
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
            .inner_size(80.0, 80.0)
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
                // Window 80x80, margin 16, anchored bottom-left.
                let x = monitor_logical_x + 16.0;
                let y = monitor_logical_y + monitor_logical_h - 80.0 - 16.0;
                dock_window.set_position(LogicalPosition::new(x, y))?;
            } else {
                // Fallback: place at a sane physical default so the
                // window is at least visible on first launch.
                let _ = dock_window
                    .set_position(PhysicalPosition::new(16i32, 16i32));
            }
            // Ensure the size took (some platforms reset on first show).
            let _ = dock_window.set_size(LogicalSize::new(80.0, 80.0));

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
                        let app_handle = app.clone();
                        let event_name = binding.tray.event;
                        let _ = app.run_on_main_thread(move || {
                            if let Some(window) = app_handle.get_webview_window("composer") {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                            let _ = app_handle.emit(event_name, ());
                        });
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
            commands::mark_inbox_opened,
            commands::open_composer_window,
            commands::open_dock_context_menu,
            commands::open_link,
            commands::reveal_capture
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
