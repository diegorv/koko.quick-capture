import { describe, it, expect } from "vitest";
import { parseMentionSegments } from "./parse-mention-segments";

describe("parseMentionSegments", () => {
  it("returns a single text segment when there are no mentions", () => {
    expect(parseMentionSegments("hello world")).toEqual([
      { kind: "text", value: "hello world" },
    ]);
  });

  it("returns an empty array on empty input", () => {
    expect(parseMentionSegments("")).toEqual([]);
  });

  it("splits a single mention from surrounding text", () => {
    expect(parseMentionSegments("see [[Diego]] tonight")).toEqual([
      { kind: "text", value: "see " },
      { kind: "mention", value: "Diego" },
      { kind: "text", value: " tonight" },
    ]);
  });

  it("handles a leading mention with no prefix text", () => {
    expect(parseMentionSegments("[[Diego]] arrived")).toEqual([
      { kind: "mention", value: "Diego" },
      { kind: "text", value: " arrived" },
    ]);
  });

  it("handles a trailing mention with no suffix text", () => {
    expect(parseMentionSegments("ping [[Diego]]")).toEqual([
      { kind: "text", value: "ping " },
      { kind: "mention", value: "Diego" },
    ]);
  });

  it("splits multiple mentions in order", () => {
    expect(parseMentionSegments("[[Ana]] then [[Diego]]")).toEqual([
      { kind: "mention", value: "Ana" },
      { kind: "text", value: " then " },
      { kind: "mention", value: "Diego" },
    ]);
  });

  it("strips alias and heading segments from the mention value", () => {
    expect(parseMentionSegments("[[Diego|d]] and [[Ana#notes]]")).toEqual([
      { kind: "mention", value: "Diego" },
      { kind: "text", value: " and " },
      { kind: "mention", value: "Ana" },
    ]);
  });

  it("preserves an unclosed [[ as plain text", () => {
    expect(parseMentionSegments("orphan [[ keeps going")).toEqual([
      { kind: "text", value: "orphan [[ keeps going" },
    ]);
  });

  it("treats empty brackets [[]] as plain text", () => {
    expect(parseMentionSegments("hello [[]] world")).toEqual([
      { kind: "text", value: "hello " },
      { kind: "text", value: "[[]]" },
      { kind: "text", value: " world" },
    ]);
  });

  it("keeps names with spaces intact", () => {
    expect(parseMentionSegments("[[Ana Beatriz]] notes")).toEqual([
      { kind: "mention", value: "Ana Beatriz" },
      { kind: "text", value: " notes" },
    ]);
  });
});
