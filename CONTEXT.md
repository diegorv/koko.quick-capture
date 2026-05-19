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

A named category the user assigns to a Capture to mark it as triaged. Examples: `Todoist`, `Readwise`, `Reference`, `Personal Brain`. Destinations are user-managed in Settings.

Every Destination has a **kind** drawn from a closed set (see [Destination kinds](#destination-kinds)). The kind decides what assigning a Destination *does* beyond the routing-state mutation: `label` is metadata-only, `kokobrain` additionally fires a `kokobrain://capture` deep link. The Routed lifecycle (Inbox → Archive, re-route, un-route) is identical for every kind.

A Destination has an optional **color** chosen from a small preset palette. Color is decorative; it is rendered as a dot next to the Destination's name in the Archive list and in the Destination picker. Absent color renders no dot.

A Destination can be soft-deleted from Settings. Soft-deleted Destinations are hidden from the assignment picker but Captures already pointing at them keep the reference; the Archive surfaces them with a "(deleted)" indicator so the user can re-route or restore.

## Destination kinds

Closed set. Adding a kind is an explicit change everywhere kinds are handled (ADR-0012).

| Kind        | Routing side-effect                                                                                              | Config                          |
| ----------- | ---------------------------------------------------------------------------------------------------------------- | ------------------------------- |
| `label`     | None beyond setting `destination_id` + `routed_at` on the Capture.                                               | None (always `null`).           |
| `kokobrain` | Same as `label`, plus dispatches a typed `kokobrain://capture?v=2&kind=...&vault=...` deep link via the OS (see [Kokobrain capture URI](#kokobrain-capture-uri)). | `{ "vault": string }` required. |

KokoBrain destinations only accept Capture kinds whose payload can be rendered as text: `Note` and `Clip` send their raw `text`, `Link` sends `url` (with an optional `title` resolved from the captured window title or `payload.title`). `Shot` and `File` cannot be routed to a KokoBrain destination — the picker disables those rows with an inline reason. The handoff is fire-and-forget: quick-capture does not know whether the brain side wrote the note successfully, so a missing vault or a crashed brain leaves an optimistically-Routed Capture without a corresponding note. The user resolves divergence by un-routing the Capture or fixing the destination's vault.

## Kokobrain capture URI

The wire contract between a `kokobrain` Destination and the brain app. Defined by ADR-0013; built by `src-tauri/src/kokobrain/mod.rs::build_capture_uri`. Schema version is **v2**; the brain side rejects URIs without `v=2` after its own v2 release.

Every URI is of the form `kokobrain://capture?v=2&kind=<k>&vault=<v>&...&captured_at=<iso>[&tags=<csv>]`. Parameter order is stable for log readability but the brain parser treats the query as unordered.

Always present:

| Param         | Source                                                                                | Notes                                                                       |
| ------------- | ------------------------------------------------------------------------------------- | --------------------------------------------------------------------------- |
| `v`           | Literal `2`.                                                                          | Schema version. Bump on any breaking change.                                |
| `kind`        | `Capture.kind` lower-cased (`note`, `clip`, `link`, `shot`, `file`).                  | Brain dispatches per kind.                                                  |
| `vault`       | Destination config `vault` string.                                                    | Required and validated non-blank at config-parse time.                      |
| `captured_at` | `Capture.created_at` (ISO 8601).                                                      | Brain may use this for the note's `created` frontmatter or filename prefix. |
| `tags`        | Kebab-cased destination name plus user-configured tags, deduped, in original order.   | Omitted entirely when the merged list is empty.                             |

Per-kind required fields:

| `kind`         | Required params         | Notes                                                                                             |
| -------------- | ----------------------- | ------------------------------------------------------------------------------------------------- |
| `note`, `clip` | `text`                  | Raw payload text. The brain may wrap it in a Source footer if `source_url` is also present.       |
| `link`         | `url` (+ optional `title`) | `title` resolves through `source_title` -> `payload.title`; omitted when neither is non-blank.    |
| `shot`, `file` | `path` (when emitted)   | Not emitted by quick-capture today — `build_capture_uri` returns `UnsupportedKind` for these.     |

Source context (optional, emitted only when non-blank):

| Param          | Source                | Emitted for                                                                                                          |
| -------------- | --------------------- | -------------------------------------------------------------------------------------------------------------------- |
| `source_app`   | `Capture.source_app`  | All kinds. macOS bundle id of the foreground app at capture time (e.g. `com.google.Chrome`).                         |
| `source_title` | `Capture.source_title`| `Note` and `Clip` only. **Stripped for `Link`** because it duplicates the canonical `title` param.                   |
| `source_url`   | `Capture.source_url`  | `Note` and `Clip` only. **Stripped for `Link`** because it duplicates the canonical `url` param.                     |

Whitespace-only values for any optional field are treated as absent. The URI never carries an empty-string param.

## Wikilink

A `[[token]]` reference embedded in the text payload of a Capture (in practice, today, a Note Capture's `text`). The token names a file the user expects to resolve **elsewhere** — typically inside a companion notes app like brain — not inside QC. QC neither resolves wikilinks nor follows them; it only produces them. The Composer offers autocompletion for `[[` so the user can drop a recognized token without retyping it, but the saved Capture is still plain text and the token is just text inside it.

A Wikilink's surface form is `[[name]]`. Forms with display aliases (`[[name|alias]]`) and intra-document anchors (`[[name#heading]]`) are not produced by the Composer in this release; if they appear in payload they were typed by hand.

## Wikilink source folder

A user-configured absolute filesystem path whose top-level `.md` filenames feed the Composer's `[[` autocomplete. The folder is the **autocomplete source** only — its contents are listed by name, never read. The folder is the user's choice (e.g. a `_people` folder in their notes vault); QC has no opinion about what the names represent. The setting is single-valued and optional: when unset, `[[` typing produces no autocomplete and the Composer behaves like a plain text field; when set but empty/missing on disk, the autocomplete popup shows a "no entries" message instead of suggestions.

The source folder is read flat: subdirectories and non-`.md` files are ignored. Listing happens on demand each time `[[` triggers autocompletion; QC does not cache or watch the folder.

## Archive

The view of Captures that have been Routed. Distinct from the [Inbox](#inbox), which shows only un-Routed, non-deleted Captures. Archive search is scoped to Routed Captures only; Inbox search is scoped to Inbox Captures only — the two searches are intentionally separate.

