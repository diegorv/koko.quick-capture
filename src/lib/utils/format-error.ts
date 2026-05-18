// Normalize a thrown value (Tauri invoke errors arrive as plain
// strings; native JS errors as Error objects) into a UI-safe string.
// Used by every component that surfaces a backend error inline.

export function formatError(err: unknown): string {
  if (err instanceof Error) return err.message;
  if (typeof err === "string") return err;
  return String(err);
}
