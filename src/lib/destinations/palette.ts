// Destination color palette. Stored as a key string (`color` column on
// destinations) so the palette can evolve in UI code without a DB
// migration. `null` color renders no swatch.
//
// Per ADR-0010: color is optional and decorative; it is rendered as a
// dot next to the Destination name in the picker and in the Archive
// list. The set is intentionally small (8 swatches) to keep the
// Settings UI a single row of clickable chips.

export const PALETTE_KEYS = [
  "red",
  "orange",
  "amber",
  "green",
  "teal",
  "blue",
  "purple",
  "gray",
] as const;

export type PaletteKey = (typeof PALETTE_KEYS)[number];

const HEX: Record<PaletteKey, string> = {
  red: "#ef4444",
  orange: "#f97316",
  amber: "#f59e0b",
  green: "#10b981",
  teal: "#14b8a6",
  blue: "#3b82f6",
  purple: "#8b5cf6",
  gray: "#6b7280",
};

/** Resolve a stored color key to its hex value. Unknown / null keys
 * resolve to `null` so the caller can skip rendering a dot. */
export function colorHex(key: string | null | undefined): string | null {
  if (!key) return null;
  return (HEX as Record<string, string>)[key] ?? null;
}

/** Narrow a free-form string to a known PaletteKey, or null. */
export function asPaletteKey(value: string | null | undefined): PaletteKey | null {
  if (!value) return null;
  return (PALETTE_KEYS as readonly string[]).includes(value)
    ? (value as PaletteKey)
    : null;
}
