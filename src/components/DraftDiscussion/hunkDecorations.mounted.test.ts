/**
 * A proposed text drawn in a real editing surface
 * (`../../../specifications/ui/DCR-draft-change-review.md` DCR-FR-02,
 * DCR-FR-09, DCR-FR-12, DCR-FR-24).
 *
 * The unit cases call a widget's `toDOM` and `destroy` by hand. These put the
 * widget where ProseMirror puts it, so what is proven is what ProseMirror does
 * with it: when it builds it again, when it drops it, and that the prompt is
 * untouched by what is typed into it.
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import { Editor } from "@tiptap/react";

import { markdownExtensions } from "../markdownFidelity";
import {
  HunkDecorations,
  hunkDecorationKey,
  type DecoratedHunk,
  type HunkDecorationPayload,
} from "./hunkDecorations";

type Surface = HTMLElement & { editor?: Editor };

const editors: Editor[] = [];

afterEach(() => {
  for (const editor of editors.splice(0)) editor.destroy();
  document.body.innerHTML = "";
});

function prompt(markdown: string): Editor {
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

function insertion(over: Partial<DecoratedHunk> = {}): DecoratedHunk {
  return {
    id: "h1",
    kind: "add",
    state: "pending",
    from: 1,
    to: 1,
    after: "\n\n## Proposed head\n\n- one",
    lost: false,
    position: 1,
    total: 1,
    agent: "Helga",
    editable: true,
    ...over,
  };
}

function surfaceIn(editor: Editor): Surface {
  const surface = editor.view.dom.querySelector<Surface>(".hunk--add .hunk__doc");
  if (!surface?.editor) throw new Error("no proposed text surface in the prompt");
  return surface;
}

describe("a proposed text in the prompt's own surface", () => {
  it("DCR-FR-09: renders rich between the prompt's blocks", () => {
    const editor = prompt("Body.");
    push(editor, { hunks: [insertion()], focused: "h1" });
    const surface = surfaceIn(editor);
    expect(surface.querySelector("h2")?.textContent).toBe("Proposed head");
    expect(surface.querySelector("ul > li")?.textContent).toBe("one");
    expect(editor.view.dom.querySelector("p .hunk--add")).toBeNull();
  });

  it("DCR-FR-24, DCR-FR-02: typing in it changes no byte of the prompt, and the prompt stays editable", () => {
    const editor = prompt("Body.");
    const onEdit = vi.fn();
    push(editor, { hunks: [insertion()], focused: "h1", onEdit });
    const before = editor.getJSON();
    const inner = surfaceIn(editor).editor as Editor;
    inner.commands.setTextSelection(inner.state.doc.content.size - 1);
    inner.commands.insertContent(" two");
    expect(onEdit).toHaveBeenLastCalledWith("h1", "\n\n## Proposed head\n\n- one two");
    expect(editor.getJSON()).toEqual(before);
    expect(editor.isEditable).toBe(true);
  });

  it("DCR-FR-12, DCR-FR-24: moving the review keeps the surface and moves the mark", () => {
    const editor = prompt("Body.\n\nTail.");
    const hunks = [insertion(), insertion({ id: "h2", from: 7, to: 7, position: 2, total: 2 })];
    push(editor, { hunks, focused: "h1" });
    const first = surfaceIn(editor).editor;
    push(editor, { hunks, focused: "h2" });
    const boxes = [...editor.view.dom.querySelectorAll<HTMLElement>(".hunk--add")];
    expect(boxes.map((b) => b.dataset.focused)).toEqual(["false", "true"]);
    expect(surfaceIn(editor).editor).toBe(first);
    expect(first?.isDestroyed).toBe(false);
  });

  it("DCR-FR-24: an edit reaches the callback the review holds now", () => {
    const editor = prompt("Body.");
    const stale = vi.fn();
    const current = vi.fn();
    push(editor, { hunks: [insertion()], focused: "h1", onEdit: stale });
    push(editor, { hunks: [insertion()], focused: "h1", onEdit: current });
    const inner = surfaceIn(editor).editor as Editor;
    inner.commands.insertContent("x");
    expect(current).toHaveBeenCalledTimes(1);
    expect(stale).not.toHaveBeenCalled();
  });

  it("DCR-FR-24, DCR-FR-17: a change that leaves the review frees its surface", () => {
    const editor = prompt("Body.");
    push(editor, { hunks: [insertion()], focused: "h1" });
    const inner = surfaceIn(editor).editor as Editor;
    push(editor, { hunks: [insertion({ state: "accepted" })], focused: null });
    expect(editor.view.dom.querySelector(".hunk--add")).toBeNull();
    expect(inner.isDestroyed).toBe(true);
  });

  it("DCR-FR-24, DCR-FR-25: a rebuilt surface shows the author's own text", () => {
    const editor = prompt("Body.");
    push(editor, { hunks: [insertion()], focused: "h1" });
    const first = surfaceIn(editor).editor as Editor;
    // A revised `after` is a new widget (DCR-FR-WRJP); the author's text wins.
    push(editor, {
      hunks: [insertion({ after: "Revised.", draft: "Author's own." })],
      focused: "h1",
    });
    expect(first.isDestroyed).toBe(true);
    expect(surfaceIn(editor).textContent).toBe("Author's own.");
  });
});
