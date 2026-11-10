import { describe, expect, it } from "vitest";

import { diagramKindOfLanguage } from "./diagramKind";

describe("the kind of a diagram fence", () => {
  it("CAD-FR-PFNM, EDT-FR-MOYS: `calm` and `mermaid` are compared trimmed and exact", () => {
    expect(diagramKindOfLanguage("calm")).toBe("calm");
    expect(diagramKindOfLanguage(" calm ")).toBe("calm");
    expect(diagramKindOfLanguage("mermaid")).toBe("mermaid");
    for (const other of ["CALM", "calmx", "json", "", null, undefined, 3]) {
      expect(diagramKindOfLanguage(other)).toBeNull();
    }
  });
});
