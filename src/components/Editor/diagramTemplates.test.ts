import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Editor } from "@tiptap/react";
import { AllSelection, NodeSelection, TextSelection } from "@tiptap/pm/state";

vi.mock("mermaid", async () => (await import("../../test/mermaidMock")).mermaidModule);
vi.mock("../../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
}));

import { resetMermaidMock } from "../../test/mermaidMock";
import { resetMermaidForTest } from "../markdownMermaidRender";
import { destroyRichEditor, mountRichEditor } from "../../test/richEditorFixtures";
import { getMarkdown } from "../markdownFidelity";
import { mermaidKey } from "../markdownMermaid";
import { HINT_SETS } from "../DiagramEditor/hints";
import {
  DIAGRAM_TEMPLATES,
  LEADING_TEMPLATE_KEYS,
  insertDiagramFence,
} from "./diagramTemplates";

const EXAMPLE = "graph TD\nA-->B";

beforeEach(() => {
  resetMermaidMock();
  resetMermaidForTest();
});

afterEach(() => {
  destroyRichEditor();
});

/**
 * The top-level blocks of the document, as `type` or `type:text`. The empty
 * paragraph that the surface keeps at the end of the document is left out.
 */
const blocks = (ed: Editor) => {
  const out: string[] = [];
  ed.state.doc.forEach((node) => {
    out.push(node.textContent === "" ? node.type.name : `${node.type.name}:${node.textContent}`);
  });
  if (out.length > 1 && out[out.length - 1] === "paragraph") out.pop();
  return out;
};

type Json = { type: string; attrs?: Record<string, unknown>; content?: Json[]; text?: string };
const p = (text?: string): Json => ({
  type: "paragraph",
  content: text ? [{ type: "text", text }] : undefined,
});
/** Replace the document with `content`, as a load does, with no edit. */
const setDoc = (ed: Editor, content: Json[]) =>
  ed.commands.setContent({ type: "doc", content }, { emitUpdate: false });

/** The position of the start of the textblock whose text is `text`. */
function posOf(ed: Editor, text: string): number {
  let at = -1;
  ed.state.doc.descendants((node, pos) => {
    if (at === -1 && node.isTextblock && node.textContent === text) at = pos;
  });
  if (at === -1) throw new Error(`no textblock "${text}"`);
  return at;
}

function caretAt(ed: Editor, pos: number) {
  ed.view.dispatch(ed.state.tr.setSelection(TextSelection.create(ed.state.doc, pos)));
}

/** The start of the only diagram fence, and whether the caret is in it. */
function fenceState(ed: Editor) {
  let fence = -1;
  ed.state.doc.forEach((node, offset) => {
    if (node.type.name === "codeBlock" && node.attrs.language === "mermaid") fence = offset;
  });
  const node = ed.state.doc.nodeAt(fence);
  const { from } = ed.state.selection;
  return {
    fence,
    caretInside: node !== null && from > fence && from < fence + node.nodeSize,
    showsSource: mermaidKey.getState(ed.state)?.sources.includes(fence) ?? false,
  };
}

describe("DIAGRAM_TEMPLATES (EDT-FR-SCSP)", () => {
  it("EDT-FR-SCSP: lists the chart types that the requirement names first, in order", () => {
    // flowchart, sequence, class, state, entity relationship, gantt, pie,
    // mindmap, user journey, git graph, timeline, quadrant, requirement, C4.
    const required = [
      "flowchart",
      "sequence",
      "class",
      "state",
      "er",
      "gantt",
      "pie",
      "mindmap",
      "journey",
      "gitGraph",
      "timeline",
      "quadrantChart",
      "requirement",
      "c4",
    ];
    expect(LEADING_TEMPLATE_KEYS).toEqual(required);
    expect(DIAGRAM_TEMPLATES.slice(0, required.length).map((t) => t.key)).toEqual(required);
  });

  it("EDT-FR-SCSP: holds each chart type of the hint sets that draws a diagram, once, with its parse-checked example", () => {
    const keys = DIAGRAM_TEMPLATES.map((t) => t.key);
    expect(keys).not.toContain("info");
    expect(new Set(keys).size).toBe(keys.length);
    const drawing = HINT_SETS.filter((set) => set.key !== "info");
    expect([...keys].sort()).toEqual(drawing.map((set) => set.key).sort());
    for (const template of DIAGRAM_TEMPLATES) {
      const set = drawing.find((s) => s.key === template.key);
      expect(template.example).toBe(set?.example);
      expect(template.name).toBe(set?.name);
      expect(template.example.trim()).not.toBe("");
    }
  });
});

describe("the keyword of a template (EDT-FR-OWNT)", () => {
  it("EDT-FR-OWNT: each template carries the keyword its hint set declares, which its example uses", () => {
    for (const template of DIAGRAM_TEMPLATES) {
      const set = HINT_SETS.find((s) => s.key === template.key);
      expect(template.keyword).not.toBe("");
      expect(set?.declaration.code.startsWith(template.keyword)).toBe(true);
      expect(template.example).toContain(template.keyword.replace(/-beta$/, ""));
    }
    expect(DIAGRAM_TEMPLATES.find((t) => t.key === "sequence")?.keyword).toBe("sequenceDiagram");
  });
});

describe("insertDiagramFence (EDT-FR-IPMX, EDT-FR-HAGB)", () => {
  it("EDT-FR-IPMX: an empty paragraph is replaced by the fence, which shows its source with the caret in it", () => {
    const ed = mountRichEditor("");
    setDoc(ed, [p("Before"), p(), p("After")]);
    caretAt(ed, posOf(ed, "Before") + "Before".length + 3);
    expect(ed.state.selection.$from.parent.content.size).toBe(0);

    expect(insertDiagramFence(ed, EXAMPLE)).toBe(true);

    expect(blocks(ed)).toEqual(["paragraph:Before", `codeBlock:${EXAMPLE}`, "paragraph:After"]);
    const state = fenceState(ed);
    expect(state.caretInside).toBe(true);
    expect(state.showsSource).toBe(true);
  });

  it("EDT-FR-IPMX: in a block with text the fence is a new block after it, and that block is unchanged", () => {
    const ed = mountRichEditor("First paragraph\n\nSecond\n");
    caretAt(ed, posOf(ed, "First paragraph") + 4);

    insertDiagramFence(ed, EXAMPLE);

    expect(blocks(ed)).toEqual([
      "paragraph:First paragraph",
      `codeBlock:${EXAMPLE}`,
      "paragraph:Second",
    ]);
    expect(fenceState(ed).caretInside).toBe(true);
  });

  it("EDT-FR-IPMX: a heading, an empty heading, and a code block are followed, never replaced", () => {
    const ed = mountRichEditor("## Title\n\n```\ncode\n```\n");
    caretAt(ed, posOf(ed, "Title") + 1);
    insertDiagramFence(ed, EXAMPLE);
    expect(blocks(ed)).toEqual(["heading:Title", `codeBlock:${EXAMPLE}`, "codeBlock:code"]);

    caretAt(ed, posOf(ed, "code") + 2);
    insertDiagramFence(ed, "pie\n\"a\": 1");
    expect(blocks(ed).slice(-2)).toEqual(["codeBlock:code", 'codeBlock:pie\n"a": 1']);

    setDoc(ed, [{ type: "heading", attrs: { level: 3 } }, p("x")]);
    caretAt(ed, 1);
    insertDiagramFence(ed, EXAMPLE);
    expect(blocks(ed)).toEqual(["heading", `codeBlock:${EXAMPLE}`, "paragraph:x"]);
  });

  it("EDT-FR-IPMX: an empty paragraph in a list, a quote, or a table cell is not replaced; the fence follows the top-level block", () => {
    const ed = mountRichEditor("");
    const listItem = (...content: Json[]): Json => ({ type: "listItem", content });
    setDoc(ed, [
      {
        type: "bulletList",
        content: [listItem(p("item"), { type: "bulletList", content: [listItem(p())] })],
      },
      { type: "blockquote", content: [p()] },
      {
        type: "table",
        content: [
          { type: "tableRow", content: [{ type: "tableHeader", content: [p("h")] }] },
          { type: "tableRow", content: [{ type: "tableCell", content: [p()] }] },
        ],
      },
    ]);
    const emptyAt = (type: string) => {
      let at = -1;
      ed.state.doc.descendants((node, pos) => {
        if (at !== -1) return false;
        const parent = ed.state.doc.resolve(pos).parent;
        if (node.type.name === "paragraph" && node.content.size === 0 && parent.type.name === type) {
          at = pos + 1;
        }
        return true;
      });
      return at;
    };

    caretAt(ed, emptyAt("tableCell"));
    insertDiagramFence(ed, EXAMPLE);
    caretAt(ed, emptyAt("blockquote"));
    insertDiagramFence(ed, EXAMPLE);
    caretAt(ed, emptyAt("listItem"));
    insertDiagramFence(ed, EXAMPLE);

    expect(blocks(ed).map((b) => b.split(":")[0])).toEqual([
      "bulletList",
      "codeBlock",
      "blockquote",
      "codeBlock",
      "table",
      "codeBlock",
    ]);
    expect(() => ed.state.doc.check()).not.toThrow();
    // EDT-FR-HAGB: the table and each fence survive a round trip.
    const md = getMarkdown(ed);
    expect(md.match(/```mermaid/g)).toHaveLength(3);
    expect(md).toContain("| h |");
  });

  it("EDT-FR-IPMX: with a whole top-level block selected the fence follows that block, and a select-all puts it at the end", () => {
    const ed = mountRichEditor("Top\n\n```mermaid\npie\n```\n\nBottom\n");
    let fencePos = -1;
    ed.state.doc.forEach((node, offset) => {
      if (node.type.name === "codeBlock") fencePos = offset;
    });
    ed.view.dispatch(ed.state.tr.setSelection(NodeSelection.create(ed.state.doc, fencePos)));
    insertDiagramFence(ed, EXAMPLE);
    expect(blocks(ed)).toEqual([
      "paragraph:Top",
      "codeBlock:pie",
      `codeBlock:${EXAMPLE}`,
      "paragraph:Bottom",
    ]);

    ed.view.dispatch(ed.state.tr.setSelection(new AllSelection(ed.state.doc)));
    insertDiagramFence(ed, "pie");
    expect(blocks(ed).slice(-1)).toEqual(["codeBlock:pie"]);
  });

  it("EDT-FR-VUGQ: a surface that accepts no input takes no insert", () => {
    const ed = mountRichEditor("Text\n", false);
    const before = ed.state.doc;
    expect(insertDiagramFence(ed, EXAMPLE)).toBe(false);
    expect(ed.state.doc).toBe(before);
  });

  it("EDT-FR-HAGB, EDT-FR-MOYS: the fence serialises as a plain mermaid fence that holds the example", () => {
    const ed = mountRichEditor("Intro\n");
    caretAt(ed, posOf(ed, "Intro") + 1);
    insertDiagramFence(ed, EXAMPLE);
    expect(getMarkdown(ed)).toContain("```mermaid\n" + EXAMPLE + "\n```");
  });
});
