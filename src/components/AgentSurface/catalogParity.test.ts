import { describe, expect, it } from "vitest";

import catalog from "../../../resources/a2ui/discussion-catalog.json";
import { FUNCTION_NAMES } from "./functions";
import { RENDERERS } from "./render";

describe("the panel and the discussion catalog", () => {
  it("ASF-FR-HRQL: the renderer draws exactly the components of the catalog", () => {
    expect(Object.keys(RENDERERS).sort()).toEqual(Object.keys(catalog.components).sort());
  });

  it("ASF-FR-GHUZ: the panel evaluates exactly the functions of the catalog", () => {
    expect([...FUNCTION_NAMES].sort()).toEqual(Object.keys(catalog.functions).sort());
  });
});
