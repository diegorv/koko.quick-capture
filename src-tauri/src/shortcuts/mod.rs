//! Intent registry for global shortcuts.
//!
//! Per ADR-0005 the OS hook is a thin wrapper; what we actually test is
//! the registry that maps a `ShortcutId` to its accelerator string and
//! the event name the app emits when it fires. The real OS binding is
//! verified by manual smoke (see slice 02 acceptance criteria).

/// Closed set of shortcut intents. Slice 02 wired `OpenComposer`;
/// slice 04 adds `CaptureClipboard`. v1.0 slice 01 adds `OpenInbox`
/// to the registry; v1.0 slice 02 wires the OS-level binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShortcutId {
    OpenComposer,
    CaptureClipboard,
    OpenInbox,
    OpenArchive,
}

/// One row in the registry: which accelerator triggers it, and which
/// event name is emitted on the Tauri event bus when it fires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutBinding {
    pub id: ShortcutId,
    pub accelerator: &'static str,
    pub event: &'static str,
}

/// Slice 04 registry: `OpenComposer` (Composer summon) and
/// `CaptureClipboard` (clipboard-text capture, no window). v1.0 slice
/// 01 adds `OpenInbox`; the OS-level shortcut for it is registered in
/// v1.0 slice 02.
pub fn default_registry() -> Vec<ShortcutBinding> {
    vec![
        ShortcutBinding {
            id: ShortcutId::OpenComposer,
            accelerator: "Ctrl+Alt+Cmd+Space",
            event: "open_composer",
        },
        ShortcutBinding {
            id: ShortcutId::CaptureClipboard,
            accelerator: "Ctrl+Alt+Cmd+C",
            event: "capture_clipboard",
        },
        ShortcutBinding {
            id: ShortcutId::OpenInbox,
            accelerator: "Ctrl+Alt+Cmd+I",
            event: "open_inbox",
        },
        ShortcutBinding {
            id: ShortcutId::OpenArchive,
            accelerator: "Ctrl+Alt+Cmd+A",
            event: "open_archive",
        },
    ]
}
