import { describe, expect, it } from "vitest";
import { Editor } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import { docText, offsetToPos, rangeToPositions } from "./findHighlight";

/**
 * The text↔position bridge the WYSIWYG surface's matching stands on. An
 * off-by-one here highlights the wrong words and makes Replace rewrite the
 * wrong span, so it is pinned against real ProseMirror documents rather than a
 * hand-built stand-in.
 */
function docFor(html: string) {
  const editor = new Editor({ extensions: [StarterKit], content: html });
  const doc = editor.state.doc;
  return { editor, doc };
}

/** Round-trip: the text a range resolves to, read back out of the document. */
function textAt(doc: ReturnType<typeof docFor>["doc"], from: number, to: number) {
  return doc.textBetween(from, to);
}

describe("docText (EFR-FR-DDUX)", () => {
  it("projects a single paragraph to its text", () => {
    const { doc } = docFor("<p>hello world</p>");
    expect(docText(doc).text).toBe("hello world");
  });

  it("separates block boundaries so a query cannot match across them", () => {
    // Without the separator the projection would read "foobar" and a search for
    // "oba" would match text that is not adjacent on screen.
    const { doc } = docFor("<p>foo</p><p>bar</p>");
    expect(docText(doc).text).toBe("foo\nbar");
  });

  it("walks headings, lists and blockquotes in document order", () => {
    const { doc } = docFor(
      "<h1>Title</h1><ul><li><p>one</p></li><li><p>two</p></li></ul><blockquote><p>quoted</p></blockquote>",
    );
    expect(docText(doc).text).toBe("Title\none\ntwo\nquoted");
  });

  it("keeps marked-up runs contiguous", () => {
    // Bold splits the paragraph into several text nodes; the projection must
    // still read as one continuous string so a query can span the boundary.
    const { doc } = docFor("<p>a <strong>bold</strong> word</p>");
    const index = docText(doc);
    expect(index.text).toBe("a bold word");
    expect(index.segments.length).toBeGreaterThan(1);
  });

  it("is empty for an empty document", () => {
    const { doc } = docFor("<p></p>");
    expect(docText(doc).text).toBe("");
  });
});

describe("offsetToPos / rangeToPositions", () => {
  it("resolves every offset of a plain paragraph back to its own character", () => {
    const { doc } = docFor("<p>hello world</p>");
    const index = docText(doc);
    for (let i = 0; i < index.text.length; i += 1) {
      const pos = offsetToPos(index, i);
      expect(pos).not.toBeNull();
      expect(textAt(doc, pos!, pos! + 1)).toBe(index.text[i]);
    }
  });

  it("resolves a range to exactly the text that matched", () => {
    const { doc } = docFor("<p>the quick brown fox</p>");
    const index = docText(doc);
    const start = index.text.indexOf("brown");
    const range = rangeToPositions(index, start, start + 5)!;
    expect(range).not.toBeNull();
    expect(textAt(doc, range.from, range.to)).toBe("brown");
  });

  it("resolves a range that spans a mark boundary", () => {
    const { doc } = docFor("<p>a <strong>bold</strong> word</p>");
    const index = docText(doc);
    const start = index.text.indexOf("bold word");
    const range = rangeToPositions(index, start, start + "bold word".length)!;
    expect(textAt(doc, range.from, range.to)).toBe("bold word");
  });

  it("resolves matches in the second block, past the separator", () => {
    const { doc } = docFor("<p>foo</p><p>bar baz</p>");
    const index = docText(doc);
    const start = index.text.indexOf("baz");
    const range = rangeToPositions(index, start, start + 3)!;
    expect(textAt(doc, range.from, range.to)).toBe("baz");
  });

  it("returns null for an offset naming a block separator", () => {
    const { doc } = docFor("<p>foo</p><p>bar</p>");
    const index = docText(doc);
    expect(index.text[3]).toBe("\n");
    expect(offsetToPos(index, 3)).toBeNull();
  });

  it("rejects a range that would cross a block boundary", () => {
    // Both ends resolve, but the span between them is not contiguous in the
    // document — highlighting it would cover text that never matched.
    const { doc } = docFor("<p>foo</p><p>bar</p>");
    const index = docText(doc);
    expect(rangeToPositions(index, 2, 5)).toBeNull();
  });

  it("rejects an empty or inverted range", () => {
    const { doc } = docFor("<p>hello</p>");
    const index = docText(doc);
    expect(rangeToPositions(index, 2, 2)).toBeNull();
    expect(rangeToPositions(index, 3, 1)).toBeNull();
  });

  it("returns null for an offset past the end", () => {
    const { doc } = docFor("<p>hi</p>");
    const index = docText(doc);
    expect(offsetToPos(index, 99)).toBeNull();
    expect(rangeToPositions(index, 1, 99)).toBeNull();
  });
});

describe("inline non-text nodes", () => {
  it("rejects a range spanning a hard break", () => {
    // A hard break consumes a document position but contributes no character to
    // the projection, so "abc" + <br> + "def" projects to "abcdef" and a query
    // like "cd" produces offsets whose ends are both resolvable while the span
    // between them is not the text that matched. Mapping it anyway would
    // highlight — and Replace would rewrite — a different four characters.
    const { doc } = docFor("<p>abc<br>def</p>");
    const index = docText(doc);
    expect(index.text).toBe("abcdef");
    const start = index.text.indexOf("cd");
    expect(rangeToPositions(index, start, start + 2)).toBeNull();
  });

  it("still resolves ranges wholly on one side of a hard break", () => {
    const { doc } = docFor("<p>abc<br>def</p>");
    const index = docText(doc);
    const range = rangeToPositions(index, 3, 6)!;
    expect(range).not.toBeNull();
    expect(textAt(doc, range.from, range.to)).toBe("def");
  });
});

describe("offsetToPos over many segments", () => {
  it("resolves correctly across a document with hundreds of text runs", () => {
    // The lookup is a binary search over the segment list; a boundary error in
    // it would only show up once there are enough segments to search through.
    const html = Array.from(
      { length: 200 },
      (_, i) => `<p>run${i} <strong>bold${i}</strong> tail${i}</p>`,
    ).join("");
    const { doc } = docFor(html);
    const index = docText(doc);

    for (const needle of ["run0", "bold7", "tail199", "run150"]) {
      const start = index.text.indexOf(needle);
      expect(start).toBeGreaterThanOrEqual(0);
      const range = rangeToPositions(index, start, start + needle.length)!;
      expect(range).not.toBeNull();
      expect(textAt(doc, range.from, range.to)).toBe(needle);
    }
    // Every separator offset still resolves to nothing.
    const sep = index.text.indexOf("\n");
    expect(offsetToPos(index, sep)).toBeNull();
  });
});
