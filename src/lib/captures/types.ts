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
}
