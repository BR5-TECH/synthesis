/**
 * DCR-FR-LGHZ / DCR-FR-TSNW: a proposed change is found in a document built by
 * the real Editor, whatever Markdown the change quotes.
 *
 * The documents here go through the same parser the WYSIWYG surface uses, so a
 * construct that renders differently from its source is tested as it reaches
 * the author, not as a hand-built text would show it.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Editor } from "@tiptap/react";

vi.mock("../../logging", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../logging")>()),
  logWarn: vi.fn(),
}));

import { logWarn } from "../../logging";

import { docText } from "../findHighlight";
import { markdownExtensions } from "../markdownFidelity";
import { placeHunk, reduce, type PlaceableHunk, type RenderedNeedles } from "./hunkMatch";
import { leadTail, renderedText } from "./hunkNeedle";

const editors: Editor[] = [];

function editorFor(markdown: string): Editor {
  const editor = new Editor({ extensions: markdownExtensions(), content: markdown });
  editors.push(editor);
  return editor;
}

beforeEach(() => {
  vi.mocked(logWarn).mockClear();
});

afterEach(() => {
  while (editors.length > 0) editors.pop()?.destroy();
  vi.restoreAllMocks();
});

/** Place a change in `editor`'s document as the surface does. */
function place(editor: Editor, hunk: PlaceableHunk) {
  const index = docText(editor.state.doc);
  const needles: RenderedNeedles = {
    before: renderedText(editor, hunk.before),
    lead: renderedText(editor, hunk.lead),
    leadTail: (() => {
      const tail = leadTail(hunk.lead);
      return tail === null ? null : renderedText(editor, tail);
    })(),
  };
  return placeHunk(index, reduce(index.text), hunk, needles);
}

/** The document text a range covers, or where it ends for an insertion. */
function textOf(editor: Editor, range: { from: number; to: number }): string {
  return editor.state.doc.textBetween(range.from, range.to, "\n");
}

/** The document text that ends where an insertion is placed. */
function textBefore(editor: Editor, at: number, length: number): string {
  return editor.state.doc.textBetween(Math.max(0, at - length), at, "\n");
}

const replace = (before: string, hint = 0): PlaceableHunk => ({
  kind: "replace",
  before,
  lead: "",
  hint,
});

const insertAfter = (lead: string, hint = 0): PlaceableHunk => ({
  kind: "add",
  before: "",
  lead,
  hint,
});

describe("rendering a change's Markdown with the Editor's parser", () => {
  it("DCR-FR-LGHZ: renders to the text the document holds", () => {
    const editor = editorFor("x");
    expect(renderedText(editor, "see [site.com](http://site.com) now")).toBe(
      "see site.com now",
    );
    expect(renderedText(editor, "hosts like \\*.example.com")).toBe(
      "hosts like *.example.com",
    );
    expect(renderedText(editor, "")).toBe("");
  });

  it("DCR-FR-LGHZ: a parser that throws gives null and logs no prompt text", () => {
    const warn = vi.mocked(logWarn);
    const editor = editorFor("x");
    const storage = editor.storage as unknown as {
      markdown: { parser: { parse: (s: string) => string } };
    };
    vi.spyOn(storage.markdown.parser, "parse").mockImplementation(() => {
      throw new Error("boom");
    });
    expect(renderedText(editor, "secret prompt words")).toBeNull();
    expect(warn).toHaveBeenCalledTimes(1);
    expect(JSON.stringify(warn.mock.calls[0])).not.toContain("secret prompt words");
  });

  it("DCR-FR-LGHZ: an editor that reads no Markdown gives null and logs nothing", () => {
    const editor = new Editor({ extensions: markdownExtensions().filter((e) => e.name !== "markdown") });
    editors.push(editor);
    expect(renderedText(editor, "see [site.com](http://site.com)")).toBeNull();
    expect(logWarn).not.toHaveBeenCalled();
  });

  it("DCR-FR-TSNW: a lead loses its first line, and only where text is left", () => {
    expect(leadTail("ination](http://x) end\n- next item")).toBe("- next item");
    expect(leadTail("one line only")).toBeNull();
    expect(leadTail("text\n  \n")).toBeNull();
  });
});

describe("placing a change that quotes Markdown the renderer rewrites", () => {
  const cases: { name: string; doc: string; before: string; shown: string }[] = [
    {
      name: "a link",
      doc: "Intro.\n\nThe default is [site.com](http://site.com) for all.\n",
      before: "default is [site.com](http://site.com) for all.",
      shown: "default is site.com for all.",
    },
    {
      name: "a backslash escape",
      doc: "Hosts like \\*.example.com are allowed.\n",
      before: "like \\*.example.com are",
      shown: "like *.example.com are",
    },
    {
      name: "an autolink",
      doc: "Read <https://example.com/docs> first.\n",
      before: "Read <https://example.com/docs> first.",
      shown: "Read https://example.com/docs first.",
    },
    {
      name: "an entity",
      doc: "Salt &amp; pepper stay.\n",
      before: "Salt &amp; pepper",
      shown: "Salt & pepper",
    },
    {
      name: "a thematic break",
      doc: "Above the line.\n\n---\n\nBelow the line.\n",
      before: "Above the line.\n\n---\n\nBelow the line.",
      shown: "Above the line.\nBelow the line.",
    },
    {
      name: "a table",
      doc: "| key | value |\n| --- | --- |\n| port | 1420 |\n",
      before: "| key | value |\n| --- | --- |\n| port | 1420 |",
      shown: "key\nvalue\nport\n1420",
    },
    {
      name: "a task item",
      doc: "- [x] done item\n- [ ] open item\n",
      before: "- [x] done item",
      shown: "done item",
    },
    {
      name: "a hard break",
      doc: "first half  \nsecond half\n",
      before: "first half  \nsecond half",
      shown: "first halfsecond half",
    },
    {
      name: "an image",
      doc: "Look ![diagram](pic.png) here.\n",
      before: "Look ![diagram](pic.png) here.",
      shown: "Look  here.",
    },
  ];

  for (const c of cases) {
    it(`DCR-FR-LGHZ: a replacement quoting ${c.name} is placed over its text`, () => {
      const editor = editorFor(c.doc);
      const range = place(editor, replace(c.before));
      expect(range).not.toBeNull();
      expect(textOf(editor, range!).replace(/\s+/g, " ")).toBe(
        c.shown.replace(/\s+/g, " "),
      );
    });

    it(`DCR-FR-LGHZ: an insertion after ${c.name} is placed at its end`, () => {
      const editor = editorFor(c.doc);
      const range = place(editor, insertAfter(c.before));
      expect(range).not.toBeNull();
      expect(range!.from).toBe(range!.to);
      // The whole quoted text ends where the insertion goes, not only its
      // last word: "line." ends both blocks around a rule.
      const shown = c.shown.replace(/\s+/g, " ").trimEnd();
      const before = textBefore(editor, range!.from, range!.from)
        .replace(/\s+/g, " ")
        .trimEnd();
      expect(before.endsWith(shown)).toBe(true);
    });
  }

  it("DCR-FR-LGHZ, DCR-FR-31: text the document does not hold is still placed nowhere", () => {
    const editor = editorFor("The default is [site.com](http://site.com) for all.\n");
    expect(place(editor, replace("default is [other.com](http://other.com)"))).toBeNull();
    expect(place(editor, replace("default is site.com for none."))).toBeNull();
  });
});

describe("the proposal that drew one change of three", () => {
  // The same shape as the proposal an author reported, with invented text: a
  // lead in plain prose, a lead that ends in a link and a trailing space at the
  // end of a list item, and a lead that starts inside a link destination and
  // ends at the end of the file.
  const base = [
    "## Purpose",
    "",
    "The tool should talk to every mirror.",
    "",
    "## Steps",
    "",
    "- User opens the panel.",
    "- User types a mirror, with a placeholder of [mirror.org](http://mirror.org) ",
    "",
    "## Rules",
    "",
    "- [mirror.org](http://mirror.org) stays the default. Hosts like \\*.mirror.net are an option.",
    "- Saved mirrors keep working since [mirror.org](http://mirror.org) is a default.",
  ].join("\n");

  const first = insertAfter("## Purpose\n\nThe tool should talk to every mirror.", 50);
  const second = insertAfter(
    "panel.\n- User types a mirror, with a placeholder of [mirror.org](http://mirror.org) ",
    170,
  );
  // Cut inside the destination of the first link of the line, as the backend
  // cuts a lead at a fixed length.
  const third = insertAfter(
    base.slice(base.indexOf("rror.org) stays the default.")),
    base.length,
  );

  it("DCR-FR-LGHZ, DCR-FR-TSNW: every change is placed", () => {
    const editor = editorFor(base);
    const one = place(editor, first);
    const two = place(editor, second);
    const three = place(editor, third);
    expect(one).not.toBeNull();
    expect(two).not.toBeNull();
    expect(three).not.toBeNull();
    expect(textBefore(editor, one!.from, 13)).toBe("every mirror.");
    expect(textBefore(editor, two!.from, 26)).toMatch(/placeholder of mirror\.org$/);
    expect(textBefore(editor, three!.from, 20)).toMatch(/is a default\.$/);
  });

  it("DCR-FR-LGHZ, DCR-FR-TSNW: the other two stay placed after the first is accepted", () => {
    const accepted = base.replace(
      "every mirror.",
      "every mirror.\n\nThe user gives a host and a token.",
    );
    const editor = editorFor(accepted);
    const two = place(editor, second);
    const three = place(editor, third);
    expect(two).not.toBeNull();
    expect(three).not.toBeNull();
    expect(textBefore(editor, two!.from, 26)).toMatch(/placeholder of mirror\.org$/);
    expect(textBefore(editor, three!.from, 20)).toMatch(/is a default\.$/);
  });
});

describe("shortening an insertion's lead from its start (DCR-FR-TSNW)", () => {
  it("DCR-FR-TSNW: a lead whose start is gone is placed by its end", () => {
    const editor = editorFor("Keep this sentence and its last words here.\n");
    const range = place(editor, insertAfter("Removed start. sentence and its last words here."));
    expect(range).not.toBeNull();
    expect(textBefore(editor, range!.from, 11)).toBe("words here.");
  });

  it("DCR-FR-TSNW: fewer than three words of the end are not enough", () => {
    const editor = editorFor("Something else entirely, words here.\n");
    expect(place(editor, insertAfter("Not in the document at all, words here."))).toBeNull();
  });

  it("DCR-FR-TSNW: of several places, the one nearest the hint wins", () => {
    const text = "Alpha ends with these words.\n\nBeta ends with these words.\n";
    const editor = editorFor(text);
    const near = place(editor, insertAfter("Gamma ends with these words.", text.length));
    const far = place(editor, insertAfter("Gamma ends with these words.", 0));
    expect(near).not.toBeNull();
    expect(far).not.toBeNull();
    expect(near!.from).toBeGreaterThan(far!.from);
  });
});

describe("the limits of shortening a lead (DCR-FR-TSNW)", () => {
  it("DCR-FR-TSNW: a lead whose tail renders to nothing is placed by the lead's words", () => {
    const editor = editorFor(
      "- [mirror.org](http://mirror.org) stays the default.\n\n---\n\nNext.\n",
    );
    const range = place(editor, insertAfter("rror.org) stays the default.\n\n---\n"));
    expect(range).not.toBeNull();
    expect(textBefore(editor, range!.from, 18)).toBe("stays the default.");
  });

  it("DCR-FR-TSNW: words come off the start only, so a different last word places nothing", () => {
    const editor = editorFor("The panel opens on the left side.\n");
    expect(place(editor, insertAfter("The panel opens on the right side."))).toBeNull();
  });

  it("DCR-FR-TSNW: three words of the end are enough", () => {
    const editor = editorFor("Something else, opens the panel.\n");
    const range = place(editor, insertAfter("Wrong opens the panel."));
    expect(range).not.toBeNull();
    expect(textBefore(editor, range!.from, 16)).toBe("opens the panel.");
  });

  it("DCR-FR-TSNW: two words of the end are not", () => {
    const editor = editorFor("Something else, the panel.\n");
    expect(place(editor, insertAfter("Wrong opens the panel."))).toBeNull();
  });

  it("DCR-FR-TSNW: a one-line lead cut inside a link label is placed by its end", () => {
    const editor = editorFor("- [mirror.org](http://mirror.org) stays the default.\n");
    const range = place(editor, insertAfter("ror.org](http://mirror.org) stays the default."));
    expect(range).not.toBeNull();
    expect(textBefore(editor, range!.from, 18)).toBe("stays the default.");
  });

  it("DCR-FR-TSNW: the longest remainder wins over a shorter one nearer the hint", () => {
    const text = "Far: one two three four.\n\nNear: two three four.\n";
    const editor = editorFor(text);
    const range = place(editor, insertAfter("zzz one two three four.", text.length));
    expect(range).not.toBeNull();
    expect(textBefore(editor, range!.from, 24)).toBe("Far: one two three four.");
  });

  it("DCR-FR-TSNW: a lead whose second line has fewer than three words is not placed by it", () => {
    const editor = editorFor("Something else.\n\nOK.\n");
    expect(place(editor, insertAfter("A line the prompt never held.\nOK."))).toBeNull();
  });

  it("DCR-FR-05, DCR-FR-LGHZ: a replacement is never shortened to fit", () => {
    const editor = editorFor("site.com and other words.\n");
    expect(place(editor, replace("[site.com](http://site.com) and stale words."))).toBeNull();
  });
});
