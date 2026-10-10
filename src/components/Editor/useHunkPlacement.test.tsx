/**
 * DCR-FR-LGHZ / DCR-FR-TSNW / DCR-FR-31: the placement hook finds every change
 * of a proposal in a document built by the real Editor, reports what it placed,
 * and parses each change's Markdown once.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, renderHook, waitFor } from "@testing-library/react";
import { Editor } from "@tiptap/react";

vi.mock("../../logging", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../logging")>()),
  logWarn: vi.fn(),
}));

import { logWarn } from "../../logging";
import { markdownExtensions } from "../markdownFidelity";
import type { EditorReview, ReviewHunk } from "./props";
import { useHunkPlacement } from "./useHunkPlacement";

const BODY = [
  "## Purpose",
  "",
  "The tool should talk to every mirror.",
  "",
  "## Steps",
  "",
  "- User types a mirror, with a placeholder of [mirror.org](http://mirror.org) ",
  "",
  "## Rules",
  "",
  "- [mirror.org](http://mirror.org) stays the default. Hosts like \\*.mirror.net are an option.",
  "- Saved mirrors keep working since [mirror.org](http://mirror.org) is a default.",
].join("\n");

const editors: Editor[] = [];

function editorFor(markdown: string): Editor {
  const editor = new Editor({ extensions: markdownExtensions(), content: markdown });
  editors.push(editor);
  return editor;
}

function hunk(id: string, lead: string, hint: number): ReviewHunk {
  return {
    id,
    kind: "add",
    state: "pending",
    before: "",
    after: "New text.",
    lead,
    hint,
    lost: false,
    position: 1,
    total: 1,
    agent: "Agent",
    editable: true,
  };
}

const PLAIN = hunk("plain", "## Purpose\n\nThe tool should talk to every mirror.", 40);
const LINK = hunk(
  "link",
  "## Steps\n\n- User types a mirror, with a placeholder of [mirror.org](http://mirror.org) ",
  130,
);
const CUT = hunk("cut", BODY.slice(BODY.indexOf("rror.org) stays the default.")), BODY.length);
const ABSENT = hunk("absent", "A sentence the prompt never held at all.", 0);
/** A replacement whose text quotes a link. */
const LINKED: ReviewHunk = {
  ...hunk("linked", "", 230),
  kind: "replace",
  before: "[mirror.org](http://mirror.org) stays the default.",
};
/** A replacement in plain prose. */
const PROSE: ReviewHunk = {
  ...hunk("prose", "", 20),
  kind: "replace",
  before: "The tool should talk to every mirror.",
};

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
    onCaretInProse: vi.fn(),
    onPlaced,
  };
}

function parseSpy(editor: Editor) {
  const storage = editor.storage as unknown as {
    markdown: { parser: { parse: (s: string) => string } };
  };
  return vi.spyOn(storage.markdown.parser, "parse");
}

beforeEach(() => {
  vi.mocked(logWarn).mockClear();
});

afterEach(() => {
  // Unmount the hooks before their editors go.
  cleanup();
  while (editors.length > 0) editors.pop()?.destroy();
  vi.restoreAllMocks();
});

describe("placing a proposal in the real Editor", () => {
  it("DCR-FR-LGHZ, DCR-FR-TSNW, DCR-FR-31: reports every change it placed, and only those", async () => {
    const editor = editorFor(BODY);
    const onPlaced = vi.fn();
    const r = review([PLAIN, LINK, CUT, LINKED, ABSENT], onPlaced);
    const { result } = renderHook(() => useHunkPlacement(editor, r, 0, "wysiwyg"));
    await waitFor(() =>
      expect(onPlaced).toHaveBeenLastCalledWith(["plain", "link", "cut", "linked"]),
    );
    const placed = result.current.placedHunks;
    expect(placed.map((h) => h.id)).toEqual(["plain", "link", "cut", "linked"]);
    // The replacement covers the rendered words it quotes, link text included.
    const linked = placed.find((h) => h.id === "linked")!;
    expect(editor.state.doc.textBetween(linked.from, linked.to)).toBe(
      "mirror.org stays the default.",
    );
  });
});

describe("parsing each change's Markdown once (DCR-FR-LGHZ)", () => {
  it("DCR-FR-LGHZ: a keystroke repeats the search and not the parse", async () => {
    const editor = editorFor(BODY);
    const spy = parseSpy(editor);
    const r = review([LINK, CUT]);
    const { rerender, result } = renderHook(
      ({ version }) => useHunkPlacement(editor, r, version, "wysiwyg"),
      { initialProps: { version: 0 } },
    );
    await waitFor(() => expect(result.current.placedHunks).toHaveLength(2));
    const parses = spy.mock.calls.length;
    expect(parses).toBeGreaterThan(0);
    rerender({ version: 1 });
    rerender({ version: 2 });
    expect(spy.mock.calls.length).toBe(parses);
  });

  it("DCR-FR-LGHZ: two changes with one lead parse it once", async () => {
    const editor = editorFor(BODY);
    const spy = parseSpy(editor);
    const twin = { ...LINK, id: "twin" };
    const { result } = renderHook(() =>
      useHunkPlacement(editor, review([LINK, twin]), 0, "wysiwyg"),
    );
    await waitFor(() => expect(result.current.placedHunks).toHaveLength(2));
    const leadParses = spy.mock.calls.filter(([md]) => md === LINK.lead);
    expect(leadParses).toHaveLength(1);
  });

  it("DCR-FR-LGHZ: a new editor parses again with its own parser", async () => {
    const first = editorFor(BODY);
    const second = editorFor(BODY);
    const firstSpy = parseSpy(first);
    const secondSpy = parseSpy(second);
    const r = review([LINK]);
    const { rerender, result } = renderHook(
      ({ editor }) => useHunkPlacement(editor, r, 0, "wysiwyg"),
      { initialProps: { editor: first } },
    );
    await waitFor(() => expect(result.current.placedHunks).toHaveLength(1));
    expect(firstSpy).toHaveBeenCalled();
    rerender({ editor: second });
    await waitFor(() => expect(secondSpy).toHaveBeenCalled());
    expect(result.current.placedHunks).toHaveLength(1);
  });
});

describe("a parser that fails (DCR-FR-LGHZ, DCR-FR-31)", () => {
  it("DCR-FR-LGHZ, DCR-FR-31: falls back to the source, logs no prompt text, and logs once", async () => {
    const editor = editorFor(BODY);
    const onPlaced = vi.fn();
    const r = review([PROSE, PLAIN, LINK], onPlaced);
    // Broken only after the document is built, so the document is real.
    parseSpy(editor).mockImplementation(() => {
      throw new Error("parser broke");
    });
    const { rerender } = renderHook(
      ({ version }) => useHunkPlacement(editor, r, version, "wysiwyg"),
      { initialProps: { version: 0 } },
    );
    // Plain text is still found by its source: a replacement has no other way
    // to be found. The lead ending in a link is not, and the bar states that
    // (DCR-FR-31).
    await waitFor(() => expect(onPlaced).toHaveBeenLastCalledWith(["prose", "plain"]));
    const warnings = vi.mocked(logWarn).mock.calls.length;
    expect(warnings).toBeGreaterThan(0);
    for (const call of vi.mocked(logWarn).mock.calls) {
      expect(JSON.stringify(call)).not.toContain("mirror");
    }
    rerender({ version: 1 });
    expect(vi.mocked(logWarn).mock.calls.length).toBe(warnings);
  });
});
