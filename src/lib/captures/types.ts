// Shared shape of a Capture row as it crosses the Tauri IPC boundary.
// Mirrors the Rust `store::Capture` serde encoding (see
// src-tauri/src/store/mod.rs). Keep these in lockstep: any field added
// on the Rust side must be added here or `pnpm check` will start
// flagging the inbox UI.

export type CaptureKind = "Link" | "Clip" | "Shot" | "File" | "Note";

export interface Capture {
  id: string;
  kind: CaptureKind;
  created_at: string;
  payload: Record<string, unknown>;
  source_app: string | null;
  starred: boolean;
  deleted_at: string | null;
  /** ISO timestamp set on first user interaction with the capture in
   * the Inbox. `null` means unread; the row stays unread until the
   * user clicks it or selects it via the keyboard. */
  read_at: string | null;
  /** Window title of the source app at capture time. Active tab
   * title for browsers; window title bar text for other apps when
   * resolvable. `null` when nothing was captured. */
  source_title: string | null;
  /** Active URL of the source app at capture time. Populated only
   * for known browsers (Chrome / Safari). `null` everywhere else. */
  source_url: string | null;
}
