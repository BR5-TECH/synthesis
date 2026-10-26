import { afterEach, describe, expect, it } from "vitest";
import { Editor } from "@tiptap/react";

import { markdownExtensions } from "../markdownFidelity";
import { fenceStarts, placeAt, relocate, writeSource } from "./fence";

let editor: Editor | null = null;

function docOf(content: string) {
  editor = new Editor({ extensions: markdownExtensions(), content });
  return editor;
}

afterEach(() => {
  editor?.destroy();
  editor = null;
});

const TWO = "```mermaid\ngraph TD\nA-->B\n```\n\n```mermaid\ngraph LR\nX-->Y\n```\n";

describe("the fence that the diagram editor overlay edits", () => {
  it("EDT-FR-SHYX: a whole-document change with as many fences keeps the fence at its place", () => {
    const ed = docOf(TWO);
    const second = fenceStarts(ed.state.doc)[1];
    const place = placeAt(ed.state.doc, second)!;
    ed.commands.setContent("# New\n\n" + TWO, { emitUpdate: false });
    const next = relocate(ed.state.doc, place, "graph LR\nX-->Y")!;
    expect(next.ordinal).toBe(1);
    expect(next.pos).toBe(fenceStarts(ed.state.doc)[1]);
  });

  it("EDT-FR-SHYX: when another fence goes, the fence is found by its source", () => {
    const ed = docOf(TWO);
    const place = placeAt(ed.state.doc, fenceStarts(ed.state.doc)[1])!;
    ed.commands.setContent("```mermaid\ngraph LR\nX-->Y\n```\n", { emitUpdate: false });
    const next = relocate(ed.state.doc, place, "graph LR\nX-->Y")!;
    expect(next).toEqual({ pos: fenceStarts(ed.state.doc)[0], ordinal: 0, count: 1 });
  });

  it("EDT-FR-SHYX: a fence that went is not found", () => {
    const ed = docOf(TWO);
    const place = placeAt(ed.state.doc, fenceStarts(ed.state.doc)[1])!;
    ed.commands.setContent("```mermaid\ngraph TD\nA-->B\n```\n", { emitUpdate: false });
    expect(relocate(ed.state.doc, place, "graph LR\nX-->Y")).toBeNull();
  });

  it("EDT-FR-SHYX: a block that is no longer a diagram fence is not one to edit", () => {
    const ed = docOf("```mermaid\ngraph TD\nA-->B\n```\n");
    const start = fenceStarts(ed.state.doc)[0];
    ed.commands.setContent("```js\ngraph TD\nA-->B\n```\n", { emitUpdate: false });
    expect(placeAt(ed.state.doc, start)).toBeNull();
    expect(writeSource(ed.view, start, "x")).toBe(false);
  });

  it("EDT-FR-EBEZ: an edit replaces only the part of the source that differs", () => {
    const ed = docOf(TWO);
    const start = fenceStarts(ed.state.doc)[0];
    expect(writeSource(ed.view, start, "graph TD\nA-->C")).toBe(true);
    expect(ed.state.doc.nodeAt(start)?.textContent).toBe("graph TD\nA-->C");
    expect(ed.state.doc.nodeAt(fenceStarts(ed.state.doc)[1])?.textContent).toBe("graph LR\nX-->Y");
  });
});
