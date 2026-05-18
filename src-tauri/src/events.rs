//! Single source of truth for Tauri event-bus names.
//!
//! Every event that crosses the Rust <-> JS boundary lives here.
//! Rust emit / listen sites import the constant; the JS side mirrors
//! the same strings in `src/lib/events.ts`. Renaming an event is a
//! two-file edit (this file + `events.ts`); the compiler catches
//! every Rust usage, and `pnpm check` catches every JS usage that
//! imports the mirror.
//!
//! Tauri 2's `IllegalEventName` regex rejects `.` as a separator —
//! we use `:` for hierarchy throughout.
//!
//! Menu identifiers (the `menu_id` field on `TrayMenuBinding` /
//! `DockMenuBinding`) are deliberately NOT in this module: they are
//! muda widget IDs, not bus events, and never reach JS.

/// Emitted on every successful Capture mutation (save, star,
/// soft-delete). Payload: a full `Capture` on save, or a
/// `MutationNotice { id, kind }` on star / soft-delete.
pub const CAPTURES_CHANGED: &str = "captures:changed";

/// Emitted on every Composer-summon so the Composer route can bump
/// its `focusKey` and re-focus the textarea (the component is
/// mounted once for the life of the app).
pub const OPEN_COMPOSER: &str = "open_composer";

/// Emitted on every successful save. The Dock subscribes and fires
/// its one-shot pulse animation. Distinct from `CAPTURES_CHANGED`
/// so star / soft-delete (which emit changed but NOT pulse) can be
/// reasoned about independently.
pub const DOCK_PULSE: &str = "dock:pulse";

/// Emitted whenever the unread count changes server-side. Payload:
/// the new u64 unread count. The Dock JS overwrites its local badge
/// state with the payload so a race never leaves it out of sync
/// with the store.
pub const DOCK_UNREAD_CHANGED: &str = "dock:unread:changed";

/// Emitted when a Finder drag enters the Dock surface. The Dock JS
/// toggles the `drag-active` visual.
pub const DOCK_DRAG_ENTER: &str = "dock:drag:enter";

/// Emitted when a drag leaves the Dock surface (cancelled, dropped,
/// or moved out).
pub const DOCK_DRAG_LEAVE: &str = "dock:drag:leave";

/// Emitted when the frontmost app enters fullscreen. The Dock route
/// hides the Dock window on receipt.
pub const DOCK_FULLSCREEN_ENTERED: &str = "dock:fullscreen:entered";

/// Emitted when the frontmost app exits fullscreen. The Dock route
/// shows the Dock window again.
pub const DOCK_FULLSCREEN_EXITED: &str = "dock:fullscreen:exited";

/// Emitted by the tray menu's "Open Inbox" item. Rust subscribes via
/// `app.listen` and shows the Inbox window — same path the shortcut
/// takes, just routed through the bus so the menu handler stays
/// dispatch-only.
pub const TRAY_OPEN_INBOX: &str = "tray:open_inbox";

/// Emitted on every Destination mutation (create / update /
/// soft-delete / restore). Carries no payload — subscribers refetch
/// `list_destinations`. Settings, the triage picker, and the Archive
/// filter bar all listen.
pub const DESTINATIONS_CHANGED: &str = "destinations:changed";
