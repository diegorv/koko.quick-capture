# quick-capture — Domain Glossary

Single source of truth for terms used in this project. Implementation lives in code; this file is glossary only.

## Capture

A single saved unit produced by the user. The product noun.

- The user performs a **capture** (verb / action) to create a **Capture** (noun / record).
- Every Capture has a kind, a payload, a creation timestamp, and the source context it came from.
- Plural: **Captures**.

Variants are named `<Kind>Capture` (e.g. `LinkCapture`) when a specific kind is meant; bare `Capture` means any kind.

## Capture kinds

Closed set. Adding a kind is an explicit change everywhere kinds are handled.

| Kind   | Meaning                                            | Payload essentials             |
| ------ | -------------------------------------------------- | ------------------------------ |
| `Link` | A URL the user wants to revisit                    | url, title, source app         |
| `Clip` | Arbitrary clipboard text (not a URL)               | text                           |
| `Shot` | A screenshot or image file                         | file path, dimensions, mime    |
| `File` | Any non-image file the user dropped onto overlay   | file path, mime                |
| `Note` | Free text typed into the capture modal             | text                           |

Detection rules:

- Clipboard text that matches a URL pattern is promoted to `Link`, otherwise `Clip`.
- Dragged files are split by mime: image mimes -> `Shot`, everything else -> `File`.

## Surfaces

The user-facing windows. Always referred to by these names.

The Inbox is the application's **main window**: future product screens (Settings, search, etc.) live as routes inside it, not as separate Tauri windows. The Composer and the Dock are the only other Tauri windows, and they exist as separate windows only because their UX requires properties the main window cannot provide (pop over any app; always-on-top widget). See ADR-0009.

### Dock

The small, frameless, always-on-top widget pinned to the bottom-left of the screen. Non-activating (never steals focus). Persistent for the life of the app session.

Roles:
- Visible drop target for files dragged from Finder. v1.0 ships file drops only; URL, text, and image drags from browsers are deferred (see ADR-0008) and reached through the clipboard shortcut instead.
- Click target that opens the [Composer](#composer).
- Surface for ambient feedback (e.g. a pulse when a capture lands).

### Composer

The Raycast/Alfred-style quick-entry window summoned by a global shortcut or by clicking the Dock. Focusable, centered, dismissed by ESC. Its single job is to accept a typed Note Capture and disappear.

The Composer is not a browser, manager, or inspector — it never lists past captures.

### Inbox

The Inbox view of the main window: a list of Captures that have not yet been Routed and have not been deleted, in reverse chronological order. Opened by a global shortcut or from the tray menu. Read-mostly: the user can star a Capture, route it, or delete it, but cannot edit its payload.

The Inbox uses a **split layout**:

- **List pane** (left): one row per Capture with a kind icon, single-line payload preview, relative timestamp, star toggle, and delete action. Selecting a row updates the detail pane.
- **Detail pane** (right): full payload of the selected Capture, with a kind-appropriate "Open" action — `Link` opens in the default browser, `File` / path-flavor `Shot` reveals in Finder, bytes-flavor `Shot` opens the blob, `Clip` and `Note` are read-only scrollable text.

The list is the navigational surface; the detail pane never lets the user mutate payload.

The Inbox view shares the main window with the [Archive](#archive) view; the user switches between the two via a top-of-window switcher.

### Archive

The Archive view of the main window: a list of Captures that have been Routed (and not deleted), grouped or filterable by [Destination](#destination). Reached via the Inbox/Archive switcher at the top of the main window.

Archive search is scoped to Routed Captures only and is a separate search surface from the Inbox search; the two indexes are not merged. From the Archive the user can re-route a Capture to a different Destination or un-route it back to the Inbox.

### Tray

The macOS menubar item the app installs at startup. Holds the same actions as the [Dock](#dock)'s right-click menu — Open Composer, Open Inbox, Quit — and serves as the small "the app is running" indicator visible at all times. The Tray is not a content surface; it never lists Captures.

## Capture lifecycle

Captures are append-only:

- **Created** by any of the trigger flows (Dock drop, Composer note, clipboard shortcut).
- **Starred / unstarred** by the user from the Inbox. A boolean flag on the Capture.
- **Routed** by the user from the Inbox by assigning a [Destination](#destination). A Routed Capture leaves the Inbox and is visible only in the [Archive](#archive). Routing is the user's signal that they have decided what to do with the Capture; it is distinct from deletion. Routing is reversible: the user may change a Capture's Destination (re-route) or send it back to the Inbox (un-route).
- **Deleted** by the user. Soft-delete (tombstone) so other processes reading the store can detect removals. Deletion means "I should not have captured this" and is not a Destination.

The payload and kind of a Capture never change after creation. If the wrong thing was captured, the user deletes it and captures again.

## Destination

A named category the user assigns to a Capture to mark it as triaged. Examples: `Todoist`, `Readwise`, `Reference`. Destinations are user-managed in Settings.

At this stage a Destination is only a label — assigning one does not move the Capture's payload anywhere outside the app. Future integrations may turn assignment into a real handoff (API call, file move, etc.), but that is out of scope for the first cut.

A Destination has an optional **color** chosen from a small preset palette. Color is decorative; it is rendered as a dot next to the Destination's name in the Archive list and in the Destination picker. Absent color renders no dot.

A Destination can be soft-deleted from Settings. Soft-deleted Destinations are hidden from the assignment picker but Captures already pointing at them keep the reference; the Archive surfaces them with a "(deleted)" indicator so the user can re-route or restore.

## Archive

The view of Captures that have been Routed. Distinct from the [Inbox](#inbox), which shows only un-Routed, non-deleted Captures. Archive search is scoped to Routed Captures only; Inbox search is scoped to Inbox Captures only — the two searches are intentionally separate.

