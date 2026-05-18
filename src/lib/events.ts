// Single source of truth for Tauri event-bus names on the JS side.
// Mirrors `src-tauri/src/events.rs`. Renaming an event is a two-file
// edit (this file + events.rs); the Rust compiler catches every Rust
// usage, `pnpm check` catches every TS usage.
//
// Tauri 2's IllegalEventName regex rejects `.` as a separator —
// we use `:` for hierarchy throughout.

export const CAPTURES_CHANGED = "captures:changed" as const;
export const OPEN_COMPOSER = "open_composer" as const;
export const DOCK_PULSE = "dock:pulse" as const;
export const DOCK_UNREAD_CHANGED = "dock:unread:changed" as const;
export const DOCK_DRAG_ENTER = "dock:drag:enter" as const;
export const DOCK_DRAG_LEAVE = "dock:drag:leave" as const;
export const DOCK_FULLSCREEN_ENTERED = "dock:fullscreen:entered" as const;
export const DOCK_FULLSCREEN_EXITED = "dock:fullscreen:exited" as const;
export const TRAY_OPEN_INBOX = "tray:open_inbox" as const;
export const DESTINATIONS_CHANGED = "destinations:changed" as const;
