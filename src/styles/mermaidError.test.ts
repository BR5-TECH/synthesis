import { describe, expect, it } from "vitest";

import { readStylesheet } from "../test/readStylesheet";

/** The declarations of the one rule whose selector is exactly `selector`. */
function ruleBody(css: string, selector: string): string {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const matches = Array.from(css.matchAll(new RegExp(`(?:^|[}/])\\s*${escaped}\\s*\\{([^}]*)\\}`, "g")));
  expect(matches).toHaveLength(1);
  return matches[0][1];
}

describe("the reason of a diagram error (EDT-FR-VIOP)", () => {
  const css = readStylesheet("components.css");

  it("EDT-FR-VIOP: the reason sets in the Source code role and keeps its line breaks", () => {
    const body = ruleBody(css, ".doc .mermaid-error__detail");
    expect(body).toMatch(/font-family\s*:\s*var\(--font-source-family\)/);
    expect(body).toMatch(/font-size\s*:\s*var\(--fs-src-[a-z0-9]+\)/);
    expect(body).toMatch(/line-height\s*:\s*var\(--font-source-line-height\)/);
    expect(body).toMatch(/white-space\s*:\s*pre-wrap/);
  });
});
