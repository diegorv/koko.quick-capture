//! Intent-registry test for the tray module. Per ADR-0005 we test the
//! menu registry seam, not the real OS-level tray icon (which is
//! verified by manual smoke).

use quick_capture_lib::tray::{default_menu, TrayMenuItem};

#[test]
fn default_menu_has_four_items_in_order() {
    let menu = default_menu();
    assert_eq!(menu.len(), 4);

    assert_eq!(menu[0].item, TrayMenuItem::OpenComposer);
    assert_eq!(menu[0].label, "Open Composer");
    assert_eq!(menu[0].event, "open_composer");

    assert_eq!(menu[1].item, TrayMenuItem::OpenInbox);
    assert_eq!(menu[1].label, "Open Inbox");
    assert_eq!(menu[1].event, "tray:open_inbox");

    assert_eq!(menu[2].item, TrayMenuItem::OpenSettings);
    assert_eq!(menu[2].label, "Settings…");
    assert_eq!(menu[2].event, "tray:open_settings");

    assert_eq!(menu[3].item, TrayMenuItem::Quit);
    assert_eq!(menu[3].label, "Quit");
    assert_eq!(menu[3].event, "tray:quit");
}
