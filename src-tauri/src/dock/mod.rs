//! Persistent Dock widget — right-click menu intent registry and the
//! macOS fullscreen observer.
//!
//! Per ADR-0004 every system-side bit (NSWorkspace subscription, window
//! level math) lives here, not in JS. Per ADR-0005 the OS hook itself
//! is verified by manual smoke; what we unit-test is the seam — the
//! function that translates an enter/exit transition into the two named
//! Tauri events on a thin `EventSink`.
//!
//! The Dock right-click context menu mirrors the Tray menu by intent:
//! the same three actions (Open Composer, Open Inbox, Quit) with the
//! same event names. The menu order and the underlying `TrayMenuItem`
//! enum are reused from `tray::default_menu()` so labels and dispatch
//! arms stay in one place. Only the `menu_id` strings are
//! Dock-specific so the Tauri menu builder can tell a Dock-popup click
//! apart from a Tray click.

use crate::tray::{default_menu, TrayMenuBinding};

/// Event name emitted when the frontmost app enters fullscreen.
pub const EVENT_FULLSCREEN_ENTERED: &str = "dock.fullscreen.entered";

/// Event name emitted when the frontmost app exits fullscreen.
pub const EVENT_FULLSCREEN_EXITED: &str = "dock.fullscreen.exited";

/// Right-click menu binding for the Dock. Same intent as the Tray, but
/// with Dock-scoped `menu_id`s so the menu builder can route the popup
/// click back to the dispatcher without colliding with the Tray menu's
/// own `on_menu_event` registration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DockMenuBinding {
    pub tray: TrayMenuBinding,
    pub menu_id: &'static str,
}

/// Dock context menu in visual order: Open Composer, Open Inbox, Quit.
/// Reuses `tray::default_menu()` as the single source of truth for
/// label + event name; only the `menu_id` differs.
pub fn default_context_menu() -> Vec<DockMenuBinding> {
    let tray = default_menu();
    debug_assert_eq!(tray.len(), 3, "tray menu shape changed; update dock menu");
    vec![
        DockMenuBinding {
            tray: tray[0].clone(),
            menu_id: "dock.open_composer",
        },
        DockMenuBinding {
            tray: tray[1].clone(),
            menu_id: "dock.open_inbox",
        },
        DockMenuBinding {
            tray: tray[2].clone(),
            menu_id: "dock.quit",
        },
    ]
}

/// Logical fullscreen transition the observer detects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FullscreenTransition {
    Entered,
    Exited,
}

impl FullscreenTransition {
    pub fn event_name(self) -> &'static str {
        match self {
            FullscreenTransition::Entered => EVENT_FULLSCREEN_ENTERED,
            FullscreenTransition::Exited => EVENT_FULLSCREEN_EXITED,
        }
    }
}

/// Thin emit seam so the observer can be exercised without a real
/// Tauri runtime. `start` on the real observer uses the `AppHandle`
/// directly; the unit test injects a fake sink and asserts the right
/// event names land.
pub trait EventSink: Send + Sync {
    fn emit_event(&self, name: &str);
}

/// Translate an observed transition into a `dock.fullscreen.*` emit on
/// the sink. This is the only piece worth unit-testing — the OS hook
/// itself is verified by manual smoke.
pub fn emit_transition<S: EventSink + ?Sized>(sink: &S, transition: FullscreenTransition) {
    sink.emit_event(transition.event_name());
}

#[cfg(target_os = "macos")]
mod macos {
    //! macOS-backed `FullscreenObserver`. Subscribes to NSWorkspace's
    //! `activeSpaceDidChangeNotification` via
    //! `addObserverForName:object:queue:usingBlock:` — a closure-based
    //! API that avoids having to register a custom ObjC subclass.
    //! Each notification triggers a Rust closure that alternates
    //! `Entered` / `Exited` transitions and dispatches through the
    //! `emit_transition` seam.
    //!
    //! Per ADR-0005 the OS hook itself is manual-smoke; the seam (the
    //! `emit_transition` function and `EventSink` trait) has unit
    //! coverage in `tests/dock.rs`.

    use super::{emit_transition, EventSink, FullscreenTransition};
    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    use objc2_foundation::{NSNotification, NSString};
    use std::sync::{Arc, Mutex};
    use tauri::{AppHandle, Emitter};

    /// Adapter: a Tauri `AppHandle` is an `EventSink`.
    struct AppHandleSink(AppHandle);

    impl EventSink for AppHandleSink {
        fn emit_event(&self, name: &str) {
            let _ = self.0.emit(name, ());
        }
    }

    /// Holds the opaque observer token returned by
    /// `addObserverForName:...:usingBlock:` and unregisters it on drop.
    pub struct FullscreenObserverImpl {
        token: Retained<AnyObject>,
    }

    // SAFETY: the observer token is only touched from this struct's
    // Drop, which runs on the thread that owns the `FullscreenObserver`
    // — guaranteed to be main on macOS, where we instantiate it in
    // Tauri's `setup`. The notification block itself is dispatched by
    // NSNotificationCenter on the main queue (we pass `mainQueue` at
    // registration), so the closure body never races with Drop.
    unsafe impl Send for FullscreenObserverImpl {}
    unsafe impl Sync for FullscreenObserverImpl {}

    impl FullscreenObserverImpl {
        pub fn start(app: AppHandle) -> Self {
            // Captured into the block; the block (and therefore these
            // Arcs) is retained for the lifetime of the observer token
            // by NSNotificationCenter.
            let sink: Arc<dyn EventSink> = Arc::new(AppHandleSink(app));
            // Alternates Entered / Exited across active-space changes.
            // macOS doesn't surface a dedicated "fullscreen entered"
            // notification at the workspace layer; active-space change
            // is the closest stable proxy that fires both on enter
            // (into a fullscreen space) and exit (back to user space).
            let last_entered = Arc::new(Mutex::new(false));

            let block = RcBlock::new(move |_note: *mut NSNotification| {
                let mut g = last_entered.lock().unwrap();
                let now_entered = !*g;
                *g = now_entered;
                let transition = if now_entered {
                    FullscreenTransition::Entered
                } else {
                    FullscreenTransition::Exited
                };
                emit_transition(&*sink, transition);
            });

            let token: Retained<AnyObject> = unsafe {
                let workspace: *mut AnyObject =
                    msg_send![class!(NSWorkspace), sharedWorkspace];
                let center: *mut AnyObject = msg_send![workspace, notificationCenter];
                let name: Retained<NSString> =
                    NSString::from_str("NSWorkspaceActiveSpaceDidChangeNotification");
                let main_queue: *mut AnyObject =
                    msg_send![class!(NSOperationQueue), mainQueue];
                msg_send![
                    center,
                    addObserverForName: &*name,
                    object: std::ptr::null::<AnyObject>(),
                    queue: main_queue,
                    usingBlock: &*block,
                ]
            };
            // The notification center retains the block internally for
            // the lifetime of the observer token, so dropping our local
            // `RcBlock` here is safe.
            drop(block);

            Self { token }
        }
    }

    impl Drop for FullscreenObserverImpl {
        fn drop(&mut self) {
            unsafe {
                let workspace: *mut AnyObject =
                    msg_send![class!(NSWorkspace), sharedWorkspace];
                let center: *mut AnyObject = msg_send![workspace, notificationCenter];
                let _: () = msg_send![center, removeObserver: &*self.token];
            }
        }
    }
}

#[cfg(target_os = "macos")]
pub use macos::FullscreenObserverImpl as FullscreenObserver;

#[cfg(not(target_os = "macos"))]
mod stub {
    //! Non-macOS builds are not a supported target (see PRD "Out of
    //! scope"). The stub exists so `cargo check` on non-mac CI doesn't
    //! break the wider workspace.
    use tauri::AppHandle;

    pub struct FullscreenObserverImpl;

    impl FullscreenObserverImpl {
        pub fn start(_app: AppHandle) -> Self {
            Self
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub use stub::FullscreenObserverImpl as FullscreenObserver;
