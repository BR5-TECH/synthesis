/**
 * DCR-FR-05 / DCR-FR-06: a proposed change is found in the **rendered**
 * document, though its text was copied from the **source**.
 *
 * Every case here is one an author hit: a review bar reporting five changes
 * over a document showing one, and a proposal of seven showing none. What they
 * had in common was Markdown — an inline-code span, a heading, a bullet, a
 * paragraph break — in the text the agent quoted.
 */
import { describe, expect, it } from "vitest";

import { docText } from "../findHighlight";
import { findAfter, findRange, placeHunk, reduce } from "./hunkMatch";
import type { DocText } from "../findHighlight";

/**
 * The rendered text of a document, and the index back to its positions.
 *
 * Built by hand rather than through Tiptap: `docText` is the projection the
 * surface actually searches, and what matters here is that a change quoting
 * source syntax is found in it. Paragraphs are separated by one newline,
 * exactly as `docText` separates textblocks.
 */
function rendered(paragraphs: string[]): DocText {
  let text = "";
  const segments = [];
  let pos = 1;
  for (const paragraph of paragraphs) {
    if (text.length > 0) text += "\n";
    segments.push({ offset: text.length, from: pos, length: paragraph.length });
    text += paragraph;
    // One position for the paragraph's open and close tokens either side.
    pos += paragraph.length + 2;
  }
  return { text, segments };
}

describe("reducing text to what survives rendering", () => {
  it("DCR-FR-05: drops the syntax the renderer drops", () => {
    // The backticks reach no rendered text: an inline-code span is a text node
    // carrying a mark, and the marks are not characters.
    expect(reduce("the `worktree prune` command").text).toBe(
      "the worktree prune command",
    );
    expect(reduce("## Endpoint contract").text).toBe("Endpoint contract");
    expect(reduce("**bold** and _thin_").text).toBe("bold and thin");
    expect(reduce("> quoted").text).toBe("quoted");
  });

  it("DCR-FR-05: makes every run of space one space", () => {
    // A paragraph break is two newlines in the source and one in the rendered
    // text, and an indented list item carries leading space in one and none in
    // the other.
    expect(reduce("one\n\ntwo").text).toBe("one two");
    expect(reduce("one\ntwo").text).toBe("one two");
    expect(reduce("  indented").text).toBe("indented");
    expect(reduce("a \t b").text).toBe("a b");
  });

  it("DCR-FR-06: every character it keeps knows where it came from", () => {
    const source = "the `code` word";
    const { text, map } = reduce(source);
    expect(text).toBe("the code word");
    for (let i = 0; i < text.length; i += 1) {
      // A space stands for the run that produced it and is indexed at what
      // follows, so every other character maps to itself exactly.
      if (text[i] === " ") continue;
      expect(source[map[i]]).toBe(text[i]);
    }
  });

  it("DCR-FR-05: keeps the words, so this is no fuzzy match", () => {
    // Nothing here tolerates a difference in the letters. A change naming text
    // the prompt does not hold must still fail to place, or the review would
    // decorate words the agent never read.
    const doc = rendered(["the endpoint must require authentication"]);
    const index = reduce(doc.text);
    expect(findRange(doc, index, "the endpoint must require permission", 0)).toBeNull();
  });
});

describe("placing a change whose text carries Markdown", () => {
  it("DCR-FR-05: a change quoting an inline-code span is placed", () => {
    // The case an author reported: a bar naming five changes over a document
    // drawing one, every unplaced change quoting a span like this.
    const doc = rendered([
      "The endpoint is unauthenticated.",
      "BMS-backend-microservice.md must be updated from its version-only contract.",
    ]);
    const index = reduce(doc.text);
    const range = findRange(
      doc,
      index,
      "`BMS-backend-microservice.md` must be updated",
      0,
    );
    expect(range).not.toBeNull();
    // And on the words themselves, not merely somewhere in the document.
    expect(range!.from).toBe(doc.segments[1].from);
  });

  it("DCR-FR-05: a change spanning a paragraph break is placed", () => {
    const doc = rendered(["The first line.", "The second line."]);
    const index = reduce(doc.text);
    // Two newlines in the source, one in the rendered text.
    expect(findRange(doc, index, "The first line.\n\nThe second", 0)).not.toBeNull();
  });

  it("DCR-FR-05: a change quoting a heading is placed", () => {
    const doc = rendered(["Endpoint contract", "Returns a JSON object."]);
    const index = reduce(doc.text);
    expect(findRange(doc, index, "## Endpoint contract", 0)).not.toBeNull();
  });

  it("DCR-FR-05: a change quoting a list is placed", () => {
    const doc = rendered(["Require the header", "Keep it unauthenticated"]);
    const index = reduce(doc.text);
    expect(
      findRange(doc, index, "- Require the header\n- Keep it unauthenticated", 0),
    ).not.toBeNull();
  });
});

describe("which occurrence a change is placed on", () => {
  it("DCR-FR-06: the one nearest the recorded hint", () => {
    // The text is the identity and the offset only decides which of several
    // places is meant, exactly as an anchored comment is resolved.
    const doc = rendered(["repeat", "middle", "repeat"]);
    const index = reduce(doc.text);
    const early = findRange(doc, index, "repeat", 0);
    const late = findRange(doc, index, "repeat", 100);
    expect(early!.from).toBe(doc.segments[0].from);
    expect(late!.from).toBe(doc.segments[2].from);
  });

  it("DCR-FR-05: an insertion is placed at the end of the text it follows", () => {
    const doc = rendered(["Worktrees this project created."]);
    const index = reduce(doc.text);
    const at = findAfter(doc, index, "Worktrees this project created.", 0);
    expect(at).not.toBeNull();
    expect(at!.from).toBe(at!.to);
    expect(at!.from).toBe(doc.segments[0].from + doc.segments[0].length);
  });

  it("DCR-FR-31: a change naming text that is not there is placed nowhere", () => {
    const doc = rendered(["Worktrees this project created."]);
    const index = reduce(doc.text);
    expect(findRange(doc, index, "words the prompt never held", 0)).toBeNull();
    expect(findRange(doc, index, "", 0)).toBeNull();
  });
});

describe("the proposal an author could not see", () => {
  /**
   * The passage from a real draft, as the renderer draws it and as the agent
   * read it. Two changes were reported over this and neither was drawn.
   */
  const doc = rendered([
    "Endpoint contract",
    "GET /v1/health",
    "Returns a JSON object containing:",
    "The endpoint is unauthenticated. It must not expose keys, QR contents, device registrations, application payloads, user identity, project ownership, or connection state. BMS-backend-microservice.md must be updated from its current version-only health contract.",
    "GET /v1/projects",
    "Returns the currently exposed projects available to an authenticated remote client. The endpoint must require authentication by a registered client handle and must not expose a global unauthenticated project list.",
  ]);
  const index = reduce(doc.text);

  it("DCR-FR-05: a change quoting a file name in code is placed", () => {
    const range = findRange(
      doc,
      index,
      "`BMS-backend-microservice.md` must be updated from its current version-only health contract.",
      0,
    );
    expect(range).not.toBeNull();
    // The hyphens inside the file name and inside `version-only` are kept: a
    // hyphen is a bullet at the head of a line and a character of a word here.
    expect(range!.to).toBe(doc.segments[3].from + doc.segments[3].length);
  });

  it("DCR-FR-05: a change quoting a heading and the paragraph under it is placed", () => {
    expect(
      findRange(
        doc,
        index,
        "### GET /v1/projects\n\nReturns the currently exposed projects",
        0,
      ),
    ).not.toBeNull();
  });

  it("DCR-FR-05: a change quoting a bulleted list is placed", () => {
    const list = rendered([
      "Require Authorization: Bearer <SYNTHESIS_SERVER_TOKEN> on every endpoint, including /v1/health.",
      "Keep /v1/health unauthenticated; use the server token for every other endpoint.",
    ]);
    expect(
      findRange(
        list,
        reduce(list.text),
        "- Require `Authorization: Bearer <SYNTHESIS_SERVER_TOKEN>` on every endpoint, including `/v1/health`.\n- Keep `/v1/health` unauthenticated; use the server token for every other endpoint.",
        0,
      ),
    ).not.toBeNull();
  });

  it("DCR-FR-05: a change quoting a fenced code block is placed", () => {
    const fenced = rendered([
      "Returns a JSON object containing:",
      '{\n  "version": "<relay-version>"\n}',
    ]);
    expect(
      findRange(
        fenced,
        reduce(fenced.text),
        'Returns a JSON object containing:\n\n```json\n{\n  "version": "<relay-version>"\n}\n```',
        0,
      ),
    ).not.toBeNull();
  });
});

describe("placing a change by its kind (DCR-FR-05)", () => {
  const paragraphs = ["Intent", "The old body.", "The tail."];
  const index = rendered(paragraphs);
  const doc = reduce(index.text);

  /** The document text the positions `from`..`to` actually cover. */
  const covered = (at: { from: number; to: number }) =>
    index.segments
      .map((s) =>
        index.text.slice(
          s.offset + Math.max(at.from - s.from, 0),
          s.offset + Math.min(at.to - s.from, s.length),
        ),
      )
      .join("");

  it("DCR-FR-05: a replacement is placed over exactly the text it replaces", () => {
    const at = placeHunk(index, doc, {
      kind: "replace",
      before: "The old body.",
      lead: "Intent",
      hint: 7,
    });
    expect(at).not.toBeNull();
    // Not merely a non-empty range: a regression that placed it over the wrong
    // paragraph, or over one character of the right one, must fail here.
    expect(covered(at!)).toBe("The old body.");
  });

  it("DCR-FR-05: a deletion is placed over exactly the text it removes", () => {
    const at = placeHunk(index, doc, {
      kind: "del",
      before: "The tail.",
      lead: "The old body.",
      hint: 21,
    });
    expect(at).not.toBeNull();
    expect(covered(at!)).toBe("The tail.");
  });

  it("DCR-FR-05: an insertion is placed after the text it follows, covering none", () => {
    const at = placeHunk(index, doc, {
      kind: "add",
      before: "",
      lead: "Intent",
      hint: 7,
    });
    expect(at).toEqual({ from: 7, to: 7 });
  });

  it("DCR-FR-05: an insertion that still carries a range covers no text either", () => {
    // A record can hold a `before` an insertion never applies (DCP-FR-HRQN).
    // Placed over it, the change would strike text accepting does not remove.
    const at = placeHunk(index, doc, {
      kind: "add",
      before: "The old body.",
      lead: "Intent",
      hint: 7,
    });
    expect(at).toEqual({ from: 7, to: 7 });
  });

  it("DCR-FR-05: an insertion naming no text it follows goes at the head", () => {
    // `lead` is empty at the head of the prompt, and also where the change
    // before this one ends exactly where this one starts — so this is not the
    // empty-prompt case alone.
    const empty = rendered([""]);
    expect(
      placeHunk(empty, reduce(empty.text), { kind: "add", before: "", lead: "", hint: 0 }),
    ).toEqual({ from: 1, to: 1 });
    expect(
      placeHunk(index, doc, { kind: "add", before: "", lead: "", hint: 0 }),
    ).toEqual({ from: 1, to: 1 });
  });

  it("DCR-FR-KDSV: a whole-prompt replacement over a prompt holding nothing is placed", () => {
    // A legacy proposal is one replacement covering the whole prompt. Over an
    // empty prompt its `before` is empty, and it names nothing to strike — but
    // it is still a change with somewhere to go, so it is drawn rather than
    // reported as one the surface cannot show.
    const empty = rendered([""]);
    expect(
      placeHunk(empty, reduce(empty.text), {
        kind: "replace",
        before: "",
        lead: "",
        hint: 0,
      }),
    ).toEqual({ from: 1, to: 1 });
  });

  it("DCR-FR-05, DCR-FR-31: a replacement naming no text is not placed at all", () => {
    // Drawn as an insertion instead, it would show the green block on its own
    // and leave the text it replaces unmarked — a rewrite read as an addition.
    // Unplaced, the review bar says it is not drawn and it stays decidable.
    for (const kind of ["replace", "del"] as const) {
      expect(
        placeHunk(index, doc, { kind, before: "", lead: "Intent", hint: 7 }),
        kind,
      ).toBeNull();
    }
  });

  it("DCR-FR-05, DCR-FR-31: an insertion whose lead the prompt no longer holds is not placed", () => {
    expect(
      placeHunk(index, doc, {
        kind: "add",
        before: "",
        lead: "text that was never here",
        hint: 7,
      }),
    ).toBeNull();
  });

  it("DCR-FR-06: an insertion whose lead occurs twice takes the one nearest the hint", () => {
    const twice = rendered(["Repeated.", "Between.", "Repeated."]);
    const reduced = reduce(twice.text);
    const near = placeHunk(twice, reduced, { kind: "add", before: "", lead: "Repeated.", hint: 0 });
    const far = placeHunk(twice, reduced, { kind: "add", before: "", lead: "Repeated.", hint: 22 });
    expect(near).not.toEqual(far);
    expect(far!.from).toBeGreaterThan(near!.from);
  });

  it("DCR-FR-05, DCR-FR-31: a change naming text the prompt no longer holds is not placed", () => {
    expect(
      placeHunk(index, doc, {
        kind: "replace",
        before: "text that was never here",
        lead: "Intent",
        hint: 7,
      }),
    ).toBeNull();
  });
});
