import { describe, expect, it } from "vitest";

import { FALLBACK_SET_KEY, HINT_SETS, declarationLine, hintSetFor, hintsFor } from "./index";

describe("the chart type of a source (EDT-FR-NWZX)", () => {
  it("EDT-FR-NWZX: the first line that is not empty and not a comment declares it", () => {
    expect(declarationLine("\n\n  %% a note\n%%{init: {}}%%\n  sequenceDiagram\n A->>B: hi")).toBe(
      "sequenceDiagram",
    );
    expect(hintSetFor("graph TD\nA-->B")?.key).toBe("flowchart");
    expect(hintSetFor("flowchart LR\nA-->B")?.key).toBe("flowchart");
    expect(hintSetFor("classDiagram\nclass A")?.key).toBe("class");
    expect(hintSetFor("stateDiagram-v2\n[*] --> A")?.key).toBe("state");
    expect(hintSetFor("erDiagram\nA ||--o{ B : has")?.key).toBe("er");
  });

  it("EDT-FR-NWZX: a frontmatter block at the start is skipped", () => {
    const source = "---\ntitle: gantt\nconfig:\n  theme: dark\n---\n%% c\npie title Pets\n";
    expect(declarationLine(source)).toBe("pie title Pets");
    expect(hintSetFor(source)?.key).toBe("pie");
  });

  it("EDT-FR-NWZX: a frontmatter block that does not close holds the rest of the source", () => {
    expect(declarationLine("---\ntitle: x\ngantt\n")).toBeNull();
    expect(hintSetFor("---\ntitle: x\ngantt\n")).toBeNull();
  });

  it("EDT-FR-NWZX: a word that only starts like a chart type is not that type", () => {
    expect(hintSetFor("pies are good")).toBeNull();
    expect(hintSetFor("graphical")).toBeNull();
    expect(hintSetFor("blockquote")).toBeNull();
  });
});

describe("the hints for a source (EDT-FR-ERTS, EDT-FR-YNQF)", () => {
  it("EDT-FR-YNQF: a known chart type shows its own set under its name, with its example", () => {
    const shown = hintsFor("sequenceDiagram\nA->>B: hi");
    const set = HINT_SETS.find((each) => each.key === "sequence")!;
    expect(shown.title).toBe(set.name);
    expect(shown.key).toBe("sequence");
    expect(shown.groups.map((group) => group.title)).toEqual([
      ...set.groups.map((group) => group.title),
      "Example",
    ]);
    expect(shown.groups[shown.groups.length - 1]?.hints[0].code).toBe(set.example);
  });

  it("EDT-FR-ERTS: an empty source shows the general hints and then the flowchart set", () => {
    for (const source of ["", "  \n\t\n", "%% only a comment"]) {
      const shown = hintsFor(source);
      expect(shown.key).toBeNull();
      expect(shown.title).toBe("General hints");
      const hints = shown.groups.flatMap((group) => group.hints);
      expect(hints.some((hint) => hint.code.startsWith("%%"))).toBe(true);
      expect(hints.some((hint) => hint.code.startsWith("---"))).toBe(true);
      const flowchart = HINT_SETS.find((each) => each.key === FALLBACK_SET_KEY)!;
      expect(shown.groups[shown.groups.length - 1]?.hints[0].code).toBe(flowchart.example);
      expect(shown.groups.some((group) => group.title === `${flowchart.name}: Example`)).toBe(
        true,
      );
    }
  });

  it("EDT-FR-ERTS: an unknown chart type shows the general hints with the declaration of each chart type", () => {
    const shown = hintsFor("notADiagram\nA --> B");
    expect(shown.key).toBeNull();
    const types = shown.groups.find((group) => group.title === "Chart types");
    expect(types?.hints.map((hint) => hint.code)).toEqual(
      HINT_SETS.map((set) => set.declaration.code),
    );
  });
});
