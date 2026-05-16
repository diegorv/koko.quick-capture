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

    assert_eq!(open_composer.accelerator, "Ctrl+Opt+Cmd+Space");
    assert_eq!(open_composer.event, "open_composer");
}
