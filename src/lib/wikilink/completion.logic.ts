// Pure logic for the [[ wikilink autocomplete. Trimmed from
// brain/src/lib/core/markdown-editor/extensions/wikilink/completion.logic.ts
// to the Composer's narrow scope: file-mode only (no heading /
// blockId modes), no alias resolution, case-insensitive substring
// match (Q10) instead of fuzzy. See ADR-0011 + CONTEXT.md.

export interface PersonEntry {
  name: string;
  path: string;
}

export interface WikilinkMatch {
  /** Document offset where the query slice begins (right after `[[`). */
  from: number;
  /** Document offset of the cursor (end of the query slice). */
  to: number;
  /** Substring between `[[` and the cursor. May be empty. */
  query: string;
}

/**
 * Detect whether the cursor sits inside an open `[[query` on the
 * current line. Returns null when:
 *  - there is no `[[` between the line start and the cursor
 *  - the `[[` is already closed by a `]]` between it and the cursor
 *    (Q13: no re-trigger inside closed links)
 *  - the wikilink has a `|` (display-text segment); the Composer
 *    never produces one but a hand-typed pipe ends suggestions
 */
export function detectWikilinkContext(
  docText: string,
  pos: number,
): WikilinkMatch | null {
  const lineStart = docText.lastIndexOf("\n", pos - 1) + 1;
  const before = docText.substring(lineStart, pos);
  const bracketIdx = before.lastIndexOf("[[");
  if (bracketIdx === -1) return null;
  const after = before.substring(bracketIdx + 2);
  if (after.includes("]]")) return null;
  if (after.includes("|")) return null;
  return {
    from: lineStart + bracketIdx + 2,
    to: pos,
    query: after,
  };
}

/**
 * Case-insensitive substring filter against `name`. Empty / whitespace
 * query keeps all entries. Result preserves input ordering — Rust's
 * `list_people` already returns alpha-case-insensitive sorted rows.
 */
export function filterPeople(
  query: string,
  people: PersonEntry[],
): PersonEntry[] {
  const trimmed = query.trim();
  if (trimmed.length === 0) return people;
  const lower = trimmed.toLowerCase();
  return people.filter((p) => p.name.toLowerCase().includes(lower));
}

/**
 * Build the insert text and end-cursor offset for picking `name` at
 * a wikilink context. Auto-closes `]]` only when the document does
 * not already have them right after the cursor (Q11).
 */
export function buildWikilinkInsert(
  name: string,
  textAfterCursor: string,
): { insert: string; cursorOffset: number } {
  const closingBrackets = textAfterCursor.startsWith("]]") ? "" : "]]";
  return {
    insert: name + closingBrackets,
    cursorOffset: name.length + closingBrackets.length,
  };
}
