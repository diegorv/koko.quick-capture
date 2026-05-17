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

The two user-facing windows. Always referred to by these names.

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

A standalone window listing recent Captures in reverse chronological order. Opened by a global shortcut or from the tray menu. Read-mostly: the user can star a Capture or delete it, but cannot edit its payload. The Inbox is the only place where past Captures are surfaced inside the app.

The Inbox uses a **split layout**:

- **List pane** (left): one row per Capture with a kind icon, single-line payload preview, relative timestamp, star toggle, and delete action. Selecting a row updates the detail pane.
- **Detail pane** (right): full payload of the selected Capture, with a kind-appropriate "Open" action — `Link` opens in the default browser, `File` / path-flavor `Shot` reveals in Finder, bytes-flavor `Shot` opens the blob, `Clip` and `Note` are read-only scrollable text.

The list is the navigational surface; the detail pane never lets the user mutate payload.

### Tray

The macOS menubar item the app installs at startup. Holds the same actions as the [Dock](#dock)'s right-click menu — Open Composer, Open Inbox, Quit — and serves as the small "the app is running" indicator visible at all times. The Tray is not a content surface; it never lists Captures.

## Capture lifecycle

Captures are append-only:

- **Created** by any of the trigger flows (Dock drop, Composer note, clipboard shortcut).
- **Starred / unstarred** by the user from the Inbox. A boolean flag on the Capture.
- **Deleted** by the user from the Inbox. Soft-delete (tombstone) so other processes reading the store can detect removals.

The payload and kind of a Capture never change after creation. If the wrong thing was captured, the user deletes it and captures again.

