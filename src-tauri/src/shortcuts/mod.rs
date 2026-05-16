//! Intent registry for global shortcuts.
//!
//! Per ADR-0005 the OS hook is a thin wrapper; what we actually test is
//! the registry that maps a `ShortcutId` to its accelerator string and
//! the event name the app emits when it fires. The real OS binding is
//! verified by manual smoke (see slice 02 acceptance criteria).

/// Closed set of shortcut intents. Slice 02 only wires `OpenComposer`;
/// `CaptureClipboard` lands in slice 04. It lives in the enum here so
/// the registry shape does not change between slices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShortcutId {
    OpenComposer,
    CaptureClipboard,
}

/// One row in the registry: which accelerator triggers it, and which
/// event name is emitted on the Tauri event bus when it fires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutBinding {
    pub id: ShortcutId,
    pub accelerator: &'static str,
    pub event: &'static str,
}

/// Slice 02 registry: only `OpenComposer` is active. `CaptureClipboard`
/// will be added by slice 04 — leaving it out here keeps the OS-side
/// registration minimal and aligns with the slice scope.
pub fn default_registry() -> Vec<ShortcutBinding> {
    vec![ShortcutBinding {
        id: ShortcutId::OpenComposer,
        accelerator: "Ctrl+Opt+Cmd+Space",
        event: "open_composer",
    }]
}
