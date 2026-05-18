//! Intent-registry test for the shortcuts module. Per ADR-0005 we test
//! the registry seam, not the real OS hook.

use quick_capture_lib::shortcuts::{default_registry, ShortcutId};

#[test]
fn default_registry_contains_open_composer_binding() {
    let bindings = default_registry();
    let open_composer = bindings
        .iter()
        .find(|b| b.id == ShortcutId::OpenComposer)
        .expect("OpenComposer binding must be present");

    assert_eq!(open_composer.accelerator, "Ctrl+Alt+Cmd+Space");
    assert_eq!(open_composer.event, "open_composer");
}

#[test]
fn default_registry_contains_capture_clipboard_binding() {
    let bindings = default_registry();
    let capture_clipboard = bindings
        .iter()
        .find(|b| b.id == ShortcutId::CaptureClipboard)
        .expect("CaptureClipboard binding must be present");

    assert_eq!(capture_clipboard.accelerator, "Ctrl+Alt+Cmd+C");
    assert_eq!(capture_clipboard.event, "capture_clipboard");
}

#[test]
fn default_registry_contains_open_inbox_binding() {
    let bindings = default_registry();
    let open_inbox = bindings
        .iter()
        .find(|b| b.id == ShortcutId::OpenInbox)
        .expect("OpenInbox binding must be present");

    assert_eq!(open_inbox.accelerator, "Ctrl+Alt+Cmd+I");
    assert_eq!(open_inbox.event, "open_inbox");
}

#[test]
fn default_registry_contains_open_archive_binding() {
    let bindings = default_registry();
    let open_archive = bindings
        .iter()
        .find(|b| b.id == ShortcutId::OpenArchive)
        .expect("OpenArchive binding must be present");

    assert_eq!(open_archive.accelerator, "Ctrl+Alt+Cmd+A");
    assert_eq!(open_archive.event, "open_archive");
}
