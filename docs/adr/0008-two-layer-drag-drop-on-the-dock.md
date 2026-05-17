# Dock drag-drop is Tauri-native only (v1.0)

The Dock accepts drops via `tauri::Manager::on_window_event` / `WebviewWindow::on_drag_drop_event` exclusively — files dragged from Finder land as one Capture per path, with `source_path` (no blob copy). URL drags, plain-text drags, and image-bytes drags from browsers are explicitly **not handled** in v1.0 and deferred until the Tauri / wry stack exposes a custom drag-drop handler that lets a user-supplied closure pass-through to the WebView.

The originally-planned two-layer design (Tauri native + HTML5 `ondrop`, deduped) was rejected after verifying that Tauri 2.11.2 + wry 0.55.1 makes the two layers mutually exclusive on macOS: the default Tauri drag-drop handler is registered with a hard-coded `return true` in `tauri-runtime-wry`, which causes wry's `perform_drag_operation` to skip the AppKit super call, which prevents WKWebView from ever dispatching HTML5 `drop` events to the page. Disabling the Tauri handler inverts the problem: HTML5 events fire, but Finder file paths are stripped by browser sandbox policy (`File.path` is empty), and `source_path` semantics — central to ADR-0001 (SQLite + blob dir, references not copies) and CONTEXT.md's File / Shot definitions — cannot be preserved.

Files via Finder are the dominant capture motion in the daily workflow. URL drags have a viable workaround already shipped (`Ctrl+Alt+Cmd+C` after `Cmd+L; Cmd+C` in the browser), as do text and image drags through the clipboard path. The deferral is therefore cheap.

Revisit this ADR when Tauri exposes a custom drag-drop handler on `WebviewWindowBuilder` (tracked upstream), at which point the two-layer design becomes feasible without a fork.
