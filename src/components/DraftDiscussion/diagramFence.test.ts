/**
 * DCR-FR-JKVA / DCR-FR-ESIY: the source of the diagram in the fence text of a
 * diagram hunk.
 */
import { describe, expect, it } from "vitest";

import { diagramSourceOf } from "./diagramFence";

describe("diagramSourceOf", () => {
  it("DCR-FR-JKVA: reads the source between backtick fences", () => {
    expect(diagramSourceOf("```mermaid\nflowchart TD\n  A --> B\n```")).toBe(
      "flowchart TD\n  A --> B",
    );
  });

  it("DCR-FR-JKVA: reads the source between tilde fences", () => {
    expect(diagramSourceOf("~~~mermaid\nflowchart TD\n  A --> B\n~~~")).toBe(
      "flowchart TD\n  A --> B",
    );
  });

  it("DCR-FR-JKVA: removes the fence indent from each line of the source", () => {
    const text = "  ```mermaid\n  flowchart TD\n    A --> B\n  ```";
    expect(diagramSourceOf(text)).toBe("flowchart TD\n  A --> B");
  });

  it("DCR-FR-JKVA: keeps the source of a fence that has no closing fence", () => {
    expect(diagramSourceOf("```mermaid\nflowchart TD\n  A --> B")).toBe("flowchart TD\n  A --> B");
  });

  it("DCR-FR-JKVA: reads CRLF line breaks as one line break each", () => {
    expect(diagramSourceOf("```mermaid\r\nflowchart TD\r\n  A --> B\r\n```\r\n")).toBe(
      "flowchart TD\n  A --> B",
    );
  });

  it("DCR-FR-JKVA: drops the line breaks that follow the fence of a removal", () => {
    expect(diagramSourceOf("```mermaid\nflowchart TD\n```\n\n")).toBe("flowchart TD");
    expect(diagramSourceOf("```mermaid\nflowchart TD\n```\n")).toBe("flowchart TD");
  });

  it("DCR-FR-JKVA: reads the fence of an addition that starts with blank lines", () => {
    expect(diagramSourceOf("\n\n```mermaid\nflowchart TD\n```")).toBe("flowchart TD");
  });

  it("DCR-FR-JKVA: keeps a blank line inside the source and a longer fence", () => {
    expect(diagramSourceOf("````mermaid\nflowchart TD\n\n  A --> B\n````")).toBe(
      "flowchart TD\n\n  A --> B",
    );
  });

  it("DCR-FR-JKVA: a closing fence shorter than the opening one is source", () => {
    expect(diagramSourceOf("````mermaid\nflowchart TD\n```")).toBe("flowchart TD\n```");
  });

  it("DCR-FR-JKVA: an empty fence has an empty source", () => {
    expect(diagramSourceOf("```mermaid\n```")).toBe("");
  });

  it("DCR-FR-JKVA: a text that is not the fence of a Mermaid diagram has no source", () => {
    expect(diagramSourceOf("")).toBeNull();
    expect(diagramSourceOf(null)).toBeNull();
    expect(diagramSourceOf("plain text")).toBeNull();
    expect(diagramSourceOf("```js\nlet a = 1;\n```")).toBeNull();
    expect(diagramSourceOf("```mermaidish\nA\n```")).toBeNull();
  });
});
