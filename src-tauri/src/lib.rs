pub mod commands;
pub mod shortcuts;
pub mod store;

use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{Builder as ShortcutBuilder, ShortcutState};

use crate::shortcuts::{default_registry, ShortcutId};
use crate::store::Store;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let registry = default_registry();
    let open_composer = registry
        .iter()
        .find(|b| b.id == ShortcutId::OpenComposer)
        .expect("OpenComposer binding missing from default registry");
    let accelerator = open_composer.accelerator;
    let event_name = open_composer.event;

    tauri::Builder::default()
        .plugin(
            ShortcutBuilder::new()
                .with_handler(move |app, _shortcut, evt| {
                    if evt.state() != ShortcutState::Pressed {
                        return;
                    }
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                    let _ = app.emit(event_name, ());
                })
                .with_shortcut(accelerator)
                .expect("invalid accelerator string")
                .build(),
        )
        .setup(|app| {
            let store = Store::open_default()
                .expect("failed to open capture store at the default path");
            app.manage(store);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![commands::save_note])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
