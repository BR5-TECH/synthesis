import { describe, expect, it } from "vitest";

import { readStylesheet } from "../test/readStylesheet";

describe("the CALM canvas inside the rich surfaces", () => {
  const css = readStylesheet("components.css");

  it("CLM-FR-ERXV, CAD-FR-IPRR, CAD-FR-KUYJ: the fit rules of a Mermaid drawing do not shrink the edge layer of a CALM canvas", () => {
    const match = css.match(
      /\.doc \.calm-canvas svg,\s*\.diagram-editor__preview \.calm-canvas svg\s*\{([^}]*)\}/,
    );
    expect(match).not.toBeNull();
    expect(match![1]).toMatch(/max-width\s*:\s*none/);
  });
});
