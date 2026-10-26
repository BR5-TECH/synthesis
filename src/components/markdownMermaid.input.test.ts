import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Editor, type AnyExtension } from "@tiptap/react";
import { TextSelection } from "@tiptap/pm/state";

vi.mock("mermaid", async () => (await import("../test/mermaidMock")).mermaidModule);
vi.mock("../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
}));

import { resetMermaidMock } from "../test/mermaidMock";
import { CommentHighlight, commentHighlightKey } from "./commentHighlight";
import { FindHighlight, findHighlightKey } from "./findHighlight";
import { getMarkdown, markdownExtensions } from "./markdownFidelity";
import {
  DIAGRAM_LAYOUT_EVENT,
  SHOW_DIAGRAM_LABEL,
  SHOW_SOURCE_LABEL,
  diagramTopAt,
  fragmentTop,
} from "./markdownMermaid";
import { resetMermaidForTest } from "./markdownMermaidRender";

// "Intro" is a paragraph at 0..7; the fence starts at 7 and its text at 8.
const MD = "Intro\n\n```mermaid\ngraph TD\nA-->B\n```\n\nOutro";
const FENCE_POS = 7;
const IN_FENCE = 10;

let editors: Editor[] = [];
let hosts: HTMLElement[] = [];

/** A surface attached to the page, so it can take focus. */
function mount(content = MD, editable = true, trailingNode = true): Editor {
  const host = document.createElement("div");
  document.body.appendChild(host);
  hosts.push(host);
  const extensions = markdownExtensions([FindHighlight, CommentHighlight]).map((extension) =>
    extension.name === "starterKit" && !trailingNode
      ? (extension as AnyExtension).configure({ trailingNode: false })
      : extension,
  );
  const ed = new Editor({ element: host, extensions, content, editable });
  editors.push(ed);
  return ed;
}

const pre = (ed: Editor) => ed.view.dom.querySelector("pre")!;
const viewOf = (ed: Editor) => pre(ed).getAttribute("data-diagram-view");
const toggleOf = (ed: Editor) => pre(ed).querySelector<HTMLButtonElement>(".mermaid-toggle");
const figureOf = (ed: Editor) => pre(ed).querySelector<HTMLElement>(".mermaid-diagram");

async function rendered(ed: Editor): Promise<void> {
  await vi.waitFor(() => expect(viewOf(ed)).toBe("diagram"));
}

function key(target: HTMLElement, key: string): void {
  target.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }));
}

function select(ed: Editor, pos: number): void {
  ed.view.dispatch(ed.state.tr.setSelection(TextSelection.create(ed.state.doc, pos)));
}

function setFind(ed: Editor, ranges: { from: number; to: number }[], current: number): void {
  const tr = ed.state.tr.setMeta(findHighlightKey, { ranges, current });
  tr.setMeta("addToHistory", false);
  ed.view.dispatch(tr);
}

beforeEach(() => {
  resetMermaidForTest();
  resetMermaidMock();
});

afterEach(() => {
  for (const ed of editors) ed.destroy();
  for (const host of hosts) host.remove();
  editors = [];
  hosts = [];
});

describe("editing the source in place (EDT-FR-KQXZ)", () => {
  it("EDT-FR-KQXZ: a click on the diagram shows the source and puts the caret in it", async () => {
    const ed = mount();
    await rendered(ed);
    const doc = ed.state.doc;
    figureOf(ed)!.click();
    expect(viewOf(ed)).toBe("source");
    expect(ed.state.selection.from).toBe(FENCE_POS + 1);
    expect(ed.view.hasFocus()).toBe(true);
    expect(ed.state.doc).toBe(doc);
  });

  it("EDT-FR-KQXZ: Enter and Space on the focused diagram do the same and type nothing", async () => {
    for (const pressed of ["Enter", " "]) {
      const ed = mount();
      await rendered(ed);
      figureOf(ed)!.focus();
      key(figureOf(ed)!, pressed);
      expect(viewOf(ed)).toBe("source");
      expect(ed.state.selection.from).toBe(FENCE_POS + 1);
      expect(getMarkdown(ed)).toBe(MD);
    }
  });

  it("EDT-FR-KQXZ: another key on the focused diagram does nothing", async () => {
    const ed = mount();
    await rendered(ed);
    key(figureOf(ed)!, "a");
    expect(viewOf(ed)).toBe("diagram");
  });
});

describe("the view toggle and the keyboard (EDT-FR-USWS)", () => {
  it("EDT-FR-USWS: the focus stays on the toggle across a change of view", async () => {
    const ed = mount();
    await rendered(ed);
    toggleOf(ed)!.focus();
    toggleOf(ed)!.click();
    expect(document.activeElement).toBe(toggleOf(ed));
    expect(document.activeElement?.getAttribute("aria-label")).toBe(SHOW_DIAGRAM_LABEL);
    toggleOf(ed)!.click();
    expect(document.activeElement).toBe(toggleOf(ed));
    expect(document.activeElement?.getAttribute("aria-label")).toBe(SHOW_SOURCE_LABEL);
  });

  it("EDT-FR-USWS: showing the diagram moves the caret out of the source", async () => {
    const ed = mount();
    await rendered(ed);
    figureOf(ed)!.click();
    select(ed, IN_FENCE);
    toggleOf(ed)!.click();
    expect(viewOf(ed)).toBe("diagram");
    const end = FENCE_POS + ed.state.doc.nodeAt(FENCE_POS)!.nodeSize;
    expect(ed.state.selection.from).toBeGreaterThanOrEqual(end);
  });

  it("EDT-FR-USWS, EDT-FR-IPMX: showing the diagram moves the caret past a next fence that shows its diagram", async () => {
    const ed = mount("Intro\n\n```mermaid\ngraph TD\n```\n\n```mermaid\npie\n```\n\nOutro");
    await vi.waitFor(() => {
      const views = [...ed.view.dom.querySelectorAll("pre")].map((p) =>
        p.getAttribute("data-diagram-view"),
      );
      expect(views).toEqual(["diagram", "diagram"]);
    });
    figureOf(ed)!.click();
    select(ed, IN_FENCE);
    toggleOf(ed)!.click();
    expect(viewOf(ed)).toBe("diagram");
    const { $from } = ed.state.selection;
    expect($from.parent.type.name).toBe("paragraph");
    expect($from.parent.textContent).toBe("Outro");
  });

  it("EDT-FR-USWS: a fence that is the last block moves the caret before it", async () => {
    const ed = mount("Intro\n\n```mermaid\ngraph TD\n```", true, false);
    await rendered(ed);
    expect(ed.state.doc.lastChild?.type.name).toBe("codeBlock");
    figureOf(ed)!.click();
    select(ed, IN_FENCE);
    toggleOf(ed)!.click();
    expect(viewOf(ed)).toBe("diagram");
    expect(ed.state.selection.from).toBeLessThanOrEqual(FENCE_POS);
  });

  it("EDT-FR-USWS: a fence that is the only block is selected whole rather than holding a hidden caret", async () => {
    const ed = mount("```mermaid\ngraph TD\n```", true, false);
    await rendered(ed);
    figureOf(ed)!.click();
    toggleOf(ed)!.click();
    expect(viewOf(ed)).toBe("diagram");
    expect(ed.state.selection.from).toBe(0);
    expect(ed.state.selection.to).toBe(ed.state.doc.firstChild!.nodeSize);
  });
});

describe("the keyboard peek (EDT-FR-UYEN)", () => {
  it("EDT-FR-UYEN: a caret the keyboard moves into the fence shows the source until it leaves", async () => {
    const ed = mount();
    await rendered(ed);
    ed.view.focus();
    key(ed.view.dom, "ArrowDown");
    select(ed, IN_FENCE);
    expect(viewOf(ed)).toBe("source");
    expect(toggleOf(ed)?.getAttribute("aria-label")).toBe(SHOW_DIAGRAM_LABEL);
    key(ed.view.dom, "ArrowDown");
    select(ed, ed.state.doc.content.size - 1);
    expect(viewOf(ed)).toBe("diagram");
  });

  it("EDT-FR-UYEN: a selection set without the keyboard leaves the diagram", async () => {
    const ed = mount();
    await rendered(ed);
    ed.view.focus();
    select(ed, IN_FENCE);
    expect(viewOf(ed)).toBe("diagram");
  });

  it("EDT-FR-UYEN: leaving the surface ends the keyboard peek", async () => {
    const ed = mount();
    await rendered(ed);
    ed.view.focus();
    key(ed.view.dom, "ArrowDown");
    select(ed, IN_FENCE);
    expect(viewOf(ed)).toBe("source");
    const outside = document.createElement("button");
    document.body.appendChild(outside);
    outside.focus();
    ed.view.dom.dispatchEvent(new FocusEvent("blur"));
    expect(viewOf(ed)).toBe("diagram");
    outside.remove();
  });

  it("EDT-FR-UYEN: a click into a source the keyboard revealed keeps it showing", async () => {
    const ed = mount();
    await rendered(ed);
    ed.view.focus();
    key(ed.view.dom, "ArrowDown");
    select(ed, IN_FENCE);
    expect(viewOf(ed)).toBe("source");
    // jsdom has no hit testing; the view's own mousedown handler asks for it.
    const doc = document as unknown as { elementFromPoint?: unknown };
    const saved = doc.elementFromPoint;
    doc.elementFromPoint = () => null;
    try {
      pre(ed)
        .querySelector("code")!
        .dispatchEvent(new MouseEvent("mousedown", { bubbles: true, cancelable: true }));
    } finally {
      doc.elementFromPoint = saved;
    }
    select(ed, IN_FENCE + 1);
    expect(viewOf(ed)).toBe("source");
  });

  it("EDT-FR-MOYS: a click into an empty fence does not keep it on its source once it has text", async () => {
    const ed = mount("Intro\n\n```mermaid\n```\n\nOutro");
    expect(viewOf(ed)).toBe("source");
    const doc = document as unknown as { elementFromPoint?: unknown };
    const saved = doc.elementFromPoint;
    doc.elementFromPoint = () => null;
    try {
      pre(ed)
        .querySelector("code")!
        .dispatchEvent(new MouseEvent("mousedown", { bubbles: true, cancelable: true }));
    } finally {
      doc.elementFromPoint = saved;
    }
    ed.view.dispatch(ed.state.tr.insertText("graph TD", FENCE_POS + 1));
    select(ed, 2);
    await rendered(ed);
  });

  it("EDT-FR-USWS: showing the diagram during a keyboard peek ends it", async () => {
    const ed = mount();
    await rendered(ed);
    ed.view.focus();
    key(ed.view.dom, "ArrowDown");
    select(ed, IN_FENCE);
    toggleOf(ed)!.click();
    expect(viewOf(ed)).toBe("diagram");
  });
});

describe("the Find peek (EFR-FR-DGBG)", () => {
  const inFence = { from: 17, to: 22 };
  const outside = { from: 1, to: 6 };

  it("EFR-FR-DGBG, EDT-FR-BQIU: a fence that holds the current match shows its source, with no toggle, and nothing else changes", async () => {
    const ed = mount();
    await rendered(ed);
    const doc = ed.state.doc;
    setFind(ed, [outside, inFence], 1);
    expect(viewOf(ed)).toBe("source");
    expect(toggleOf(ed)).toBeNull();
    expect(pre(ed).querySelector(".find-match--current")?.textContent).toBe("A-->B");
    expect(ed.state.doc).toBe(doc);
  });

  it("EFR-FR-DGBG: the fence shows its diagram again when the match moves on or the panel closes", async () => {
    const ed = mount();
    await rendered(ed);
    setFind(ed, [outside, inFence], 1);
    setFind(ed, [outside, inFence], 0);
    expect(viewOf(ed)).toBe("diagram");
    setFind(ed, [outside, inFence], 1);
    setFind(ed, [], -1);
    expect(viewOf(ed)).toBe("diagram");
  });

  it("EFR-FR-DGBG: a match in the fence that is not current reveals nothing", async () => {
    const ed = mount();
    await rendered(ed);
    setFind(ed, [inFence], -1);
    expect(viewOf(ed)).toBe("diagram");
  });

  it("EDT-FR-LEUI: an inert surface never peeks", async () => {
    const ed = mount(MD, false);
    await rendered(ed);
    setFind(ed, [inFence], 0);
    expect(viewOf(ed)).toBe("diagram");
  });

  it("EFR-FR-DGBG, EDT-FR-USWS: a fence set to its source keeps it after the Find peek", async () => {
    const ed = mount();
    await rendered(ed);
    toggleOf(ed)!.click();
    setFind(ed, [inFence], 0);
    setFind(ed, [], -1);
    expect(viewOf(ed)).toBe("source");
    expect(toggleOf(ed)?.getAttribute("aria-label")).toBe(SHOW_DIAGRAM_LABEL);
  });
});

describe("comment fragments (EDT-FR-UYEN)", () => {
  function setComments(ed: Editor, ranges: { threadId: string; from: number; to: number }[]) {
    const tr = ed.state.tr.setMeta(commentHighlightKey, { ranges, focused: null });
    tr.setMeta("addToHistory", false);
    ed.view.dispatch(tr);
  }

  it("EDT-FR-UYEN: a fragment in a fence that shows its diagram marks the whole diagram", async () => {
    const ed = mount();
    await rendered(ed);
    setComments(ed, [{ threadId: "t1", from: 17, to: 22 }]);
    expect(pre(ed).getAttribute("data-diagram-commented")).toBe("true");
    setComments(ed, [{ threadId: "t1", from: 1, to: 6 }]);
    expect(pre(ed).hasAttribute("data-diagram-commented")).toBe(false);
  });

  it("EDT-FR-UYEN: a card aligns with the top of a diagram, and with the text otherwise", async () => {
    const ed = mount();
    await rendered(ed);
    vi.spyOn(pre(ed), "getBoundingClientRect").mockReturnValue({ top: 123 } as DOMRect);
    expect(diagramTopAt(ed.view, IN_FENCE)).toBe(123);
    expect(diagramTopAt(ed.view, 2)).toBeNull();
    expect(diagramTopAt(ed.view, FENCE_POS)).toBeNull();
    expect(diagramTopAt(ed.view, -1)).toBeNull();
    expect(diagramTopAt(ed.view, ed.state.doc.content.size + 1)).toBeNull();
    expect(fragmentTop(ed.view, IN_FENCE)).toBe(123);
    toggleOf(ed)!.click();
    expect(diagramTopAt(ed.view, IN_FENCE)).toBeNull();
    // In the source view the card aligns with the text, as anywhere else.
    vi.spyOn(ed.view, "coordsAtPos").mockReturnValue({ top: 45, bottom: 50, left: 0, right: 0 });
    expect(fragmentTop(ed.view, IN_FENCE)).toBe(45);
    expect(fragmentTop(ed.view, 2)).toBe(45);
  });

  it("EDT-FR-UYEN: the surface reports each change of fence height, so the cards align again", async () => {
    const ed = mount();
    const heard = vi.fn();
    ed.view.dom.addEventListener(DIAGRAM_LAYOUT_EVENT, heard);
    await rendered(ed);
    await vi.waitFor(() => expect(heard).toHaveBeenCalledTimes(1));
    toggleOf(ed)!.click();
    expect(heard).toHaveBeenCalledTimes(2);
    select(ed, 2);
    expect(heard).toHaveBeenCalledTimes(2);
  });
});
