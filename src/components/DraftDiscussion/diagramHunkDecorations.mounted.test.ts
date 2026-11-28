/**
 * A diagram hunk drawn in a real editing surface
 * (`../../../specifications/ui/DCR-draft-change-review.md` DCR-FR-02,
 * DCR-FR-JKVA, DCR-FR-XNPE, DCR-FR-BYSX).
 *
 * These put the widgets where ProseMirror puts them, so what is proven is what
 * ProseMirror does with them: that the old fence stays in the document, that a
 * widget is kept while the review moves, and that its render goes with it.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Editor } from "@tiptap/react";
import { waitFor } from "@testing-library/react";

vi.mock("mermaid", async () => (await import("../../test/mermaidMock")).mermaidModule);

import { markdownExtensions } from "../markdownFidelity";
import { resetMermaidForTest } from "../markdownMermaidRender";
import { fenceStarts } from "../DiagramEditor/fence";
import { mermaidApi, resetMermaidMock } from "../../test/mermaidMock";
import {
  HunkDecorations,
  hunkDecorationKey,
  type DecoratedHunk,
  type HunkDecorationPayload,
} from "./hunkDecorations";

const OLD = "flowchart TD\n  A --> B";
const NEW = "flowchart TD\n  A --> B\n  B --> C";
const BODY = `Intro line.\n\n\`\`\`mermaid\n${OLD}\n\`\`\`\n\nOutro line.`;

const editors: Editor[] = [];

beforeEach(() => {
  resetMermaidForTest();
  resetMermaidMock();
  document.documentElement.removeAttribute("data-theme");
});

afterEach(() => {
  for (const editor of editors.splice(0)) editor.destroy();
  document.body.innerHTML = "";
  document.documentElement.removeAttribute("data-theme");
});

function surface(markdown = BODY): Editor {
  const element = document.createElement("div");
  document.body.append(element);
  const editor = new Editor({
    element,
    extensions: markdownExtensions([HunkDecorations]),
    content: markdown,
  });
  editors.push(editor);
  return editor;
}

function push(editor: Editor, payload: HunkDecorationPayload): void {
  editor.view.dispatch(editor.state.tr.setMeta(hunkDecorationKey, payload));
}

/** The one diagram fence of the document, as the range a placed hunk holds. */
function fenceRange(editor: Editor): { from: number; to: number } {
  const [from] = fenceStarts(editor.state.doc);
  return { from, to: from + (editor.state.doc.nodeAt(from)?.nodeSize ?? 0) };
}

function change(editor: Editor, over: Partial<DecoratedHunk> = {}): DecoratedHunk {
  return {
    id: "h1",
    kind: "replace",
    state: "pending",
    ...fenceRange(editor),
    after: "",
    lost: false,
    position: 1,
    total: 1,
    agent: "Helga",
    editable: false,
    diagram: { oldSource: OLD, newSource: NEW },
    ...over,
  };
}

/** The Markdown the surface would write: what the buffer and the file hold. */
const markdownOf = (editor: Editor): string =>
  (editor.storage as unknown as { markdown: { getMarkdown: () => string } }).markdown.getMarkdown();

const widget = (editor: Editor) =>
  editor.view.dom.querySelector<HTMLElement>(".hunk--diagram[data-hunk]");

describe("a diagram hunk in the prompt's own surface", () => {
  it("DCR-FR-JKVA, DCR-FR-02: a change hides the old fence and draws the new diagram in its place, and the document is not touched", async () => {
    const editor = surface();
    const before = editor.getJSON();
    const markdown = markdownOf(editor);
    push(editor, { hunks: [change(editor)], focused: "h1" });

    const drawn = widget(editor);
    expect(drawn).toHaveClass("hunk--add");
    await waitFor(() => expect(drawn?.querySelector("svg")?.getAttribute("data-source")).toBe(NEW));
    // The widget stands where the fence stood: right before the hidden fence.
    const fence = editor.view.dom.querySelector(".hunk--diagram-old");
    expect(fence).not.toBeNull();
    expect(drawn?.nextElementSibling).toBe(fence);
    expect(editor.getJSON()).toEqual(before);
    expect(markdownOf(editor)).toBe(markdown);
  });

  it("DCR-FR-JKVA: the hunk is drawn with the classes of an added text and a mark in words", () => {
    const editor = surface();
    push(editor, { hunks: [change(editor)], focused: "h1" });
    const drawn = widget(editor);
    expect(drawn?.querySelector(".hunk__gutter")).toHaveTextContent("+");
    expect(drawn?.querySelector(".hunk__tag")).toHaveTextContent("Diagram edit");
    expect(drawn).toHaveAttribute("role", "group");
    expect(drawn).toHaveAttribute(
      "aria-label",
      "Edit 1 of 1, diagram edit proposed by Helga",
    );
  });

  it("DCR-FR-JKVA, DCR-FR-07: a hunk held for discussion is drawn in the held tone, with the new diagram", () => {
    const editor = surface();
    push(editor, { hunks: [change(editor, { state: "discussing" })], focused: "h1" });
    expect(widget(editor)).toHaveClass("hunk--discussing");
    expect(editor.view.dom.querySelector(".hunk--diagram-old")).not.toBeNull();
  });

  it("DCR-FR-XNPE, DCR-FR-24: the widget is kept while the review moves, and it keeps its render", async () => {
    const editor = surface();
    push(editor, { hunks: [change(editor)], focused: "h1" });
    const first = widget(editor);
    await waitFor(() => expect(first?.querySelector("svg")).not.toBeNull());
    const svg = first?.querySelector("svg");
    push(editor, { hunks: [change(editor)], focused: null });
    expect(widget(editor)).toBe(first);
    expect(first?.querySelector("svg")).toBe(svg);
    expect(first?.dataset.focused).toBe("false");
  });

  it("DCR-FR-XNPE: the new diagram renders again in the colours of the new theme", async () => {
    const editor = surface();
    push(editor, { hunks: [change(editor)], focused: "h1" });
    const drawn = widget(editor);
    await waitFor(() => expect(drawn?.querySelector("svg")).toHaveAttribute("data-theme", "default"));
    document.documentElement.dataset.theme = "dark";
    await waitFor(() => expect(drawn?.querySelector("svg")).toHaveAttribute("data-theme", "dark"));
  });

  it("DCR-FR-XNPE: a source that does not render shows its source and the message, as text", async () => {
    const editor = surface();
    push(editor, {
      hunks: [change(editor, { diagram: { oldSource: OLD, newSource: "INVALID <i>x</i>" } })],
      focused: "h1",
    });
    const drawn = widget(editor);
    await waitFor(() => expect(drawn?.querySelector(".mermaid-error")).not.toBeNull());
    expect(drawn?.querySelector(".mermaid-error")).toHaveTextContent("Diagram error");
    expect(drawn?.querySelector(".hunk__diagram-source")?.textContent).toBe("INVALID <i>x</i>");
    expect(drawn?.querySelector("i")).toBeNull();
  });

  it("DCR-FR-XNPE: a widget that is dropped stops following the theme", async () => {
    const editor = surface();
    push(editor, { hunks: [change(editor)], focused: "h1" });
    await waitFor(() => expect(widget(editor)?.querySelector("svg")).not.toBeNull());
    push(editor, { hunks: [change(editor, { state: "accepted" })], focused: null });
    expect(widget(editor)).toBeNull();
    // The fence of the document draws itself again in the dark theme, which is
    // the document's own render. The dropped widget's source is not drawn again.
    const rendersOfNew = () => mermaidApi.render.mock.calls.filter((c) => c[1] === NEW).length;
    const renders = rendersOfNew();
    document.documentElement.dataset.theme = "dark";
    for (let turn = 0; turn < 5; turn += 1) await Promise.resolve();
    expect(rendersOfNew()).toBe(renders);
  });

  it("DCR-FR-JKVA: an accepted or withdrawn hunk draws nothing, and the old fence shows again", () => {
    const editor = surface();
    for (const state of ["accepted", "withdrawn"] as const) {
      push(editor, { hunks: [change(editor, { state })], focused: null });
      expect(widget(editor)).toBeNull();
      expect(editor.view.dom.querySelector(".hunk--diagram-old")).toBeNull();
      expect(editor.view.dom.querySelector("[data-hunk]")).toBeNull();
    }
  });

  it("DCR-FR-JKVA, DCR-FR-07: a rejected change shows the document's own fence in the held tone, with no new diagram", () => {
    const editor = surface();
    push(editor, { hunks: [change(editor, { state: "rejected" })], focused: "h1" });
    expect(widget(editor)).toBeNull();
    expect(editor.view.dom.querySelector(".hunk--diagram-old")).toBeNull();
    const marked = editor.view.dom.querySelector(".hunk--diagram-marked");
    expect(marked).toHaveClass("hunk--rejected");
    expect(marked).toHaveAttribute("data-hunk", "h1");
    const foot = editor.view.dom.querySelector(".hunk--diagram-foot");
    expect(foot).toHaveTextContent("Diagram edit rejected");
    expect(foot).toHaveClass("hunk--del-foot", "hunk--rejected");
  });

  it("DCR-FR-JKVA: a removal marks the old fence and says Diagram removed, and draws no new diagram", () => {
    const editor = surface();
    push(editor, {
      hunks: [change(editor, { kind: "del", diagram: { oldSource: OLD, newSource: null } })],
      focused: "h1",
    });
    expect(widget(editor)).toBeNull();
    const marked = editor.view.dom.querySelector(".hunk--diagram-marked");
    expect(marked).toHaveAttribute("data-hunk", "h1");
    expect(marked).not.toHaveClass("hunk--rejected");
    const foot = editor.view.dom.querySelector<HTMLElement>(".hunk--diagram-foot");
    expect(foot).toHaveTextContent("Diagram removed");
    expect(foot).toHaveAttribute("aria-label", "Edit 1 of 1, diagram removal proposed by Helga");
    // The foot comes after the fence, which holds the space of the chip.
    expect(marked?.nextElementSibling).toBe(foot);
  });

  it("DCR-FR-JKVA, DCR-FR-TSNW: an addition draws the new diagram at the insertion, between the document's blocks", () => {
    const editor = surface();
    const at = BODY.indexOf("Intro line.") + "Intro line.".length;
    const from = editor.state.doc.resolve(1).end();
    push(editor, {
      hunks: [
        {
          ...change(editor, { kind: "add", diagram: { oldSource: null, newSource: NEW } }),
          from,
          to: from,
        },
      ],
      focused: "h1",
    });
    expect(at).toBeGreaterThan(0);
    const drawn = widget(editor);
    expect(drawn?.parentElement).toBe(editor.view.dom);
    expect(drawn?.previousElementSibling?.tagName).toBe("P");
    expect(drawn?.querySelector(".hunk__tag")).toHaveTextContent("Diagram addition");
    expect(editor.view.dom.querySelector(".hunk--diagram-old")).toBeNull();
  });

  it("DCR-FR-BYSX: a hunk whose range is not a diagram fence draws nothing", () => {
    const editor = surface();
    push(editor, { hunks: [change(editor, { from: 1, to: 5 })], focused: "h1" });
    expect(widget(editor)).toBeNull();
    expect(editor.view.dom.querySelector(".hunk--diagram-old")).toBeNull();
  });

  it("DCR-FR-WTCQ: nothing in a diagram hunk takes an edit", () => {
    const editor = surface();
    const onEdit = vi.fn();
    push(editor, { hunks: [change(editor)], focused: "h1", onEdit });
    expect(widget(editor)?.querySelector("[contenteditable='true'], textarea, input")).toBeNull();
    expect(onEdit).not.toHaveBeenCalled();
  });
});
