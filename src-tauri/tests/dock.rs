//! Tests for the `dock` module.
//!
//! Per ADR-0005 we test seams, not OS hooks. Two seams here:
//!
//! 1. `default_context_menu()` — the right-click intent registry.
//!    Mirrors `tray::default_menu()` by intent (same labels, same
//!    event names); we assert the three expected items in order and
//!    that the dock-scoped `menu_id`s do not collide with the tray's.
//! 2. `emit_transition` — the function the macOS NSWorkspace observer
//!    calls when it detects a fullscreen enter / exit. We feed it a
//!    fake `EventSink` and assert the two named events fire.

use quick_capture_lib::dock::{
    default_context_menu, emit_transition, EventSink, FullscreenTransition,
};
use quick_capture_lib::events::{DOCK_FULLSCREEN_ENTERED, DOCK_FULLSCREEN_EXITED};
use quick_capture_lib::tray::{default_menu, TrayMenuItem};
use std::sync::Mutex;

#[test]
fn default_context_menu_has_five_items_in_order_mirroring_tray() {
    let menu = default_context_menu();
    assert_eq!(menu.len(), 5);

    assert_eq!(menu[0].tray.item, TrayMenuItem::OpenComposer);
    assert_eq!(menu[0].tray.label, "Open Composer");
    assert_eq!(menu[0].tray.event, "open_composer");
    assert_eq!(menu[0].menu_id, "dock:open_composer");

    assert_eq!(menu[1].tray.item, TrayMenuItem::OpenInbox);
    assert_eq!(menu[1].tray.label, "Open Inbox");
    assert_eq!(menu[1].tray.event, "tray:open_inbox");
    assert_eq!(menu[1].menu_id, "dock:open_inbox");

    assert_eq!(menu[2].tray.item, TrayMenuItem::OpenArchive);
    assert_eq!(menu[2].tray.label, "Open Archive…");
    assert_eq!(menu[2].tray.event, "tray:open_archive");
    assert_eq!(menu[2].menu_id, "dock:open_archive");

    assert_eq!(menu[3].tray.item, TrayMenuItem::OpenSettings);
    assert_eq!(menu[3].tray.label, "Settings…");
    assert_eq!(menu[3].tray.event, "tray:open_settings");
    assert_eq!(menu[3].menu_id, "dock:open_settings");

    assert_eq!(menu[4].tray.item, TrayMenuItem::Quit);
    assert_eq!(menu[4].tray.label, "Quit");
    assert_eq!(menu[4].tray.event, "tray:quit");
    assert_eq!(menu[4].menu_id, "dock:quit");
}

#[test]
fn dock_menu_ids_do_not_collide_with_tray_menu_ids() {
    let tray_ids: Vec<&str> = default_menu().iter().map(|b| b.menu_id).collect();
    let dock_ids: Vec<&str> = default_context_menu().iter().map(|b| b.menu_id).collect();
    for d in &dock_ids {
        assert!(
            !tray_ids.contains(d),
            "dock menu_id `{d}` collides with a tray menu_id; the app-level \
             on_menu_event dispatcher would route to the wrong handler"
        );
    }
}

/// Fake `EventSink` that records every emitted event name in order.
struct RecordingSink {
    events: Mutex<Vec<String>>,
}

impl RecordingSink {
    fn new() -> Self {
        Self {
            events: Mutex::new(Vec::new()),
        }
    }

    fn names(&self) -> Vec<String> {
        self.events.lock().unwrap().clone()
    }
}

impl EventSink for RecordingSink {
    fn emit_event(&self, name: &str) {
        self.events.lock().unwrap().push(name.to_string());
    }
}

#[test]
fn emit_transition_fires_entered_event_for_enter() {
    let sink = RecordingSink::new();
    emit_transition(&sink, FullscreenTransition::Entered);
    assert_eq!(sink.names(), vec![DOCK_FULLSCREEN_ENTERED]);
}

#[test]
fn emit_transition_fires_exited_event_for_exit() {
    let sink = RecordingSink::new();
    emit_transition(&sink, FullscreenTransition::Exited);
    assert_eq!(sink.names(), vec![DOCK_FULLSCREEN_EXITED]);
}

#[test]
fn emit_transition_preserves_event_order_across_multiple_transitions() {
    let sink = RecordingSink::new();
    emit_transition(&sink, FullscreenTransition::Entered);
    emit_transition(&sink, FullscreenTransition::Exited);
    emit_transition(&sink, FullscreenTransition::Entered);
    assert_eq!(
        sink.names(),
        vec![
            DOCK_FULLSCREEN_ENTERED.to_string(),
            DOCK_FULLSCREEN_EXITED.to_string(),
            DOCK_FULLSCREEN_ENTERED.to_string(),
        ]
    );
}
