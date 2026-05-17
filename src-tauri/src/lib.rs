pub mod clipboard;
pub mod commands;
pub mod kind_detect;
pub mod shortcuts;
pub mod store;
pub mod tray;

use std::str::FromStr;

use tauri::{
    menu::MenuBuilder,
    tray::TrayIconBuilder,
    Emitter, Manager,
};
use tauri_plugin_global_shortcut::{
    Builder as ShortcutBuilder, Shortcut, ShortcutState,
};

use crate::clipboard::SystemClipboard;
use crate::commands::capture_clipboard_now_with;
use crate::shortcuts::{default_registry, ShortcutBinding, ShortcutId};
use crate::store::Store;
use crate::tray::{default_menu, TrayMenuItem};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let registry = default_registry();

    // Parse each accelerator once so the OS handler can dispatch by
    // comparing the `Shortcut` instance the plugin hands us back
    // against the one we registered. We cannot key on string form: the
    // `HotKey` Display impl normalizes (`control+alt+super+space`) but
    // our registry uses the user-facing `Ctrl+Opt+Cmd+Space` spelling.
    //
    // `OpenInbox` lives in the registry so slice 02 doesn't churn the
    // tests, but its OS binding is wired in slice 02 (the Inbox window
    // doesn't exist yet). Filter it out here.
    let parsed: Vec<(Shortcut, ShortcutBinding)> = registry
        .iter()
        .filter(|b| b.id != ShortcutId::OpenInbox)
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
                    if let Some(window) = app_handle.get_webview_window("main") {
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
                    }
                    Err(e) => {
                        eprintln!("capture_clipboard_now failed: {e}");
                    }
                }
            }
            ShortcutId::OpenInbox => {
                // Slice 02 owns this. The binding is filtered out of
                // OS-level registration above, so this arm is never
                // reached in v1.0 slice 01; it exists to keep the
                // match exhaustive.
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
        .setup(|app| {
            let store = Store::open_default()
                .expect("failed to open capture store at the default path");
            app.manage(store);

            // Tray menu: build from the testable registry so the
            // visible order and event names match `default_menu()`.
            let menu_items = default_menu();
            let mut menu = MenuBuilder::new(app);
            for binding in &menu_items {
                menu = menu.text(binding.menu_id, binding.label);
            }
            let menu = menu.build()?;

            let dispatch: Vec<crate::tray::TrayMenuBinding> = menu_items.clone();
            let default_icon = app
                .default_window_icon()
                .cloned()
                .expect("default window icon must be embedded");
            let _tray = TrayIconBuilder::new()
                .icon(default_icon)
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
                                if let Some(window) = app_handle.get_webview_window("main") {
                                    let _ = window.show();
                                    let _ = window.set_focus();
                                }
                                let _ = app_handle.emit(event_name, ());
                            });
                        }
                        TrayMenuItem::OpenInbox => {
                            // Inbox window arrives in slice 02; for
                            // now just announce intent on the bus.
                            let _ = app.emit(binding.event, ());
                        }
                        TrayMenuItem::Quit => {
                            app.exit(0);
                        }
                    }
                })
                .build(app)?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::save_note,
            commands::capture_clipboard_now
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
