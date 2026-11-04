/**
 * CMT-FR-WJED: a rich-body selection over the markup of HTML comments maps
 * onto the source on the terms of CMT-FR-06.
 */
import { afterEach, describe, expect, it } from "vitest";
import { Editor } from "@tiptap/react";
import { markdownExtensions } from "../markdownFidelity";
import { TextSelection } from "@tiptap/pm/state";
import { bodyOffsetOfSelection, sourceRange } from "./selection";

let editor: Editor | null = null;

afterEach(() => {
  const element = editor?.view.dom.parentElement;
  editor?.destroy();
  element?.remove();
  editor = null;
});

/** The text a browser selection over the whole rendered body copies. */
function renderedText(content: string): string {
  const element = document.createElement("div");
  document.body.appendChild(element);
  editor = new Editor({ element, extensions: markdownExtensions(), content });
  const range = document.createRange();
  range.selectNodeContents(editor.view.dom);
  return range.toString();
}

describe("CMT-FR-WJED: selections over markup", () => {
  it("CMT-FR-WJED: the position hint counts a chip as its raw comment, as the DOM text does", () => {
    renderedText("A <!-- a long comment --> B");
    const ed = editor!;
    ed.view.dispatch(ed.state.tr.setSelection(TextSelection.create(ed.state.doc, ed.state.doc.content.size - 1)));
    expect(bodyOffsetOfSelection(ed, null)).toBe("A <!-- a long comment --> B".length);
  });

  it("CMT-FR-WJED: a chip contributes its full comment text to the selection", () => {
    expect(renderedText("Before <!-- a note --> after.")).toBe("Before <!-- a note --> after.");
  });

  it("CMT-FR-WJED: a selection inside one block that holds an inline chip is found as it stands", () => {
    const source = "Intro.\n\nBefore <!-- a note --> after.\n";
    const text = "Before <!-- a note --> after";
    const range = sourceRange(source, text, 8);
    expect(range).not.toBeNull();
    expect(source.slice(range!.start, range!.end)).toBe(text);
  });

  it("CMT-FR-WJED: a selection across a spacer and an aligned block includes the break and marker lines", () => {
    const source = "Intro text.\n\n<!-- %% -->\n<!-- center -->\nTitle line\n\nEnd.\n";
    // A browser joins the blocks with line breaks; the spacer and the marker
    // show no text.
    const range = sourceRange(source, "text.\n\n\nTitle line", 0);
    expect(range).not.toBeNull();
    expect(source.slice(range!.start, range!.end)).toBe(
      "text.\n\n<!-- %% -->\n<!-- center -->\nTitle line",
    );
  });

  it("CMT-FR-WJED: a selection across a block chip includes the whole comment", () => {
    const source = "First part.\n\n<!--\n  a long\n  note\n-->\n\nSecond part.\n";
    const range = sourceRange(source, "part.\n<!--\n  a long\n  note\n-->\nSecond", 0);
    expect(range).not.toBeNull();
    expect(source.slice(range!.start, range!.end)).toBe(
      "part.\n\n<!--\n  a long\n  note\n-->\n\nSecond",
    );
  });
});
