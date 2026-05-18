// Split free text into renderable segments around `[[Name]]` tokens.
// Used by the Inbox/Archive detail pane to turn Note text into a mix
// of plain text spans and clickable mention chips. The parsing rules
// mirror the Rust `extract_mentions` (alias + heading stripped, empty
// names degrade to plain text) so the JS render and the Rust index
// agree on what counts as a mention.

export type MentionSegment =
  | { kind: "text"; value: string }
  | { kind: "mention"; value: string };

export function parseMentionSegments(text: string): MentionSegment[] {
  const segments: MentionSegment[] = [];
  let i = 0;
  while (i < text.length) {
    const openIdx = text.indexOf("[[", i);
    if (openIdx === -1) {
      if (i < text.length) {
        segments.push({ kind: "text", value: text.slice(i) });
      }
      break;
    }
    const closeIdx = text.indexOf("]]", openIdx + 2);
    if (closeIdx === -1) {
      // Unclosed token — flush the rest as plain text.
      segments.push({ kind: "text", value: text.slice(i) });
      break;
    }
    if (openIdx > i) {
      segments.push({ kind: "text", value: text.slice(i, openIdx) });
    }
    const inner = text.slice(openIdx + 2, closeIdx);
    const base = inner.split("|")[0].split("#")[0].trim();
    if (base.length === 0) {
      // `[[]]` is not a wikilink — keep the raw chars.
      segments.push({
        kind: "text",
        value: text.slice(openIdx, closeIdx + 2),
      });
    } else {
      segments.push({ kind: "mention", value: base });
    }
    i = closeIdx + 2;
  }
  return segments;
}
