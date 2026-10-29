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

  it("DQA-FR-MIBQ, DQA-FR-SRMF: the rails stand side by side in equal shares, a thin rule between them", () => {
    const css = style();
    // The group is the grid, and the rails wrapper adds no box of its own.
    const group = blocksFor(css, ".discussion-questions__group--railed")[0];
    expect(decl(group, "display")).toBe("grid");
    expect(decl(group, "grid-template-columns")).toBe("minmax(0, 1fr) minmax(0, 1fr)");
    expect(decl(blocksFor(css, ".discussion-questions__rails")[0], "display")).toBe("contents");
    // The draft column's card rule is the more specific one, so it sets the
    // grid again rather than leaving the group a flex column.
    expect(
      decl(blocksFor(sheet("kit/discussion-stream.css"), ".dds-discussion .discussion-questions__group--railed")[0], "display"),
    ).toBe("grid");
    // The options and the description share the first row, side by side; the
    // own answer and the note span both columns below them.
    const options = blocksFor(css, ".discussion-questions__options")[0];
    const detail = blocksFor(css, ".discussion-questions__detail")[0];
    expect([decl(options, "grid-column"), decl(options, "grid-row")]).toEqual(["1", "1"]);
    expect([decl(detail, "grid-column"), decl(detail, "grid-row")]).toEqual(["2", "1"]);
    expect(
      decl(
        blocksFor(css, ".discussion-questions__group--railed > :not(.discussion-questions__rails)")[0],
        "grid-column",
      ),
    ).toBe("1 / -1");
    expect(decl(detail, "border-left")).toBe("1px solid var(--border-1)");

    // The draft column keeps that rule: it overrides no border of the rail,
    // side by side or stacked.
    const stream = sheet("kit/discussion-stream.css");
    for (const block of [
      ...blocksFor(stream, ".dds-discussion .discussion-questions__detail"),
      ...blocksFor(stackedRules(stream), ".dds-discussion .discussion-questions__detail"),
    ]) {
      for (const prop of ["border", "border-left", "border-top"]) {
        expect(decl(block, prop)).toBeNull();
      }
    }
  });

  it("DQA-FR-SRMF: the rails add no background, in any surface", () => {
    for (const name of ["kit/discussion-questions.css", "kit/discussion-stream.css"]) {
      let examined = 0;
      for (const rule of sheet(name).matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
        if (!/__(rails|options|detail|group--railed)/.test(rule[1])) continue;
        examined += 1;
        expect(decl(rule[2], "background")).toBeNull();
        expect(decl(rule[2], "background-color")).toBeNull();
      }
      // A guard that matches nothing passes for nothing.
      expect(examined).toBeGreaterThan(0);
    }
  });

  it("DQA-FR-LFIB: the description takes the option row's top inset and type, and every layer shares one cell", () => {
    const css = style();
    const row = blocksFor(css, ".discussion-questions__choice")[0];
    const option = blocksFor(css, ".discussion-questions__option")[0];
    const detail = blocksFor(css, ".discussion-questions__detail")[0];
    expect(decl(detail, "padding")?.split(" ")[0]).toBe(decl(row, "padding")?.split(" ")[0]);
    expect(decl(detail, "font-size")).toBe(decl(option, "font-size"));
    expect(decl(detail, "line-height")).toBe(decl(option, "line-height"));
    // Every layer shares one cell of a grid and keeps its space while it is
    // hidden, so the rail is as tall as the longest layer.
    expect(decl(detail, "display")).toBe("grid");
    const layer = blocksFor(css, ".discussion-questions__detail-text")[0];
    expect(decl(layer, "grid-area")).toBe("1 / 1");
    expect(decl(layer, "visibility")).toBe("hidden");
    expect(
      decl(blocksFor(css, '.discussion-questions__detail-text[data-shown="true"]')[0], "visibility"),
    ).toBe("visible");
    for (const name of ["kit/discussion-questions.css", "kit/discussion-stream.css"]) {
      for (const rule of sheet(name).matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
        if (rule[1].includes("__detail-text")) expect(decl(rule[2], "display")).toBeNull();
      }
    }

    // The draft column on the same terms, against its own row.
    const stream = sheet("kit/discussion-stream.css");
    const columnRow = blocksFor(stream, ".dds-discussion .discussion-questions__choice")[0];
    const columnOption = blocksFor(stream, ".dds-discussion .discussion-questions__option")[0];
    const columnDetail = blocksFor(stream, ".dds-discussion .discussion-questions__detail")[0];
    expect(decl(columnDetail, "padding")?.split(" ")[0]).toBe(
      decl(columnRow, "padding")?.split(" ")[0],
    );
    expect(decl(columnDetail, "line-height")).toBe(decl(columnOption, "line-height"));
    // A row set taller than its text would push its label below the
    // description's first line, so no row sets a height of its own.
    for (const block of [row, columnRow]) {
      expect(decl(block, "min-height")).toBeNull();
      expect(decl(block, "height")).toBeNull();
    }
    // The size is the base one, which the column's option takes too.
    expect(decl(columnDetail, "font-size") ?? decl(detail, "font-size")).toBe(
      decl(columnOption, "font-size"),
    );
  });

  it("DQA-FR-OGVU: under 560 px of the block's own width the rails stack into one column", () => {
    const css = style();
    expect(css).not.toContain("@media (max-width");
    const stacked = stackedRules(css);
    const group = blocksFor(stacked, ".discussion-questions__group--railed")[0];
    expect(decl(group, "grid-template-columns")).toBe("minmax(0, 1fr)");
    // The description moves to the foot of the group, after the own answer and
    // the note, set off by a thin top rule rather than by the side rule.
    const detail = blocksFor(stacked, ".discussion-questions__detail")[0];
    expect(decl(detail, "order")).toBe("1");
    // Back to the one column, or the grid would add a column beside the group.
    expect(decl(detail, "grid-column")).toBe("1");
    expect(decl(detail, "grid-row")).toBe("auto");
    expect(decl(blocksFor(stacked, ".discussion-questions__options")[0], "grid-row")).toBe("auto");
    expect(decl(detail, "border-left")).toBe("0");
    expect(decl(detail, "border-top")).toContain("1px solid");
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
      '.discussion-questions__detail-text[data-kind="placeholder"]',
    )[0];
    expect(decl(detail, "color")).toBe("var(--fg-2)");
    expect(decl(placeholder, "color")).toBe("var(--fg-3)");
    // The column overrides no placeholder tone down to the quietest one.
    expect(
      blocksFor(
        sheet("kit/discussion-stream.css"),
        '.dds-discussion .discussion-questions__detail-text[data-kind="placeholder"]',
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

describe("the rail while the own answer is chosen (DQA-FR-USTA)", () => {
  it("DQA-FR-USTA: a hidden rail keeps its place side by side and takes none when stacked", () => {
    const css = style();
    expect(
      decl(blocksFor(css, '.discussion-questions__detail[data-hidden="true"]')[0], "visibility"),
    ).toBe("hidden");
    // The shown layer sets its own visibility, so the hidden rail must reach it
    // too, or the placeholder stays on screen without its rule.
    expect(
      decl(
        blocksFor(
          css,
          '.discussion-questions__detail[data-hidden="true"] .discussion-questions__detail-text',
        )[0],
        "visibility",
      ),
    ).toBe("hidden");
    expect(
      decl(blocksFor(stackedRules(css), '.discussion-questions__detail[data-hidden="true"]')[0], "display"),
    ).toBe("none");
  });
});

describe("the question text (DQA-FR-FJXQ)", () => {
  it("DQA-FR-FJXQ: one step above the default text in the draft column, two steps elsewhere", () => {
    const stream = sheet("kit/discussion-stream.css");
    expect(decl(blocksFor(style(), ".discussion-questions__ask")[0], "font-size")).toBe(
      "var(--fs-ui-lg)",
    );
    expect(
      decl(blocksFor(stream, ".dds-discussion .discussion-questions__ask")[0], "font-size"),
    ).toBe("var(--fs-ui-title)");
    // The default text the steps count from is the option labels' size.
    expect(decl(blocksFor(style(), ".discussion-questions__option")[0], "font-size")).toBe(
      "var(--fs-ui-md)",
    );
    expect(
      decl(blocksFor(stream, ".dds-discussion .discussion-questions__option")[0], "font-size"),
    ).toBe("var(--fs-ui-md)");
  });

  it("DQA-FR-FJXQ: no other text of the block is as large as the question", () => {
    // Ordered smallest first; a text at or above the question's step fails.
    const scale = ["--fs-ui-xs", "--fs-ui-sm", "--fs-ui-md", "--fs-ui-title", "--fs-ui-lg"];
    const step = (size: string) => scale.findIndex((token) => size.includes(`${token})`));
    for (const [name, prefix, ask] of [
      ["kit/discussion-questions.css", "", "--fs-ui-lg"],
      ["kit/discussion-stream.css", ".dds-discussion ", "--fs-ui-title"],
    ] as const) {
      for (const rule of sheet(name).matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
        const selector = rule[1].trim();
        if (!selector.startsWith(`${prefix}.discussion-questions`)) continue;
        if (selector.includes("__ask")) continue;
        const size = decl(rule[2], "font-size");
        if (size === null) continue;
        expect(step(size), `${selector}: ${size}`).toBeLessThan(step(`var(${ask})`));
      }
    }
  });
});
