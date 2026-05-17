pub mod clipboard;
pub mod commands;
pub mod kind_detect;
pub mod shortcuts;
pub mod store;

use std::str::FromStr;

use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{
    Builder as ShortcutBuilder, Shortcut, ShortcutState,
};

use crate::clipboard::SystemClipboard;
use crate::commands::capture_clipboard_now_with;
use crate::shortcuts::{default_registry, ShortcutBinding, ShortcutId};
use crate::store::Store;

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
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
                let _ = app.emit(binding.event, ());
            }
            ShortcutId::CaptureClipboard => {
                let store = app.state::<Store>();
                match capture_clipboard_now_with(&SystemClipboard::new(), &store) {
                    Ok(capture) => {
                        let _ = app.emit(binding.event, &capture);
                    }
                    Err(e) => {
                        eprintln!("capture_clipboard_now failed: {e}");
                    }
                }
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
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::save_note,
            commands::capture_clipboard_now
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
