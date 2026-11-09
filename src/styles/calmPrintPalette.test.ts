import { describe, expect, it } from "vitest";

import { readStylesheet } from "../test/readStylesheet";
import { PRINT_TOKENS } from "../components/CalmDiagram/printPalette";

describe("the colours of the CALM print drawing", () => {
  const css = readStylesheet("colors_and_type.css");
  const light = css.match(/:root,\s*\[data-theme="light"\]\s*\{([^}]*)\}/);

  it.each(Object.entries(PRINT_TOKENS))(
    "CAD-FR-FONB: %s is the value of the light theme",
    (token, value) => {
      expect(light).not.toBeNull();
      const declared = light![1].match(new RegExp(`${token}:\\s*([^;]+);`));
      expect(declared, token).not.toBeNull();
      const normal = (text: string) => text.replace(/\s+/g, "").toUpperCase();
      expect(normal(declared![1])).toBe(normal(value));
    },
  );
});
