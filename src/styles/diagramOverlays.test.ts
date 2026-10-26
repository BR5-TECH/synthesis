import { describe, expect, it } from "vitest";

import { readStylesheet } from "../test/readStylesheet";

/** The declarations of the one rule whose selector is exactly `selector`. */
function ruleBody(css: string, selector: string): string {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const matches = Array.from(css.matchAll(new RegExp(`(?:^|[}/])\\s*${escaped}\\s*\\{([^}]*)\\}`, "g")));
  expect(matches).toHaveLength(1);
  return matches[0][1];
}

describe("the layout of the diagram overlays", () => {
  const css = readStylesheet("components.css");

  it("EDT-FR-NKGX, EDT-FR-ULPU: the hints area takes the bottom 20% of the whole overlay, and the title bar and the panes the rest", () => {
    // The overlay is a flex column whose border is inside its size, so 20% of
    // the column is 20% of the overlay.
    const overlay = ruleBody(css, ".modal.diagram-overlay");
    expect(overlay).toMatch(/display\s*:\s*flex/);
    expect(overlay).toMatch(/flex-direction\s*:\s*column/);
    expect(overlay).toMatch(/box-sizing\s*:\s*border-box/);
    const hints = ruleBody(css, ".diagram-hints");
    expect(hints).toMatch(/flex\s*:\s*0 0 20%/);
    expect(hints).toMatch(/box-sizing\s*:\s*border-box/);
    expect(ruleBody(css, ".diagram-editor__panes")).toMatch(/flex\s*:\s*1/);
    expect(ruleBody(css, ".diagram-overlay__head")).toMatch(/flex\s*:\s*none/);
  });

  it("EDT-FR-FKUH: the hints area scrolls, and its text can be selected", () => {
    const body = ruleBody(css, ".diagram-hints");
    expect(body).toMatch(/overflow\s*:\s*auto/);
    expect(body).toMatch(/(?:^|[\s;])user-select\s*:\s*text/);
  });

  it("EDT-FR-NVGA: a snippet is code text in the Source code role, an explanation in the UI role", () => {
    const code = ruleBody(css, ".diagram-hints__code code");
    expect(code).toMatch(/font-family\s*:\s*var\(--font-source-family\)/);
    expect(code).toMatch(/line-height\s*:\s*var\(--font-source-line-height\)/);
    const area = ruleBody(css, ".diagram-hints");
    expect(area).toMatch(/font-family\s*:\s*var\(--font-ui-family\)/);
  });

  it("EDT-FR-VWNJ, EDT-FR-LJAP: the viewer area clips the drawing, which its transform places", () => {
    expect(ruleBody(css, ".diagram-viewer__area.doc")).toMatch(/overflow\s*:\s*hidden/);
    expect(ruleBody(css, ".diagram-viewer__area .diagram-viewer__stage")).toMatch(
      /transform-origin\s*:\s*0 0/,
    );
  });
});
