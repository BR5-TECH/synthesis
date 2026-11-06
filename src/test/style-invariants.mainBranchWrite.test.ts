import { describe, expect, it } from "vitest";
import { blocksFor, decl, sheet } from "./cssRules";

/**
 * Stylesheet invariants of the inline section of the main branch write
 * confirmation (`../../specifications/ui/MBW-main-branch-write.md`
 * MBW-FR-URIL, MBW-FR-IOGF).
 *
 * jsdom computes no layout, so no rendering test can see this.
 */

const last = (selector: string, prop: string) => {
  const found = blocksFor(sheet("components.css"), selector)
    .map((block) => decl(block, prop))
    .filter((value): value is string => value !== null);
  return found.length === 0 ? null : found[found.length - 1];
};

describe("the inline section in a modal (MBW-FR-URIL)", () => {
  it("MBW-FR-URIL, MBW-FR-IOGF: a section between the body and the action row of a modal is inset as the content of the body is", () => {
    // `.modal__body` has 18 px of padding at each side and 14 px between its
    // blocks, so the section lines up with the body content and not the edge.
    expect(last(".modal > .main-branch-write-section", "margin")).toBe("0 18px 14px");
  });
});
