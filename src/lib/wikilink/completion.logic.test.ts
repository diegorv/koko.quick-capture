import { describe, it, expect } from "vitest";
import {
  buildWikilinkInsert,
  detectWikilinkContext,
  filterPeople,
} from "./completion.logic";

describe("detectWikilinkContext", () => {
  it("returns null when there is no [[ before the cursor", () => {
    expect(detectWikilinkContext("hello world", 5)).toBeNull();
  });

  it("matches an open [[query with the query slice", () => {
    const m = detectWikilinkContext("see [[di", 8);
    expect(m).not.toBeNull();
    expect(m!.query).toBe("di");
    expect(m!.from).toBe(6); // after the [[
    expect(m!.to).toBe(8);
  });

  it("matches an empty query right after [[", () => {
    const m = detectWikilinkContext("hey [[", 6);
    expect(m).not.toBeNull();
    expect(m!.query).toBe("");
    expect(m!.from).toBe(6);
    expect(m!.to).toBe(6);
  });

  it("returns null when the cursor has moved past the closing ]]", () => {
    // cursor immediately after the final `]]` of a closed link
    expect(detectWikilinkContext("[[Diego]]", 9)).toBeNull();
  });

  it("returns null when an earlier [[ on the same line is already closed", () => {
    // Cursor after a fully-closed wikilink. The earlier `[[` is
    // the lastIndexOf match, but the `]]` between it and the cursor
    // disqualifies the context.
    expect(detectWikilinkContext("[[old]] tail", 12)).toBeNull();
  });

  it("returns null when the open [[ is on a prior line", () => {
    expect(detectWikilinkContext("[[Diego\nseparate", 16)).toBeNull();
  });

  it("returns null when a pipe (display-text segment) is present", () => {
    expect(detectWikilinkContext("[[Diego|alias", 13)).toBeNull();
  });

  it("uses the line-local [[ when several appear on previous lines", () => {
    const doc = "first line [[old]]\nnext [[ne";
    // cursor at end of "ne"
    const m = detectWikilinkContext(doc, doc.length);
    expect(m).not.toBeNull();
    expect(m!.query).toBe("ne");
  });
});

describe("filterPeople", () => {
  const people = [
    { name: "Ana Beatriz", path: "/p/Ana Beatriz.md" },
    { name: "Diego", path: "/p/Diego.md" },
    { name: "Ralf", path: "/p/Ralf.md" },
  ];

  it("empty query keeps all entries in input order", () => {
    expect(filterPeople("", people).map((p) => p.name)).toEqual([
      "Ana Beatriz",
      "Diego",
      "Ralf",
    ]);
  });

  it("whitespace-only query also keeps all entries", () => {
    expect(filterPeople("   ", people).map((p) => p.name)).toEqual([
      "Ana Beatriz",
      "Diego",
      "Ralf",
    ]);
  });

  it("is case-insensitive substring", () => {
    expect(filterPeople("AN", people).map((p) => p.name)).toEqual([
      "Ana Beatriz",
    ]);
    expect(filterPeople("dieg", people).map((p) => p.name)).toEqual(["Diego"]);
  });

  it("matches substrings in the middle of the name", () => {
    expect(filterPeople("beatriz", people).map((p) => p.name)).toEqual([
      "Ana Beatriz",
    ]);
  });

  it("returns an empty array when nothing matches", () => {
    expect(filterPeople("xyz", people)).toEqual([]);
  });
});

describe("buildWikilinkInsert", () => {
  it("appends ]] when the cursor is not already followed by them", () => {
    const out = buildWikilinkInsert("Diego", "");
    expect(out.insert).toBe("Diego]]");
    expect(out.cursorOffset).toBe("Diego]]".length);
  });

  it("does not double-close when ]] already follow", () => {
    const out = buildWikilinkInsert("Diego", "]] rest");
    expect(out.insert).toBe("Diego");
    expect(out.cursorOffset).toBe("Diego".length);
  });

  it("treats a single ] (not the closing pair) as not-closed", () => {
    const out = buildWikilinkInsert("Diego", "] rest");
    expect(out.insert).toBe("Diego]]");
  });

  it("handles spaces in names without escaping", () => {
    const out = buildWikilinkInsert("Ana Beatriz", "");
    expect(out.insert).toBe("Ana Beatriz]]");
    expect(out.cursorOffset).toBe("Ana Beatriz]]".length);
  });
});
