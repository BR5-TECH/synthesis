import { describe, expect, it } from "vitest";
import { blocksFor, decl, sheet } from "./cssRules";

/**
 * Stylesheet invariants of the answering block's two rails and of the option
 * descriptions in history
 * (`../../specifications/ui/DQA-discussion-question-answering.md`).
 *
 * jsdom evaluates no container query and computes no layout, so these are the
 * only checks that the rails stand side by side, stack where the block is
 * narrow, and read the block's own width rather than the window's.
 */

const style = () => sheet("kit/discussion-questions.css");
const STACKED = "@container discussion-questions (max-width: 559.98px)";

/** The part of the stylesheet inside the stacking query. */
function stackedRules(css: string): string {
  const at = css.indexOf(STACKED);
  expect(at).toBeGreaterThanOrEqual(0);
  const open = css.indexOf("{", at);
  let depth = 0;
  for (let index = open; index < css.length; index += 1) {
    if (css[index] === "{") depth += 1;
    if (css[index] === "}") depth -= 1;
    if (depth === 0) return css.slice(open + 1, index);
  }
  throw new Error("the stacking query does not close");
}

describe("the two rails (DQA-FR-MIBQ, DQA-FR-OGVU)", () => {
  it("DQA-FR-OGVU: the block is a named container of its own inline size, and keeps its width", () => {
    const block = blocksFor(style(), ".discussion-questions")[0];
    expect(decl(block, "container")).toBe("discussion-questions / inline-size");
    // Inline-size containment gives a block with no width of its own a width
    // of nothing, so the width it fills is what keeps it on the measure.
    expect(decl(block, "width")).toBe("100%");
  });

  it("DQA-FR-OGVU: no unnamed container query styles the block, which is now the nearest container", () => {
    for (const name of ["kit/discussion-questions.css", "kit/discussion-stream.css"]) {
      const css = sheet(name);
      for (const match of css.matchAll(/@container \(/g)) {
        const open = css.indexOf("{", match.index);
        let depth = 0;
        let end = open;
        for (; end < css.length; end += 1) {
          if (css[end] === "{") depth += 1;
          if (css[end] === "}") depth -= 1;
          if (depth === 0) break;
        }
        expect(css.slice(open, end)).not.toContain(".discussion-questions");
      }
    }
  });

  it("DQA-FR-MIBQ: the rails stand side by side, the group as a column rather than as grid cells", () => {
    const rails = blocksFor(style(), ".discussion-questions__rails")[0];
    expect(decl(rails, "display")).toBe("grid");
    expect(decl(rails, "grid-template-columns")).toBe("minmax(0, 2fr) minmax(0, 3fr)");
    const group = blocksFor(
      style(),
      ".discussion-questions__rails > .discussion-questions__group",
    )[0];
    expect(decl(group, "display")).toBe("flex");
  });

  it("DQA-FR-OGVU: under 560 px of the block's own width the rails stack into one column", () => {
    const css = style();
    expect(css).not.toContain("@media (max-width");
    const stacked = stackedRules(css);
    const rails = blocksFor(stacked, ".discussion-questions__rails")[0];
    expect(decl(rails, "grid-template-columns")).toBe("minmax(0, 1fr)");
    // The description becomes a panel below the note, set off by its top edge
    // rather than by the side rule it takes beside the labels.
    const detail = blocksFor(stacked, ".discussion-questions__detail")[0];
    expect(decl(detail, "border-left")).toBe("0");
    expect(decl(detail, "border-top")).toContain("1px solid");
    // Lines held for the description, so the foot stays put under the pointer.
    expect(decl(detail, "min-height")).toBe("3lh");
  });

  it("DQA-FR-OGVU: the draft column stacks on the same query, so its own detail rule follows it", () => {
    const stream = sheet("kit/discussion-stream.css");
    const stacked = stackedRules(stream);
    expect(blocksFor(stacked, ".dds-discussion .discussion-questions__detail")).toHaveLength(1);
  });

  it("DQA-FR-TKUM, DQA-FR-VNKQ: the placeholder takes a quieter tone than a description, and keeps its contrast in the column", () => {
    const css = style();
    const detail = blocksFor(css, ".discussion-questions__detail")[0];
    const placeholder = blocksFor(
      css,
      '.discussion-questions__detail[data-placeholder="true"]',
    )[0];
    expect(decl(detail, "color")).toBe("var(--fg-2)");
    expect(decl(placeholder, "color")).toBe("var(--fg-3)");
    // The column overrides no placeholder tone down to the quietest one.
    expect(
      blocksFor(
        sheet("kit/discussion-stream.css"),
        '.dds-discussion .discussion-questions__detail[data-placeholder="true"]',
      ),
    ).toHaveLength(0);
  });
});

describe("the option descriptions in history (DQA-FR-XRLK)", () => {
  it("DQA-FR-XRLK: a description is set quieter and smaller than its label", () => {
    const css = style();
    const label = blocksFor(css, ".comment__option-label")[0];
    const description = blocksFor(css, ".comment__option-description")[0];
    expect(decl(label, "color")).toBe("var(--fg-1)");
    expect(decl(description, "color")).toBe("var(--fg-2)");
    // The labels take the body's own size, so expanding moves nothing.
    expect(decl(label, "font-size")).toBe("var(--fs-ui-sm)");
    expect(decl(blocksFor(sheet("kit/notes-cont.css"), ".comment__body")[0], "font-size")).toBe(
      "var(--fs-ui-sm)",
    );

    // In the card, a description never reads louder than an option the author
    // did not choose.
    const stream = sheet("kit/discussion-stream.css");
    const card = blocksFor(stream, ".dds-question__description")[0];
    const option = blocksFor(stream, ".dds-question__option")[0];
    expect(decl(card, "display")).toBe("block");
    expect(decl(card, "color")).toBe(decl(option, "color"));
  });
});

describe("the descriptions control is always visible (DQA-FR-MDMU)", () => {
  it("DQA-FR-MDMU: no rule hides the control, and in the card it takes the trailing end beside the time", () => {
    for (const name of ["kit/discussion-questions.css", "kit/discussion-stream.css"]) {
      for (const rule of sheet(name).matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
        if (!rule[1].includes("descriptions-toggle")) continue;
        expect(decl(rule[2], "opacity")).toBeNull();
        expect(decl(rule[2], "visibility")).toBeNull();
        expect(decl(rule[2], "display")).toBeNull();
      }
    }
    const stream = sheet("kit/discussion-stream.css");
    expect(decl(blocksFor(stream, ".dds-question__descriptions-toggle")[0], "margin-left")).toBe("auto");
    expect(
      decl(blocksFor(stream, ".dds-question__descriptions-toggle + .dds-stream__time")[0], "margin-left"),
    ).toBe("0");
  });
});
