import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { CODE_GLYPH, DIAGRAM_GLYPH, iconElement } from "./domIcons";
import { Icon } from "./icons";

describe("icons drawn outside React (EDT-FR-UTHM)", () => {
  it("EDT-FR-UTHM: the code glyph is the glyph of the editing-mode toggle", () => {
    const { container } = render(<Icon.Code />);
    expect(container.querySelector("path")?.getAttribute("d")).toBe(CODE_GLYPH);
  });

  it("EDT-FR-UTHM: an icon element has the look of the React icon set and is hidden from assistive technology", () => {
    const { container } = render(<Icon.Code size={15} />);
    const react = container.querySelector("svg")!;
    const dom = iconElement(DIAGRAM_GLYPH, 15);
    expect(dom.namespaceURI).toBe(react.namespaceURI);
    for (const name of [
      "class",
      "viewBox",
      "width",
      "height",
      "fill",
      "stroke",
      "stroke-width",
      "stroke-linecap",
      "stroke-linejoin",
    ]) {
      expect(dom.getAttribute(name)).toBe(react.getAttribute(name));
    }
    expect(dom.getAttribute("aria-hidden")).toBe("true");
    expect(dom.querySelector("path")?.getAttribute("d")).toBe(DIAGRAM_GLYPH);
  });
});
