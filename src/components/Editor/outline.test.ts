import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("mermaid", async () => (await import("../../test/mermaidMock")).mermaidModule);
vi.mock("../../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
}));

import { resetMermaidMock } from "../../test/mermaidMock";
import { resetMermaidForTest } from "../markdownMermaidRender";
import { destroyRichEditor, mountRichEditor } from "../../test/richEditorFixtures";
import { plainHeadingText, richHeadings, sourceHeadings } from "./outline";

beforeEach(() => {
  resetMermaidMock();
  resetMermaidForTest();
});

afterEach(() => {
  destroyRichEditor();
});

const summary = (entries: ReturnType<typeof sourceHeadings>) =>
  entries.map((e) => `${e.level}:${e.text}`);

describe("sourceHeadings (EDT-FR-OCJG)", () => {
  it("EDT-FR-OCJG, EDT-FR-PNGU: lists the ATX headings of levels 1 to 6 in document order", () => {
    const text = "# One\n\npara\n\n## Two\n### Three\n#### Four\n##### Five\n###### Six\n";
    expect(summary(sourceHeadings(text))).toEqual([
      "1:One",
      "2:Two",
      "3:Three",
      "4:Four",
      "5:Five",
      "6:Six",
    ]);
  });

  it("EDT-FR-OCJG: a line that only looks like a heading is not one", () => {
    const text = "#nospace\n####### seven\n    # indented code\n\\# escaped\ntext # not\n";
    expect(sourceHeadings(text)).toEqual([]);
  });

  it("EDT-FR-OCJG: accepts up to three leading spaces and removes the closing hashes", () => {
    const text = "   ## Spaced ##\n# C# notes\n# Trailing #   \n#\n";
    expect(summary(sourceHeadings(text))).toEqual([
      "2:Spaced",
      "1:C# notes",
      "1:Trailing",
      "1:",
    ]);
  });

  it("EDT-FR-OCJG: a line inside the leading frontmatter block is never a heading", () => {
    const text = "---\ntitle: x\n# not a heading\n---\n# Real\n";
    const entries = sourceHeadings(text);
    expect(summary(entries)).toEqual(["1:Real"]);
    expect(entries[0].target).toEqual({ kind: "source", offset: text.indexOf("# Real") });
  });

  it("EDT-FR-OCJG: a `---` block that is not at the start is not frontmatter", () => {
    const text = "# Top\n\n---\n# After rule\n---\n";
    expect(summary(sourceHeadings(text))).toEqual(["1:Top", "1:After rule"]);
  });

  it("EDT-FR-OCJG: lines inside backtick and tilde fences are never headings", () => {
    const text = [
      "# A",
      "```mermaid",
      "# inside backticks",
      "```",
      "~~~",
      "# inside tildes",
      "~~~",
      "# B",
    ].join("\n");
    expect(summary(sourceHeadings(text))).toEqual(["1:A", "1:B"]);
  });

  it("EDT-FR-OCJG: a fence closes only on a fence of its own character and length", () => {
    const text = [
      "````md",
      "```",
      "# still inside",
      "~~~",
      "````",
      "# Out",
    ].join("\n");
    expect(summary(sourceHeadings(text))).toEqual(["1:Out"]);
  });

  it("EDT-FR-OCJG: a fence that does not close holds the rest of the text", () => {
    expect(summary(sourceHeadings("# A\n```\n# B\n# C\n"))).toEqual(["1:A"]);
  });

  it("EDT-FR-OCJG, EDT-FR-YSJX: carries the offset of the start of each heading line, also with CRLF", () => {
    const text = "intro\r\n## Two\r\n\r\n# One\r\n";
    const entries = sourceHeadings(text);
    expect(summary(entries)).toEqual(["2:Two", "1:One"]);
    expect(entries.map((e) => e.target)).toEqual([
      { kind: "source", offset: text.indexOf("## Two") },
      { kind: "source", offset: text.indexOf("# One") },
    ]);
  });

  it("EDT-FR-PNGU: an entry shows the heading text with no inline formatting", () => {
    const text = "# **Bold** and *em* with `code`, [a link](x.md), ~~gone~~ and ![pic](p.png)\n";
    expect(summary(sourceHeadings(text))).toEqual([
      "1:Bold and em with code, a link, gone and pic",
    ]);
  });

  it("EDT-FR-OCJG, EDT-FR-JVXN: a heading in a block quote or a list item is listed, as the rich body lists it", () => {
    const text = "> # Quoted\n>> ## Deeper\n- ### In a list\n1. #### Numbered\n- plain item\n";
    expect(summary(sourceHeadings(text))).toEqual([
      "1:Quoted",
      "2:Deeper",
      "3:In a list",
      "4:Numbered",
    ]);
  });

  it("EDT-FR-OCJG: a closing fence carries no info string, and a backtick info string is not a fence", () => {
    expect(summary(sourceHeadings("```\n# in\n```js\n# still in\n```\n# Out\n"))).toEqual([
      "1:Out",
    ]);
    expect(summary(sourceHeadings("```a`b\n# Listed\n"))).toEqual(["1:Listed"]);
  });

  it("EDT-FR-OCJG: a setext heading is not an ATX heading", () => {
    expect(sourceHeadings("Title\n=====\n\nSub\n---\n")).toEqual([]);
  });

  it("EDT-FR-OCJG: the last line is listed without a newline, and frontmatter alone gives nothing", () => {
    expect(summary(sourceHeadings("text\n## Last"))).toEqual(["2:Last"]);
    expect(sourceHeadings("---\ntitle: x\n---\n")).toEqual([]);
  });

  it("EDT-FR-EBNU: a document with no heading gives no entry", () => {
    expect(sourceHeadings("")).toEqual([]);
    expect(sourceHeadings("just text\n\nmore\n")).toEqual([]);
  });

  it("EDT-FR-OCJG, EDT-FR-JVXN: one pass handles a document of several thousand lines", () => {
    const lines: string[] = [];
    for (let i = 0; i < 5000; i++) lines.push(i % 50 === 0 ? `## Section ${i}` : `line ${i}`);
    const entries = sourceHeadings(lines.join("\n"));
    expect(entries).toHaveLength(100);
    expect(entries[99].text).toBe("Section 4950");
  });
});

describe("plainHeadingText (EDT-FR-PNGU)", () => {
  it("EDT-FR-PNGU: keeps plain text as it is and trims it", () => {
    expect(plainHeadingText("  Prerequisites  ")).toBe("Prerequisites");
  });
});

describe("richHeadings (EDT-FR-JVXN)", () => {
  it("EDT-FR-JVXN, EDT-FR-PNGU: lists the headings of the rich body with their levels and positions", () => {
    const editor = mountRichEditor("# One\n\ntext\n\n## **Two** bold\n\n#### Four\n");
    const entries = richHeadings(editor.state.doc);
    expect(entries.map((e) => `${e.level}:${e.text}`)).toEqual([
      "1:One",
      "2:Two bold",
      "4:Four",
    ]);
    for (const entry of entries) {
      if (entry.target.kind !== "rich") throw new Error("expected a rich target");
      expect(editor.state.doc.nodeAt(entry.target.pos)?.type.name).toBe("heading");
    }
  });

  it("EDT-FR-JVXN, EDT-FR-OCJG: the two modes list the same headings of one document", () => {
    const md =
      "# A\n\n> ## Quoted\n\n```\n# code\n```\n\n> ```\n> # in a quoted fence\n> ```\n\n" +
      "- item\n\n  ```\n  # in a listed fence\n  ```\n\n### C\n";
    const editor = mountRichEditor(md);
    expect(richHeadings(editor.state.doc).map((e) => `${e.level}:${e.text}`)).toEqual(
      summary(sourceHeadings(md)),
    );
  });

  it("EDT-FR-JVXN: the content of a code block or a diagram fence never gives an entry", () => {
    const editor = mountRichEditor(
      "# A\n\n```mermaid\n# not a heading\n```\n\n```\n## nor this\n```\n\n> ## Quoted\n",
    );
    expect(richHeadings(editor.state.doc).map((e) => e.text)).toEqual(["A", "Quoted"]);
  });
});
