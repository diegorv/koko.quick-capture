# quick-capture

> [!WARNING]
> **Early stage project.** APIs, schemas, and behavior may change without
> notice. Not ready for production use. Expect bugs, breaking changes,
> and missing features.

Frictionless macOS capture inbox. Tauri 2 + SvelteKit + Rust.

The app lives in the macOS menubar as an Accessory app — no system Dock
icon by default — and only surfaces in Cmd+Tab while the Inbox window
is open. Three always-on entry points: a Composer popover, a small
on-screen Dock widget, and a Tray menubar item. Captures land in a
single chronological Inbox with per-item read state.

See `CONTEXT.md` for the domain glossary, `docs/adr/` for architectural
decisions, and `.scratch/` for the active PRDs and handoff docs.

---

## Surfaces

The product has four surfaces. Every entry point routes through one of
them.

### Composer

A small borderless popover (600x240) for free-text notes and voice
recordings. Lives hidden until summoned.

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

### Inbox

The main window. Split-pane: list of captures on the left,
detail of the selected capture on the right. Hidden until summoned.

- **Open**: `Ctrl+Alt+Cmd+I`, tray menu "Open Inbox", or Dock
  right-click → "Open Inbox".
- **Close**: red traffic-light button, `Esc`, or `Cmd+W`. Window
  hides (does not destroy).
- **Drag**: drag from the 28px strip below the traffic-light buttons.

Status bar at the bottom shows: total captures, time since the newest
capture, and the current unread count (hidden when zero).

### Dock

A 96x96 always-on-top, non-activating widget pinned to the bottom-left
of the primary monitor. The visible disc is 80x80 violet; the 8px
ring of slack lets the unread badge overflow without clipping.

- **Click**: opens the Composer.
- **Right-click**: popup menu (Open Composer, Open Inbox, Quit).
- **Drag files onto it**: each dropped file becomes a Capture
  (image-mime → Shot, anything else → File). The disc shows a
  violet ring while a drag is hovering.
- **Pulse**: one-shot animation on every successful save.
- **Badge**: red dot with the live unread count (rows whose
  `read_at` is still NULL). Capped at "99+". Auto-hides when zero.
- **Auto-hide**: disappears when any frontmost app enters fullscreen
  and reappears on exit.

### Tray

A macOS menubar item with the Lucide brain-circuit glyph. Template
icon — macOS recolours it for the menubar theme automatically. The
menu items themselves have the right per-item glyph for the current
appearance, picked once at app launch.

Items: **Open Composer** (`Ctrl+Alt+Cmd+Space`), **Open Inbox**
(`Ctrl+Alt+Cmd+I`), **Quit** (`Cmd+Q`).

---

## Capture kinds

Every row in the Inbox is one of six kinds. The active capture path
determines which kind is produced.

| Kind | Payload                       | Created by                                                                                |
|------|-------------------------------|-------------------------------------------------------------------------------------------|
| Note | `{ text }`                    | Composer save (`Cmd+Enter`).                                                              |
| Clip | `{ text }`                    | Clipboard shortcut (`Ctrl+Alt+Cmd+C`) when the clipboard holds plain text that is not a URL. |
| Link | `{ url, raw_text, title? }`   | Clipboard shortcut when the text parses as a URL.                                         |
| Shot | `{ source_path | blob_path, mime, width?, height? }` | Clipboard shortcut with image bytes (`blob_path`), or drag-drop / clipboard with an image file (`source_path`). |
| File | `{ source_path, mime, original_name? }` | Drag-drop of a non-image file onto the Dock, or clipboard with non-image file paths.       |
| Transcription | `{ text, audio_path, duration_secs }` | Composer mic button. Records mic (+ optional system audio), transcribes locally via whisper-rs (Metal GPU). |

The kind detection lives in `src-tauri/src/kind_detect/`. Voice
transcription lives in `src-tauri/src/recording/` (pipeline) and
`src-tauri/src/transcription/` (whisper inference + hallucination
filtering).

---

## Global shortcuts

Registered via `tauri-plugin-global-shortcut`. Fire from any frontmost
app — no need to focus the quick-capture window first.

| Shortcut             | Action                                                                                  |
|----------------------|-----------------------------------------------------------------------------------------|
| `Ctrl+Alt+Cmd+Space` | Open / focus the Composer popover.                                                      |
| `Ctrl+Alt+Cmd+C`     | Read the macOS clipboard, decide the kind, save one capture (or N for a multi-file paste). |
| `Ctrl+Alt+Cmd+I`     | Open / focus the Inbox window.                                                          |

---

## Inbox keyboard

The listbox grabs window focus on cold open, on every row click, and on
window-focus events, so the shortcuts below work without an extra Tab.

| Key                    | Action                                                                                |
|------------------------|---------------------------------------------------------------------------------------|
| `↑` / `↓`              | Move selection. First arrow press also marks the row as read.                          |
| `Enter`                | Trigger the row's "Open" action (open URL in browser, reveal in Finder, etc.).        |
| `S`                    | Toggle star on the selected row.                                                       |
| `Cmd+Delete` / `Cmd+Backspace` | Soft-delete the selected row.                                                  |
| `Esc` / `Cmd+W`        | Hide the Inbox window.                                                                |

Mouse equivalents: click a row to select + mark read, click ★/☆ in the
list or in the detail header to toggle star, click × to delete.

---

## Read state

New captures land as unread. Unread rows show a violet dot in the
left gutter and bold payload text; the Dock badge counts them.

A row flips to read on the first user interaction — click, arrow-key
selection, or anything that calls `mark_read`. The flip is idempotent;
re-selecting an already-read row is a no-op.

The schema migration that introduced `read_at` backfills every
pre-existing capture as read so the dot indicator does not flood the
inbox on first launch under the new model.

---

## Manual verification checklist

Use this after a fresh `pnpm tauri dev` to confirm every path. The
checklist mirrors the surfaces section above and is the gate the
project uses in place of macOS UI automation.

### Composer
- [ ] `Ctrl+Alt+Cmd+Space` opens an empty Composer over the
      previously-frontmost app.
- [ ] Typing + `Cmd+Enter` flashes an indigo ring, hides the window,
      and the new Note appears at the top of the Inbox.
- [ ] `Esc` cancels without saving.
- [ ] Reopening the Composer starts with an empty textarea (no leak
      from the previous session).
- [ ] Dragging from the padding around the textarea moves the window;
      clicking inside the textarea places the caret.

### Voice transcription
- [ ] Click the mic button in the Composer. On first use, the whisper
      model downloads (~547 MB) with a progress bar in Settings.
- [ ] Recording shows a pulsing red dot, elapsed timer, and Stop
      button. The text editor is hidden during recording.
- [ ] After 30 seconds of recording, a partial transcript appears
      below the timer.
- [ ] Click Stop. The Composer dismisses and a Transcription capture
      appears in the Inbox with the mic icon.
- [ ] The Inbox detail pane shows an audio player, duration, and
      the transcript text.
- [ ] Audio playback works from the detail pane.
- [ ] Settings > Transcription shows the model status, language
      picker (PT/EN), mic device selector, and system audio toggle.
- [ ] Changing the language in Settings affects subsequent recordings.

### Clipboard capture
- [ ] Copy plain text → `Ctrl+Alt+Cmd+C` creates a `Clip`.
- [ ] Copy a URL → same shortcut creates a `Link` (`url` populated;
      `raw_text` keeps the original copy).
- [ ] Copy an image (e.g. screenshot) → creates a `Shot { blob_path }`
      with the image written under `~/Library/Application Support/com.koko.quick-capture/blobs/`.
- [ ] Copy a file in Finder (Cmd+C) → creates a `Shot { source_path }`
      for an image mime or a `File { source_path }` otherwise.

### Inbox
- [ ] `Ctrl+Alt+Cmd+I` opens the window; arrow keys work immediately
      without a click.
- [ ] Each kind renders the correct row icon and a sensible preview
      string.
- [ ] Detail pane updates on every selection change; "Open" button
      label matches the kind.
- [ ] `Open in Browser` on a `Link` actually launches the default
      browser at the URL.
- [ ] `Reveal in Finder` on a `File` / path-`Shot` opens Finder with
      the file highlighted.
- [ ] `Shot { blob_path }` shows the image inline and the action
      button reads "Open Image".
- [ ] Scrolling past row 50 quietly loads the next page.
- [ ] Saving from the Composer while the Inbox is open prepends the
      new row live + bumps the status bar total + bumps the Dock
      unread badge.
- [ ] Status bar shows `N captures · last Xm ago · K new`. The "new"
      count hides when zero.

### Dock
- [ ] Disc renders at bottom-left of the primary monitor with a 16px
      margin from both edges.
- [ ] Click opens the Composer; the Dock does not steal focus from
      whatever app you were in.
- [ ] Right-click opens the popup menu with the three items.
- [ ] On every successful save the disc pulses once.
- [ ] Unread badge appears with the live count after a save, decrements
      on row interaction in the Inbox, hides at zero.
- [ ] Entering fullscreen in any app (Cmd+Ctrl+F in Safari, for
      example) hides the Dock. Exiting fullscreen shows it again.

### Drag-drop onto the Dock (the one path you said you have not tested yet)
- [ ] Drag a PDF from Finder onto the Dock disc → disc shows a violet
      ring while hovering → drop creates a `File` capture; row appears
      in the Inbox with the original filename.
- [ ] Drag a PNG from Finder → drop creates a `Shot { source_path }`;
      the detail pane shows an inline preview.
- [ ] Drag multiple files at once → one capture per file, all
      prepended.
- [ ] Drag-cancel (drop outside the disc) clears the ring without
      saving.

> URLs from a browser address bar, plain selected text, and image
> bytes from a browser are *not* accepted on the Dock by design — see
> ADR-0008. Use the clipboard shortcut for those.

### Tray
- [ ] Brain-circuit icon visible in the menubar; recolours correctly
      with the menubar theme.
- [ ] Menu shows the three items with the right Lucide glyph + the
      accelerator hint on the right.
- [ ] Each item invokes the same path as the global shortcut /
      Composer click.

### Activation policy (Cmd+Tab)
- [ ] At idle: app does not show up in Cmd+Tab and has no system Dock
      icon.
- [ ] Opening the Inbox: app appears in Cmd+Tab and the system Dock
      icon shows up.
- [ ] Closing the Inbox: both disappear.

### Window position persistence
- [ ] Move the Inbox window, close it, reopen → it lands at the same
      position.
- [ ] Force-quit while the Inbox is hidden → on next launch the Inbox
      is *not* automatically visible (the plugin only persists size +
      position, never visibility).

---

## Requirements

- macOS (Apple Silicon or Intel — only macOS is supported).
- Rust toolchain (`rustup`).
- Node.js + pnpm 10.

## Dev

```sh
pnpm install
pnpm tauri dev
```

Vite serves the frontend on `localhost:1420`; if the port is held by
a previous dev process, kill it (`pkill -9 -f quick-capture`) or run
on a different port.

## Build

```sh
pnpm tauri build
```

Produces `src-tauri/target/release/bundle/macos/quick-capture.app`
and a `.dmg` under the same directory.

## Checks

```sh
pnpm test            # Vitest (Svelte components + routes)
pnpm check           # svelte-check (types)
cargo test --manifest-path src-tauri/Cargo.toml      # Rust unit + integration
cargo check --manifest-path src-tauri/Cargo.toml     # Rust typecheck
```

Every slice must leave all four green before commit (see ADR-0006).

## Regenerating the app icon

```sh
cargo run --manifest-path src-tauri/Cargo.toml --bin gen_icon
pnpm tauri icon src-tauri/icons/source-1024.png
```

The first command renders the violet brain-circuit squircle at
1024x1024 from an inline SVG. The second repopulates every bundle
icon (`icon.icns`, `icon.ico`, sized PNGs, iOS / Android variants).

## Dev verification

The `dev_list` binary prints recent captures directly from the SQLite
store; useful for sanity-checking the write path without opening the
Inbox window.

```sh
cd src-tauri
cargo run --bin dev_list -- --limit 5
```

Each line has four columns separated by two spaces:

```
<short-ulid>  <kind>  <created_at>  <payload preview>
```

- `<short-ulid>` — first 8 chars of the Capture's ULID.
- `<kind>` — one of `Link`, `Clip`, `Shot`, `File`, `Note`, `Transcription`.
- `<created_at>` — ISO-8601 UTC timestamp.
- `<payload preview>` — single-line preview of the payload, truncated
  with `...` and `\n` escapes if it overruns 60 chars.

Flags:

- `--limit N` — number of rows to print (default 20).
- `--db <path>` — read from a specific SQLite file instead of the
  default location.
