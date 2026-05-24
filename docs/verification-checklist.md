# Manual verification checklist

Use this after a fresh `pnpm tauri dev` to confirm every path. The
checklist mirrors the [surfaces](surfaces.md) and is the gate the
project uses in place of macOS UI automation.

## Composer

- [ ] `Ctrl+Alt+Cmd+Space` opens an empty Composer over the
      previously-frontmost app.
- [ ] Typing + `Cmd+Enter` flashes an indigo ring, hides the window,
      and the new Note appears at the top of the Inbox.
- [ ] `Esc` cancels without saving.
- [ ] Reopening the Composer starts with an empty textarea (no leak
      from the previous session).
- [ ] Dragging from the padding around the textarea moves the window;
      clicking inside the textarea places the caret.

## Voice transcription

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

## Clipboard capture

- [ ] Copy plain text -> `Ctrl+Alt+Cmd+C` creates a `Clip`.
- [ ] Copy a URL -> same shortcut creates a `Link` (`url` populated;
      `raw_text` keeps the original copy).
- [ ] Copy an image (e.g. screenshot) -> creates a `Shot { blob_path }`
      with the image written under `~/Library/Application Support/com.koko.quick-capture/blobs/`.
- [ ] Copy a file in Finder (Cmd+C) -> creates a `Shot { source_path }`
      for an image mime or a `File { source_path }` otherwise.

## Inbox

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
- [ ] Status bar shows `N captures . last Xm ago . K new`. The "new"
      count hides when zero.
- [ ] `R` routes the selected capture to a destination.

## Dock

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

## Drag-drop onto the Dock

- [ ] Drag a PDF from Finder onto the Dock disc -> disc shows a violet
      ring while hovering -> drop creates a `File` capture; row appears
      in the Inbox with the original filename.
- [ ] Drag a PNG from Finder -> drop creates a `Shot { source_path }`;
      the detail pane shows an inline preview.
- [ ] Drag multiple files at once -> one capture per file, all
      prepended.
- [ ] Drag-cancel (drop outside the disc) clears the ring without
      saving.

> URLs from a browser address bar, plain selected text, and image
> bytes from a browser are *not* accepted on the Dock by design - see
> ADR-0008. Use the clipboard shortcut for those.

## Tray

- [ ] Brain-circuit icon visible in the menubar; recolours correctly
      with the menubar theme.
- [ ] Menu shows the three items with the right Lucide glyph + the
      accelerator hint on the right.
- [ ] Each item invokes the same path as the global shortcut /
      Composer click.

## Settings

- [ ] Settings accessible from tray menu.
- [ ] Transcription section shows model status, language, mic, system audio toggle.
- [ ] Destinations section shows configured routing targets.

## Activation policy (Cmd+Tab)

- [ ] At idle: app does not show up in Cmd+Tab and has no system Dock
      icon.
- [ ] Opening the Inbox: app appears in Cmd+Tab and the system Dock
      icon shows up.
- [ ] Closing the Inbox: both disappear.

## Window position persistence

- [ ] Move the Inbox window, close it, reopen -> it lands at the same
      position.
- [ ] Force-quit while the Inbox is hidden -> on next launch the Inbox
      is not automatically visible (the plugin only persists size +
      position, never visibility).
