/**
 * DCR-FR-JKVA / DCR-FR-BYSX / DCR-FR-31: the placement hook finds the fence
 * that a diagram hunk changes or removes by its source, places an addition as
 * an insertion, and leaves a diagram hunk that finds no fence undrawn.
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, renderHook, waitFor } from "@testing-library/react";
import { Editor } from "@tiptap/react";

import { markdownExtensions } from "../markdownFidelity";
import { placeOnSource } from "./SourceReview";
import type { EditorReview, ReviewHunk } from "./props";
import { useHunkPlacement } from "./useHunkPlacement";

const ONE = "```mermaid\nflowchart TD\n  A --> B\n```";
const OTHER = "```mermaid\nflowchart TD\n  X --> Y\n```";
const NEXT = "```mermaid\nflowchart TD\n  A --> B\n  B --> C\n```";
const BODY = [
  "## First",
  "",
  "The first diagram follows this line.",
  "",
  ONE,
  "",
  "## Second",
  "",
  "The second diagram follows this line.",
  "",
  OTHER,
  "",
  "## Third",
  "",
  "The same diagram again, further down.",
  "",
  ONE,
  "",
  "The end.",
].join("\n");

const editors: Editor[] = [];

function editorFor(markdown: string): Editor {
  const editor = new Editor({ extensions: markdownExtensions(), content: markdown });
  editors.push(editor);
  return editor;
}

function diagramHunk(over: Partial<ReviewHunk> & { old?: string | null; next?: string | null }): ReviewHunk {
  const { old = "flowchart TD\n  A --> B", next = "flowchart TD\n  A --> B\n  B --> C", ...rest } = over;
  return {
    id: "h1",
    kind: "replace",
    state: "pending",
    before: ONE,
    after: NEXT,
    lead: "",
    hint: 0,
    lost: false,
    position: 1,
    total: 1,
    agent: "Agent",
    editable: false,
    diagram: { oldSource: old, newSource: next },
    ...rest,
  };
}

function review(hunks: ReviewHunk[], onPlaced = vi.fn()): EditorReview {
  return {
    proposalId: "p1",
    hunks,
    focused: null,
    busy: false,
    onFocus: vi.fn(),
    onAccept: vi.fn(),
    onReject: vi.fn(),
    onDiscuss: vi.fn(),
    onEdit: vi.fn(),
    onPlaced,
  };
}

afterEach(() => {
  cleanup();
  while (editors.length > 0) editors.pop()?.destroy();
});

function place(editor: Editor, hunks: ReviewHunk[], onPlaced = vi.fn()) {
  return renderHook(() => useHunkPlacement(editor, review(hunks, onPlaced), 0, "wysiwyg"));
}

/** The text of the fence that starts at `from`. */
const fenceTextAt = (editor: Editor, from: number) => editor.state.doc.nodeAt(from)?.textContent;

describe("a diagram hunk that changes or removes a diagram (DCR-FR-JKVA)", () => {
  it("DCR-FR-JKVA: a change is placed on the fence whose source is the old source, as the range of the whole fence", async () => {
    const editor = editorFor(BODY);
    const { result } = place(editor, [diagramHunk({ id: "c", hint: 0 })]);
    await waitFor(() => expect(result.current.placedHunks).toHaveLength(1));
    const placed = result.current.placedHunks[0];
    expect(fenceTextAt(editor, placed.from)).toBe("flowchart TD\n  A --> B");
    expect(placed.to).toBe(placed.from + (editor.state.doc.nodeAt(placed.from)?.nodeSize ?? 0));
    expect(placed.diagram).toEqual({
      oldSource: "flowchart TD\n  A --> B",
      newSource: "flowchart TD\n  A --> B\n  B --> C",
    });
    expect(placed.editable).toBe(false);
  });

  it("DCR-FR-JKVA: where several fences hold the old source, the one nearest the hint wins", async () => {
    const editor = editorFor(BODY);
    const near = place(editor, [diagramHunk({ hint: 0 })]);
    await waitFor(() => expect(near.result.current.placedHunks).toHaveLength(1));
    const first = near.result.current.placedHunks[0].from;
    const far = place(editor, [diagramHunk({ hint: BODY.length })]);
    await waitFor(() => expect(far.result.current.placedHunks).toHaveLength(1));
    expect(far.result.current.placedHunks[0].from).toBeGreaterThan(first);
  });

  it("DCR-FR-JKVA: the sources are compared after trimming", async () => {
    const editor = editorFor(BODY);
    const { result } = place(editor, [diagramHunk({ old: "\nflowchart TD\n  A --> B\n\n" })]);
    await waitFor(() => expect(result.current.placedHunks).toHaveLength(1));
  });

  it("DCR-FR-JKVA: a removal is placed on its fence", async () => {
    const editor = editorFor(BODY);
    const removal = diagramHunk({
      kind: "del",
      after: "",
      old: "flowchart TD\n  X --> Y",
      next: null,
      before: `${OTHER}\n\n`,
    });
    const { result } = place(editor, [removal]);
    await waitFor(() => expect(result.current.placedHunks).toHaveLength(1));
    expect(fenceTextAt(editor, result.current.placedHunks[0].from)).toBe("flowchart TD\n  X --> Y");
  });

  it("DCR-FR-BYSX, DCR-FR-31: a diagram hunk whose old diagram is not in the document is not drawn and is not reported as placed", async () => {
    const editor = editorFor(BODY);
    const onPlaced = vi.fn();
    const lost = diagramHunk({ id: "lost", old: "flowchart TD\n  Q --> R", lost: true });
    const { result } = place(editor, [lost, diagramHunk({ id: "ok" })], onPlaced);
    await waitFor(() => expect(onPlaced).toHaveBeenLastCalledWith(["ok"]));
    expect(result.current.placedHunks.map((h) => h.id)).toEqual(["ok"]);
  });

  it("DCR-FR-BYSX: a diagram hunk with no old source is not drawn, though it replaces", async () => {
    const editor = editorFor(BODY);
    const onPlaced = vi.fn();
    place(editor, [diagramHunk({ old: null })], onPlaced);
    await waitFor(() => expect(onPlaced).toHaveBeenCalled());
    expect(onPlaced).toHaveBeenLastCalledWith([]);
  });

  it("DCR-FR-JKVA, DCR-FR-07: an accepted or withdrawn diagram hunk is not placed, and a rejected one is", async () => {
    const editor = editorFor(BODY);
    const { result } = place(editor, [
      diagramHunk({ id: "a", state: "accepted" }),
      diagramHunk({ id: "w", state: "withdrawn" }),
      diagramHunk({ id: "r", state: "rejected" }),
    ]);
    await waitFor(() => expect(result.current.placedHunks).toHaveLength(1));
    expect(result.current.placedHunks[0].id).toBe("r");
  });
});

describe("a diagram hunk that adds a diagram (DCR-FR-JKVA, DCR-FR-TSNW)", () => {
  it("DCR-FR-JKVA, DCR-FR-TSNW: an addition is placed as an insertion, after the end of its lead", async () => {
    const editor = editorFor(BODY);
    const addition = diagramHunk({
      kind: "add",
      before: "",
      after: `\n\n${OTHER}`,
      lead: "The end.",
      old: null,
      next: "flowchart TD\n  X --> Y",
      hint: BODY.length,
    });
    const { result } = place(editor, [addition]);
    await waitFor(() => expect(result.current.placedHunks).toHaveLength(1));
    const placed = result.current.placedHunks[0];
    expect(placed.from).toBe(placed.to);
    expect(editor.state.doc.textBetween(0, placed.from)).toMatch(/The end\.$/);
    expect(placed.diagram?.oldSource).toBeNull();
  });

  it("DCR-FR-BYSX, DCR-FR-31: an addition whose lead the document does not hold is not drawn", async () => {
    const editor = editorFor(BODY);
    const onPlaced = vi.fn();
    place(
      editor,
      [diagramHunk({ kind: "add", before: "", lead: "A line the document never held at all.", old: null })],
      onPlaced,
    );
    await waitFor(() => expect(onPlaced).toHaveBeenCalled());
    expect(onPlaced).toHaveBeenLastCalledWith([]);
  });
});

describe("a diagram hunk on the source surface (PCR-FR-GZSV)", () => {
  const FILE = `intro\n${ONE}\noutro\n${OTHER}\n`;

  it("PCR-FR-GZSV: a change is placed on the fence text of the file, and keeps its diagram", () => {
    const [placed] = placeOnSource(FILE, [diagramHunk({ hint: FILE.indexOf(ONE) })]);
    expect(FILE.slice(placed.from, placed.to)).toBe(ONE);
    expect(placed.diagram?.newSource).toBe("flowchart TD\n  A --> B\n  B --> C");
    expect(placed.editable).toBe(false);
  });

  it("PCR-FR-GZSV: a removal whose fence text ends in line breaks is placed on the fence", () => {
    const [placed] = placeOnSource(FILE, [
      diagramHunk({ kind: "del", before: `${OTHER}\n\n`, after: "", old: "x", next: null, hint: FILE.indexOf(OTHER) }),
    ]);
    expect(FILE.slice(placed.from, placed.to)).toBe(OTHER);
  });

  it("PCR-FR-GZSV: an addition is placed after its lead", () => {
    const [placed] = placeOnSource(FILE, [
      diagramHunk({ kind: "add", before: "", after: `\n\n${NEXT}`, lead: "intro", old: null, hint: 5 }),
    ]);
    expect(placed.from).toBe("intro".length);
    expect(placed.to).toBe(placed.from);
  });

  it("PCR-FR-GZSV, DCR-FR-31: a fence that the file does not hold is not placed", () => {
    expect(
      placeOnSource("nothing here\n", [diagramHunk({ hint: 0 })]),
    ).toEqual([]);
  });
});
