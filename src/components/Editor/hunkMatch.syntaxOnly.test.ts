/**
 * A replacement or a deletion whose `before` renders to no text
 * (`../../../specifications/ui/DCR-draft-change-review.md` DCR-FR-MAHJ,
 * DCR-FR-31).
 *
 * An agent fills an empty bullet of a draft by replacing the bullet's syntax.
 * That syntax holds no text the document shows, so the change is placed by what
 * is around it.
 */
import { describe, expect, it } from "vitest";

import type { DocText } from "../findHighlight";
import { placeHunk, reduce, type PlaceableHunk } from "./hunkMatch";

/**
 * The rendered text of a document, as `docText` builds it: one newline between
 * textblocks, and no segment for an empty one.
 */
function rendered(paragraphs: string[]): DocText {
  let text = "";
  const segments: DocText["segments"] = [];
  let pos = 1;
  paragraphs.forEach((paragraph, at) => {
    if (at > 0) text += "\n";
    if (paragraph.length > 0) {
      segments.push({ offset: text.length, from: pos, length: paragraph.length });
    }
    text += paragraph;
    pos += paragraph.length + 2;
  });
  return { text, segments };
}

// "## User journey\n\n- \n\n## Requirements\n\nSee the spec and more."
const index = rendered(["User journey", "", "Requirements", "See the spec and more."]);
const doc = reduce(index.text);

/** The position right after "User journey" and the one at the head of "Requirements". */
const END_OF_JOURNEY = 1 + "User journey".length;
// Each textblock takes its length plus its open and close tokens.
const HEAD_OF_REQUIREMENTS = 1 + ("User journey".length + 2) + (0 + 2);

const fill = (over: Partial<PlaceableHunk>): PlaceableHunk => ({
  kind: "replace",
  before: "- ",
  lead: "## User journey\n\n",
  trail: "\n\n## Requirements",
  hint: 0,
  ...over,
});

describe("a change of syntax alone (DCR-FR-MAHJ)", () => {
  it("DCR-FR-MAHJ: a replacement and a deletion of an empty bullet stand at the end of their lead, with no width", () => {
    for (const kind of ["replace", "del"] as const) {
      expect(placeHunk(index, doc, fill({ kind })), kind).toEqual({
        from: END_OF_JOURNEY,
        to: END_OF_JOURNEY,
      });
    }
  });

  it("DCR-FR-MAHJ: every kind of syntax-only text is placed the same way", () => {
    for (const before of ["- ", "* ", "-\n", "  \n", "\n\n", "1. "]) {
      expect(placeHunk(index, doc, fill({ before })), JSON.stringify(before)).toEqual({
        from: END_OF_JOURNEY,
        to: END_OF_JOURNEY,
      });
    }
  });

  it("DCR-FR-MAHJ, DCR-FR-LGHZ: a text that renders to nothing is syntax alone, though its source holds a character", () => {
    // `-` alone renders to an empty list item. Searched by its source, it would
    // land on the hyphen of some other word.
    const hyphened = rendered(["User journey", "", "A well-known rule."]);
    const at = placeHunk(hyphened, reduce(hyphened.text), fill({ before: "-" }), {
      before: "",
      lead: "User journey",
      leadTail: null,
    });
    expect(at).toEqual({ from: END_OF_JOURNEY, to: END_OF_JOURNEY });
  });

  it("DCR-FR-MAHJ: with a lead that holds no text, it stands at the head of its trail", () => {
    const at = placeHunk(index, doc, fill({ lead: "\n\n" }));
    expect(at).toEqual({ from: HEAD_OF_REQUIREMENTS, to: HEAD_OF_REQUIREMENTS });
  });

  it("DCR-FR-MAHJ: with a lead the prompt does not hold, it stands at the head of its trail", () => {
    const at = placeHunk(index, doc, fill({ lead: "A sentence the prompt never held.\n\n" }));
    expect(at).toEqual({ from: HEAD_OF_REQUIREMENTS, to: HEAD_OF_REQUIREMENTS });
  });

  it("DCR-FR-MAHJ: the rendered trail is searched before the source trail", () => {
    // The source names "Requirements" and the rendered text names the next
    // paragraph. Only an order that tries the rendered text first lands there.
    const at = placeHunk(index, doc, fill({ lead: "", trail: "\n\n## Requirements" }), {
      before: "",
      lead: "",
      leadTail: null,
      trail: "See the spec and more.",
    });
    const head = HEAD_OF_REQUIREMENTS + "Requirements".length + 2;
    expect(at).toEqual({ from: head, to: head });
  });

  it("DCR-FR-MAHJ: a trail cut inside a link is found by as much of its start as the prompt holds", () => {
    const at = placeHunk(
      index,
      doc,
      fill({ lead: "", trail: "\n\nSee the spec and [the li" }),
    );
    const head = HEAD_OF_REQUIREMENTS + "Requirements".length + 2;
    expect(at).toEqual({ from: head, to: head });
  });

  it("DCR-FR-MAHJ, DCR-FR-31: with neither side found, it is a change the review cannot draw", () => {
    expect(
      placeHunk(index, doc, fill({ lead: "Not held.", trail: "Not held either." })),
    ).toBeNull();
    expect(placeHunk(index, doc, fill({ lead: "\n\n", trail: "" }))).toBeNull();
    // Two empty bullets in a row: each side of the change is syntax alone.
    for (const trail of ["\n\n", "\n- \n", "  "]) {
      expect(placeHunk(index, doc, fill({ lead: "\n\n", trail })), JSON.stringify(trail)).toBeNull();
    }
    expect(placeHunk(index, doc, fill({ lead: "", trail: undefined }))).toBeNull();
  });

  it("DCR-FR-05: a replacement that names text is still placed over that text", () => {
    const at = placeHunk(index, doc, fill({ before: "See the spec and more." }));
    expect(at).not.toBeNull();
    expect(at!.to - at!.from).toBe("See the spec and more.".length);
  });
});
