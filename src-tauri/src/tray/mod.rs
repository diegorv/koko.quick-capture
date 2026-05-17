//! Intent registry for the macOS menubar (Tray) menu.
//!
//! Per ADR-0005 the OS hook is a thin wrapper; what we actually test
//! is the registry that maps a `TrayMenuItem` to its label, the event
//! name emitted on the Tauri event bus, and the internal menu id used
//! by `TrayIconBuilder`'s `on_menu_event` to dispatch back to a
//! `TrayMenuItem`. The real tray icon is verified by manual smoke.

/// Closed set of tray menu intents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrayMenuItem {
    OpenComposer,
    OpenInbox,
    Quit,
}

/// One row in the menu registry.
///
/// `menu_id` is the stable string Tauri's menu builder uses to identify
/// the item when `on_menu_event` fires; the handler maps it back to a
/// `TrayMenuItem` for dispatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayMenuBinding {
    pub item: TrayMenuItem,
    pub label: &'static str,
    pub event: &'static str,
    pub menu_id: &'static str,
}

/// Menu order is the visual order shown to the user: Open Composer,
/// Open Inbox, Quit.
pub fn default_menu() -> Vec<TrayMenuBinding> {
    vec![
        TrayMenuBinding {
            item: TrayMenuItem::OpenComposer,
            label: "Open Composer",
            event: "open_composer",
            menu_id: "tray.open_composer",
        },
        TrayMenuBinding {
            item: TrayMenuItem::OpenInbox,
            label: "Open Inbox",
            event: "tray.open_inbox",
            menu_id: "tray.open_inbox",
        },
        TrayMenuBinding {
            item: TrayMenuItem::Quit,
            label: "Quit",
            event: "tray.quit",
            menu_id: "tray.quit",
        },
    ]
}
