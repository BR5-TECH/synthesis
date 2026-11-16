import { describe, expect, it } from "vitest";

import { descriptionExcerpt } from "./excerpt";
import { blocksFor, decl, sheet } from "../../test/cssRules";

describe("descriptionExcerpt (FLO-FR-XCRP)", () => {
  it("FLO-FR-XCRP: the first block that is not a heading, with no heading text joined to it", () => {
    expect(descriptionExcerpt("## Description\nFull Implementation of the Executor\n\n## Criteria\n- one")).toBe(
      "Full Implementation of the Executor",
    );
    expect(descriptionExcerpt("## Description\nTBD\n## High-Level Acceptance Criteria\n- TBD")).toBe("TBD");
  });

  it("FLO-FR-XCRP: the Markdown marks are removed and each run of white space is one space", () => {
    expect(descriptionExcerpt("Use **bold**,  _em_ and `code`\nand a [link](https://x.test).")).toBe(
      "Use bold, em and code and a link.",
    );
  });

  it("FLO-FR-XCRP: a list item or a quote is a block like a paragraph", () => {
    expect(descriptionExcerpt("# Title\n\n- first item\n- second")).toBe("first item");
    expect(descriptionExcerpt("> quoted text")).toBe("quoted text");
  });

  it("FLO-FR-XCRP: no description, only headings, or only rules give no excerpt", () => {
    expect(descriptionExcerpt(undefined)).toBeNull();
    expect(descriptionExcerpt("")).toBeNull();
    expect(descriptionExcerpt("## Only\n### Headings")).toBeNull();
    expect(descriptionExcerpt("---\n\n## End")).toBeNull();
  });

  it("FLO-FR-XCRP, FLO-FR-CMPK: the excerpt and a card's name show in at most two lines, then an ellipsis", () => {
    const css = sheet("kit.css");
    for (const selector of [".flow-excerpt", "span.flow-card__name"]) {
      const [rule] = blocksFor(css, selector);
      expect(rule, selector).toBeDefined();
      expect(decl(rule, "-webkit-line-clamp")).toBe("2");
      expect(decl(rule, "-webkit-box-orient")).toBe("vertical");
      expect(decl(rule, "overflow")).toBe("hidden");
    }
  });

  it("FLO-FR-13: the panel's description box keeps its height in the scrolling fields and scrolls inside", () => {
    const [rule] = blocksFor(sheet("kit.css"), ".flow-panel__desc");
    expect(decl(rule, "flex")).toBe("none");
    expect(decl(rule, "overflow-y")).toBe("auto");
  });
});
