# Surfaces

The product has five surfaces. Every entry point routes through one of
them.

## Composer

A small borderless popover (600x240) for free-text notes. Lives hidden
until summoned. Voice recording opens a separate window (600x280) with
its own layout.

- **Open**: `Ctrl+Alt+Cmd+Space`, click the Dock disc, tray menu
  "Open Composer".
- **Save**: `Cmd+Enter`. Indigo confirmation ring flashes for ~180ms,
  then the window hides itself.
- **Record**: click the mic button to start recording. A pulsing red
  dot, timer, and partial transcript appear. Click Stop to transcribe
  and save as a Transcription capture. The whisper model downloads
  automatically on first use (~547 MB).
- **Cancel**: `Esc` or `Cmd+W`. Window hides without saving. If
  recording, stops and saves first.
- **Drag**: drag from any non-textarea padding to reposition.

## Inbox

The main window. Split-pane: list of captures on the left,
detail of the selected capture on the right. Hidden until summoned.

- **Open**: `Ctrl+Alt+Cmd+I`, tray menu "Open Inbox", or Dock
  right-click -> "Open Inbox".
- **Close**: red traffic-light button, `Esc`, or `Cmd+W`. Window
  hides (does not destroy).
- **Drag**: drag from the 28px strip below the traffic-light buttons.

Status bar at the bottom shows: total captures, time since the newest
capture, and the current unread count (hidden when zero).

### Keyboard

The listbox grabs window focus on cold open, on every row click, and on
window-focus events, so the shortcuts below work without an extra Tab.

| Key                             | Action                                                                         |
|---------------------------------|--------------------------------------------------------------------------------|
| `Up` / `Down`                   | Move selection. First arrow press also marks the row as read.                  |
| `Enter`                         | Trigger the row's "Open" action (open URL in browser, reveal in Finder, etc.). |
| `S`                             | Toggle star on the selected row.                                               |
| `R`                             | Route the selected capture to a destination (see ADR-0010).                    |
| `Shift+R`                       | Route with options.                                                            |
| `Cmd+Delete` / `Cmd+Backspace`  | Soft-delete the selected row.                                                  |
| `Esc` / `Cmd+W`                 | Hide the Inbox window.                                                         |

Mouse equivalents: click a row to select + mark read, click the star in the
list or in the detail header to toggle star, click x to delete.

### Read state

New captures land as unread. Unread rows show a violet dot in the
left gutter and bold payload text; the Dock badge counts them.

A row flips to read on the first user interaction - click, arrow-key
selection, or anything that calls `mark_read`. The flip is idempotent;
re-selecting an already-read row is a no-op.

The schema migration that introduced `read_at` backfills every
pre-existing capture as read so the dot indicator does not flood the
inbox on first launch under the new model.

## Dock

A 96x96 always-on-top, non-activating widget pinned to the bottom-left
of the primary monitor (16px margin from edges). The visible disc is
80x80 violet; the 8px ring of slack lets the unread badge overflow
without clipping.

- **Click**: opens the Composer.
- **Right-click**: popup menu (Open Composer, Open Inbox, Quit).
- **Drag files onto it**: each dropped file becomes a Capture
  (image-mime -> Shot, anything else -> File). The disc shows a
  violet ring while a drag is hovering.
- **Pulse**: one-shot animation on every successful save.
- **Badge**: red dot with the live unread count (rows whose
  `read_at` is still NULL). Capped at "99+". Auto-hides when zero.
- **Auto-hide**: disappears when any frontmost app enters fullscreen
  and reappears on exit.

## Tray

A macOS menubar item with the Lucide brain-circuit glyph. Template
icon - macOS recolours it for the menubar theme automatically. The
menu items themselves have the right per-item glyph for the current
appearance, picked once at app launch.

Items: **Open Composer** (`Ctrl+Alt+Cmd+Space`), **Open Inbox**
(`Ctrl+Alt+Cmd+I`), **Quit** (`Cmd+Q`).

## Settings

Accessible from the tray menu. Sections:

- **Transcription** - model status/download, language picker (PT/EN),
  mic device selector, system audio toggle.
- **Updates** - app version and update check.
- **Destinations** - configure routing targets for captures (see ADR-0010, ADR-0012).

## Activation policy (Cmd+Tab)

The app runs as a macOS Accessory (see ADR-0009):

- At idle: no Cmd+Tab entry, no system Dock icon.
- Inbox open: appears in Cmd+Tab + system Dock.
- Inbox closed: both disappear.

## Window position persistence

- Move the Inbox window, close it, reopen -> it lands at the same
  position.
- Force-quit while the Inbox is hidden -> on next launch the Inbox
  is not automatically visible (the plugin only persists size +
  position, never visibility).
