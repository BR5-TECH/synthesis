import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Editor } from "@tiptap/react";
import { TextSelection } from "@tiptap/pm/state";

vi.mock("mermaid", async () => (await import("../test/mermaidMock")).mermaidModule);
vi.mock("../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
}));

import {
  holdNextRender,
  mermaidApi,
  overrideNextRender,
  resetMermaidMock,
} from "../test/mermaidMock";
import { getMarkdown, markdownExtensions } from "./markdownFidelity";
import {
  mermaidKey,
  DIAGRAM_LABEL,
  DIAGRAM_LAYOUT_EVENT,
  INERT_DIAGRAM_LABEL,
  SHOW_DIAGRAM_LABEL,
  SHOW_SOURCE_LABEL,
} from "./markdownMermaid";
import { cachedDiagram, renderDiagram, resetMermaidForTest } from "./markdownMermaidRender";

const FENCE = "```mermaid\ngraph TD\nA-->B\n```";

let editors: Editor[] = [];

function mount(content: string, editable = true): Editor {
  const ed = new Editor({ extensions: markdownExtensions(), content, editable });
  editors.push(ed);
  return ed;
}

const pres = (ed: Editor) => Array.from(ed.view.dom.querySelectorAll("pre"));
const viewOf = (ed: Editor, i = 0) => pres(ed)[i]?.getAttribute("data-diagram-view");
const toggleOf = (ed: Editor, i = 0) =>
  pres(ed)[i]?.querySelector<HTMLButtonElement>(".mermaid-toggle") ?? null;
const figureOf = (ed: Editor, i = 0) =>
  pres(ed)[i]?.querySelector<HTMLElement>(".mermaid-diagram") ?? null;

/**
 * Renders run one at a time, in order, so once a render asked for now has
 * finished, every render asked for before it has been run or skipped.
 */
async function settleRenders(): Promise<void> {
  await renderDiagram("graph TD\nsentinel", "light");
}

const renderedSources = () => mermaidApi.render.mock.calls.map((call) => call[1]);

async function rendered(ed: Editor, i = 0): Promise<void> {
  await vi.waitFor(() => expect(viewOf(ed, i)).toBe("diagram"));
}

beforeEach(() => {
  resetMermaidForTest();
  resetMermaidMock();
  delete document.documentElement.dataset.theme;
});

afterEach(() => {
  for (const ed of editors) ed.destroy();
  editors = [];
  delete document.documentElement.dataset.theme;
});

describe("the default view (EDT-FR-MOYS)", () => {
  it("EDT-FR-MOYS, EDT-FR-USWS, EDT-FR-KQXZ: a mermaid fence shows as its diagram with a toggle to its source", async () => {
    const ed = mount(FENCE);
    expect(viewOf(ed)).toBe("pending");
    await rendered(ed);
    const figure = figureOf(ed)!;
    expect(figure.querySelector("svg text")?.textContent).toBe("graph TD\nA-->B");
    expect(figure.getAttribute("role")).toBe("button");
    expect(figure.getAttribute("aria-label")).toBe(DIAGRAM_LABEL);
    expect(figure.tabIndex).toBe(0);
    // EDT-FR-USWS: a real button the keyboard reaches.
    const toggle = toggleOf(ed)!;
    expect(toggle.tagName).toBe("BUTTON");
    expect(toggle.type).toBe("button");
    expect(toggle.disabled).toBe(false);
    expect(toggle.tabIndex).toBeGreaterThanOrEqual(0);
    expect(toggle.getAttribute("aria-label")).toBe(SHOW_SOURCE_LABEL);
  });

  it("EDT-FR-MOYS, EDT-FR-CQBG: the Markdown is the plain fence it loaded as, after the diagram renders", async () => {
    const md = `# Title\n\n${FENCE}\n\nAfter.`;
    const ed = mount(md);
    await rendered(ed);
    const out = getMarkdown(ed);
    expect(out).toBe(md);
    expect(out).not.toMatch(/Rendering diagram|Diagram error|Show diagram/);
  });

  it("EDT-FR-XTEB, EDT-FR-CQBG: an invalid fence keeps its Markdown exactly", async () => {
    const md = "```mermaid\nINVALID\n```";
    const ed = mount(md);
    await vi.waitFor(() => expect(viewOf(ed)).toBe("error"));
    expect(getMarkdown(ed)).toBe(md);
  });

  it("EDT-FR-MOYS, ESH-FR-RKHL: a fence in another language, or one only named like mermaid, stays plain code", async () => {
    const ed = mount("```js\nconst a = 1;\n```\n\n```mermaidx\ngraph TD\n```\n\n```Mermaid\ngraph TD\n```");
    for (let i = 0; i < 3; i++) expect(viewOf(ed, i)).toBeNull();
    await settleRenders();
    expect(renderedSources()).toEqual(["graph TD\nsentinel"]);
  });

  it("EDT-FR-MOYS: a fence inside a list item and inside a blockquote shows its diagram and round-trips", async () => {
    const md = `- item\n\n  ${FENCE.split("\n").join("\n  ")}\n\n> ${FENCE.split("\n").join("\n> ")}`;
    const ed = mount(md);
    await rendered(ed, 0);
    await rendered(ed, 1);
    const before = getMarkdown(ed);
    expect(before).toContain("```mermaid");
    // The round trip is stable through the decorations.
    const again = mount(before);
    await rendered(again, 1);
    expect(getMarkdown(again)).toBe(before);
  });

  it("EDT-FR-MOYS, EDT-FR-BQIU: an empty fence shows its source, renders nothing, and has no toggle", async () => {
    const ed = mount(`\`\`\`mermaid\n\`\`\`\n\n\`\`\`mermaid\n   \n\`\`\`\n\n${FENCE}`);
    expect(viewOf(ed, 0)).toBe("source");
    expect(viewOf(ed, 1)).toBe("source");
    expect(toggleOf(ed, 0)).toBeNull();
    expect(toggleOf(ed, 1)).toBeNull();
    await rendered(ed, 2);
    await settleRenders();
    expect(renderedSources()).toEqual(["graph TD\nA-->B", "graph TD\nsentinel"]);
  });

  it("EDT-FR-MOYS: an inserted mermaid fence shows as a diagram", async () => {
    const ed = mount("Text.");
    ed.commands.insertContentAt(ed.state.doc.content.size, "```mermaid\ngraph TD\nX-->Y\n```");
    await rendered(ed);
    expect(figureOf(ed)?.querySelector("svg text")?.textContent).toBe("graph TD\nX-->Y");
  });
});

describe("the view toggle (EDT-FR-USWS, EDT-FR-CQBG)", () => {
  it("EDT-FR-USWS, EDT-FR-CQBG: the toggle switches the view and changes no document, no history, no update", async () => {
    const ed = mount(FENCE);
    await rendered(ed);
    const doc = ed.state.doc;
    const onUpdate = vi.fn();
    ed.on("update", onUpdate);

    toggleOf(ed)!.click();
    expect(viewOf(ed)).toBe("source");
    expect(toggleOf(ed)?.getAttribute("aria-label")).toBe(SHOW_DIAGRAM_LABEL);
    expect(toggleOf(ed)?.textContent).toBe("Diagram");
    expect(figureOf(ed)).toBeNull();

    toggleOf(ed)!.click();
    expect(viewOf(ed)).toBe("diagram");
    expect(toggleOf(ed)?.textContent).toBe("Source");

    expect(ed.state.doc).toBe(doc);
    expect(onUpdate).not.toHaveBeenCalled();
    expect(getMarkdown(ed)).toBe(FENCE);
  });

  it("EDT-FR-USWS: two identical fences share one render, show separate nodes, and toggle apart", async () => {
    const ed = mount(`${FENCE}\n\n${FENCE}`);
    await rendered(ed, 0);
    await rendered(ed, 1);
    expect(mermaidApi.render).toHaveBeenCalledTimes(1);
    const [a, b] = [figureOf(ed, 0)!.querySelector("svg"), figureOf(ed, 1)!.querySelector("svg")];
    expect(a).not.toBe(b);
    expect(a?.id).not.toBe(b?.id);

    toggleOf(ed, 1)!.click();
    expect(viewOf(ed, 0)).toBe("diagram");
    expect(viewOf(ed, 1)).toBe("source");

    // The source view follows its fence when text is added above it.
    ed.view.dispatch(ed.state.tr.insert(0, ed.schema.nodes.paragraph.create(null, ed.schema.text("Above"))));
    expect(viewOf(ed, 0)).toBe("diagram");
    expect(viewOf(ed, 1)).toBe("source");
  });

  it("EDT-FR-USWS: a fence set to its source while it renders stays on its source", async () => {
    const held = holdNextRender();
    const ed = mount(FENCE);
    expect(viewOf(ed)).toBe("pending");
    toggleOf(ed)!.click();
    expect(viewOf(ed)).toBe("source");
    const refreshes = vi.fn();
    ed.on("transaction", ({ transaction }) => {
      if (transaction.getMeta(mermaidKey)) refreshes();
    });
    held.release();
    await vi.waitFor(() => expect(cachedDiagram("graph TD\nA-->B", "light")).toBeDefined());
    await vi.waitFor(() => expect(refreshes).toHaveBeenCalled());
    expect(viewOf(ed)).toBe("source");
  });
});

describe("invalid syntax (EDT-FR-XTEB, EDT-FR-JBZU)", () => {
  it("EDT-FR-XTEB, EDT-FR-BQIU: an invalid fence shows its source and an inline error, and stays editable", async () => {
    const ed = mount("```mermaid\nINVALID\n```");
    await vi.waitFor(() => expect(viewOf(ed)).toBe("error"));
    const pre = pres(ed)[0];
    expect(pre.querySelector(".mermaid-error")?.textContent).toBe(
      "Diagram error: Parse error on line 1: INVALID",
    );
    expect(figureOf(ed)).toBeNull();
    expect(toggleOf(ed)).toBeNull();
    expect(pre.classList.contains("mermaid-block--error")).toBe(true);

    ed.view.dispatch(ed.state.tr.insertText("x", 2));
    expect(ed.state.doc.textContent).toBe("IxNVALID");
  });

  it("EDT-FR-XTEB, EDT-FR-MIQE: fixing the source replaces the error with the diagram", async () => {
    const ed = mount("```mermaid\nINVALID\n```");
    await vi.waitFor(() => expect(viewOf(ed)).toBe("error"));
    ed.view.dispatch(ed.state.tr.insertText("graph TD", 1, 8));
    await rendered(ed);
    expect(pres(ed)[0].querySelector(".mermaid-error")).toBeNull();
  });

  it("EDT-FR-JBZU: an error message that holds markup shows as text", async () => {
    overrideNextRender(async () => {
      throw new Error('<img src=x onerror="window.__mermaidX=1">');
    });
    const ed = mount("```mermaid\ngraph TD\n```");
    await vi.waitFor(() => expect(viewOf(ed)).toBe("error"));
    const error = pres(ed)[0].querySelector(".mermaid-error")!;
    expect(error.querySelector("img")).toBeNull();
    expect(error.textContent).toContain('<img src=x onerror="window.__mermaidX=1">');
    expect((window as unknown as Record<string, unknown>).__mermaidX).toBeUndefined();
  });

  it("EDT-FR-XTEB: an SVG that does not parse is an error with the source, not an empty diagram", async () => {
    overrideNextRender(async () => ({ svg: "<svg" }));
    const ed = mount(FENCE);
    await vi.waitFor(() => expect(viewOf(ed)).toBe("error"));
    expect(figureOf(ed)).toBeNull();
  });
});

describe("a surface that accepts no input (EDT-FR-LEUI)", () => {
  it("EDT-FR-LEUI: the diagram shows with no toggle, no focus and no click", async () => {
    const ed = mount(FENCE, false);
    await rendered(ed);
    expect(toggleOf(ed)).toBeNull();
    const figure = figureOf(ed)!;
    expect(figure.getAttribute("role")).toBe("img");
    expect(figure.getAttribute("aria-label")).toBe(INERT_DIAGRAM_LABEL);
    expect(figure.hasAttribute("tabindex")).toBe(false);
    const selection = ed.state.selection;
    figure.click();
    for (const key of ["Enter", " "]) {
      figure.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }));
    }
    expect(viewOf(ed)).toBe("diagram");
    expect(ed.state.selection.eq(selection)).toBe(true);
    expect(getMarkdown(ed)).toBe(FENCE);
  });

  it("EDT-FR-LEUI: a diagram still rendering on an inert surface is announced as rendering", () => {
    const held = holdNextRender();
    const ed = mount(FENCE, false);
    const figure = figureOf(ed)!;
    expect(figure.getAttribute("role")).toBe("status");
    expect(figure.textContent).toBe("Rendering diagram…");
    expect(figure.hasAttribute("aria-label")).toBe(false);
    held.release();
  });

  it("EDT-FR-LEUI, EDT-FR-XTEB: an invalid fence there shows its source and error, read-only", async () => {
    const ed = mount("```mermaid\nINVALID\n```", false);
    await vi.waitFor(() => expect(viewOf(ed)).toBe("error"));
    expect(toggleOf(ed)).toBeNull();
    expect(pres(ed)[0].querySelector(".mermaid-error")).not.toBeNull();
  });

  it("EDT-FR-LEUI: a surface that becomes read-only drops the toggle and the source view, and gets them back", async () => {
    const ed = mount(FENCE);
    await rendered(ed);
    toggleOf(ed)!.click();
    expect(viewOf(ed)).toBe("source");
    ed.setEditable(false, false);
    expect(viewOf(ed)).toBe("diagram");
    expect(toggleOf(ed)).toBeNull();
    ed.setEditable(true, false);
    expect(viewOf(ed)).toBe("source");
    expect(toggleOf(ed)).not.toBeNull();
  });
});

describe("the theme (EDT-FR-JIBV)", () => {
  it("EDT-FR-JIBV: a change of theme renders the diagram again in that theme, and back from the cache", async () => {
    const ed = mount(FENCE);
    await rendered(ed);
    expect(figureOf(ed)?.querySelector("svg")?.getAttribute("data-theme")).toBe("default");
    const doc = ed.state.doc;

    document.documentElement.dataset.theme = "dark";
    await vi.waitFor(() =>
      expect(figureOf(ed)?.querySelector("svg")?.getAttribute("data-theme")).toBe("dark"),
    );
    expect(mermaidApi.initialize).toHaveBeenLastCalledWith(
      expect.objectContaining({ theme: "dark" }),
    );

    document.documentElement.dataset.theme = "light";
    await vi.waitFor(() =>
      expect(figureOf(ed)?.querySelector("svg")?.getAttribute("data-theme")).toBe("default"),
    );
    expect(mermaidApi.render).toHaveBeenCalledTimes(2);
    expect(ed.state.doc).toBe(doc);
  });

  it("EDT-FR-JIBV: a surface opened in the dark theme renders dark first", async () => {
    document.documentElement.dataset.theme = "dark";
    const ed = mount(FENCE);
    await rendered(ed);
    expect(figureOf(ed)?.querySelector("svg")?.getAttribute("data-theme")).toBe("dark");
    expect(mermaidApi.render).toHaveBeenCalledTimes(1);
  });
});

describe("render requests", () => {
  it("EDT-FR-MIQE: a burst of edits renders the first and the last source, not each one", async () => {
    const held = holdNextRender();
    const ed = mount(FENCE);
    toggleOf(ed)!.click();
    await vi.waitFor(() => expect(mermaidApi.render).toHaveBeenCalledTimes(1));
    for (const ch of ["C", "D", "E"]) {
      const end = ed.state.doc.firstChild!.nodeSize - 1;
      ed.view.dispatch(ed.state.tr.insertText(ch, end));
    }
    held.release();
    await vi.waitFor(() => expect(mermaidApi.render).toHaveBeenCalledTimes(2));
    await vi.waitFor(() =>
      expect(pres(ed)[0].getAttribute("data-diagram-view")).toBe("source"),
    );
    expect(mermaidApi.render.mock.calls[1][1]).toBe("graph TD\nA-->BCDE");
  });

  it("EDT-FR-MOYS: a surface destroyed while its diagram renders takes no late result", async () => {
    const held = holdNextRender();
    const ed = mount(FENCE);
    await vi.waitFor(() => expect(mermaidApi.render).toHaveBeenCalledTimes(1));
    const dispatch = vi.spyOn(ed.view, "dispatch");
    ed.destroy();
    editors = editors.filter((e) => e !== ed);
    held.release();
    await vi.waitFor(() => expect(cachedDiagram("graph TD\nA-->B", "light")).toBeDefined());
    await settleRenders();
    expect(dispatch).not.toHaveBeenCalled();
  });

  it("EDT-FR-MOYS: a surface destroyed before its render's turn never renders", async () => {
    const held = holdNextRender();
    const blocker = mount("```mermaid\ngraph TD\nblocker\n```");
    const ed = mount(FENCE);
    await vi.waitFor(() => expect(mermaidApi.render).toHaveBeenCalledTimes(1));
    ed.destroy();
    editors = editors.filter((e) => e !== ed);
    held.release();
    await rendered(blocker);
    await settleRenders();
    expect(renderedSources()).not.toContain("graph TD\nA-->B");
  });

  it("EDT-FR-MIQE: many transactions during one render ask once and redraw once", async () => {
    const held = holdNextRender();
    const ed = mount(`Intro\n\n${FENCE}`);
    const refreshes = vi.fn();
    ed.on("transaction", ({ transaction }) => {
      if ((transaction.getMeta(mermaidKey) as { type?: string } | undefined)?.type === "refresh") {
        refreshes();
      }
    });
    await vi.waitFor(() => expect(mermaidApi.render).toHaveBeenCalledTimes(1));
    for (let i = 0; i < 5; i++) {
      ed.view.dispatch(ed.state.tr.setSelection(TextSelection.create(ed.state.doc, 1 + (i % 3))));
    }
    held.release();
    await rendered(ed);
    expect(mermaidApi.render).toHaveBeenCalledTimes(1);
    expect(refreshes).toHaveBeenCalledTimes(1);
  });

  it("EDT-FR-MOYS: a document with more diagrams than the cache holds renders each once and settles", async () => {
    const md = Array.from({ length: 70 }, (_, i) => `\`\`\`mermaid\ngraph TD\nn${i}\n\`\`\``).join("\n\n");
    const ed = mount(md);
    await vi.waitFor(() =>
      expect(pres(ed).every((p) => p.getAttribute("data-diagram-view") === "diagram")).toBe(true),
    );
    // Hold the next render, so a render asked for again would stay visible.
    ed.view.dispatch(ed.state.tr.setSelection(TextSelection.create(ed.state.doc, 1)));
    await settleRenders();
    expect(mermaidApi.render).toHaveBeenCalledTimes(71);
    expect(pres(ed).every((p) => p.getAttribute("data-diagram-view") === "diagram")).toBe(true);
  });

  it("EDT-FR-MIQE: selecting text does not ask for any render again", async () => {
    const ed = mount(`Intro\n\n${FENCE}`);
    await rendered(ed);
    ed.view.dispatch(ed.state.tr.setSelection(TextSelection.create(ed.state.doc, 2)));
    await settleRenders();
    expect(renderedSources()).toEqual(["graph TD\nA-->B", "graph TD\nsentinel"]);
  });

  it("EDT-FR-JIBV, EDT-FR-UYEN: a change of theme reports the change of fence height", async () => {
    const ed = mount(FENCE);
    await rendered(ed);
    const heard = vi.fn();
    ed.view.dom.addEventListener(DIAGRAM_LAYOUT_EVENT, heard);
    document.documentElement.dataset.theme = "dark";
    await vi.waitFor(() =>
      expect(figureOf(ed)?.querySelector("svg")?.getAttribute("data-theme")).toBe("dark"),
    );
    expect(heard).toHaveBeenCalled();
  });
});
