# Capture kinds

Every row in the Inbox is one of six kinds. The active capture path
determines which kind is produced.

| Kind          | Payload                                          | Created by                                                                                    |
|---------------|--------------------------------------------------|-----------------------------------------------------------------------------------------------|
| Note          | `{ text }`                                       | Composer save (`Cmd+Enter`).                                                                  |
| Clip          | `{ text }`                                       | Clipboard shortcut (`Ctrl+Alt+Cmd+C`) when the clipboard holds plain text that is not a URL.  |
| Link          | `{ url, raw_text, title? }`                      | Clipboard shortcut when the text parses as a URL.                                             |
| Shot          | `{ source_path \| blob_path, mime, width?, height? }` | Clipboard shortcut with image bytes (`blob_path`), or drag-drop / clipboard with an image file (`source_path`). |
| File          | `{ source_path, mime, original_name? }`          | Drag-drop of a non-image file onto the Dock, or clipboard with non-image file paths.          |
| Transcription | `{ text, audio_path, duration_secs }`            | Composer mic button. Records mic (+ optional system audio), transcribes locally via whisper-rs (Metal GPU). |

## Source modules

- Kind detection: `src-tauri/src/kind_detect/`
- Recording pipeline: `src-tauri/src/recording/`
- Whisper inference + hallucination filtering: `src-tauri/src/transcription/`

## Global shortcuts

Registered via `tauri-plugin-global-shortcut`. Fire from any frontmost
app - no need to focus the quick-capture window first.

| Shortcut              | Action                                                                                   |
|-----------------------|------------------------------------------------------------------------------------------|
| `Ctrl+Alt+Cmd+Space`  | Open / focus the Composer popover.                                                       |
| `Ctrl+Alt+Cmd+C`      | Read the macOS clipboard, decide the kind, save one capture (or N for a multi-file paste).|
| `Ctrl+Alt+Cmd+I`      | Open / focus the Inbox window.                                                           |
