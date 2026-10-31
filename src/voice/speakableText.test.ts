import { describe, expect, it } from "vitest";

import {
  CODE_PHRASE,
  SEGMENT_LIMIT,
  TABLE_PHRASE,
  speakableText,
  speechSegments,
} from "./speakableText";

describe("what is spoken of a comment", () => {
  it("VOM-FR-TRWN: gives the text of the Markdown", () => {
    const text = speakableText(
      "# Title\n\nSome **bold**, *italic*, and `code`.\n\n- one\n- two\n\n> quoted\n\n[a link](https://e.x) and ![pic](u.png)",
    );
    expect(text).toBe("Title\nSome bold, italic, and code.\none\ntwo\nquoted\na link and pic");
  });

  it("VOM-FR-TRWN: replaces a code block with a short phrase", () => {
    expect(speakableText("Before.\n```rust\nlet a = 1;\n```\nAfter.")).toBe(
      `Before.\n${CODE_PHRASE}\nAfter.`,
    );
  });

  it("VOM-FR-TRWN: replaces a table with a short phrase", () => {
    expect(speakableText("Look:\n| a | b |\n|---|---|\n| 1 | 2 |\nDone.")).toBe(
      `Look:\n${TABLE_PHRASE}\nDone.`,
    );
  });

  it("VOM-FR-TRWN: no tag stays in spoken text, also a nested one", () => {
    expect(speakableText("Before <b>bold</b> after.")).toBe("Before bold after.");
    const text = speakableText("<<script>script>alert(1)<</script>/script>");
    expect(text).not.toMatch(/[<>]/);
    expect(speakableText("Use a < b here.")).toBe("Use a  b here.");
  });

  it("VOM-FR-TRWN: a comment with no text has nothing to speak", () => {
    expect(speakableText("   \n\n")).toBe("");
  });
});

describe("speech segments", () => {
  it("VOM-FR-QOXX: splits at sentence ends, and the first segment is short", () => {
    const text = Array.from({ length: 40 }, (_, i) => `Sentence number ${i} is here.`).join(" ");
    const segments = speechSegments(text);
    expect(segments.length).toBeGreaterThan(1);
    expect(segments[0].length).toBeLessThanOrEqual(240);
    segments.forEach((s) => expect(s.endsWith(".")).toBe(true));
    expect(segments.join(" ")).toBe(text);
  });

  it("VOM-FR-QOXX: no segment reaches the limit, even with no sentence end", () => {
    const segments = speechSegments("a".repeat(SEGMENT_LIMIT * 2 + 10));
    segments.forEach((s) => expect(s.length).toBeLessThan(SEGMENT_LIMIT));
    expect(segments.join("")).toHaveLength(SEGMENT_LIMIT * 2 + 10);
  });

  it("VOM-FR-QOXX: empty text has no segments", () => {
    expect(speechSegments("")).toEqual([]);
  });
});
