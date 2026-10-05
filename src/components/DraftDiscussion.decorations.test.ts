/**
 * What a proposal draws in the prose it changes
 * (`../../specifications/ui/DCR-draft-change-review.md` DCR-FR-05, DCR-FR-07,
 * DCR-FR-09, DCR-FR-12).
 *
 * Two defects these cases exist for, both of which a passing suite let through
 * because nothing here was drawn in a test at all. A rewrite showed its green
 * block and marked nothing as removed, so the author read a replacement as an
 * addition. And the green block was mounted inside the textblock its anchor
 * ended in, so a block after a heading read at the heading's size and weight —
 * and was a `div` inside an `h2` besides.
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Editor } from "@tiptap/react";
import { Schema } from "@tiptap/pm/model";
import type { Node as PMNode } from "@tiptap/pm/model";

import {
  buildDecorations,
  syncFocused,
  type DecoratedHunk,
} from "./DraftDiscussion/hunkDecorations";
import { REVIEW_SELECTOR } from "./Editor/props";

/**
 * Enough of the editing surface's schema to hold a heading, prose and a list.
 *
 * Built here rather than taken from Tiptap: what these cases are about is where
 * a decoration lands in a document tree, and the tree is the only part of the
 * editor that has any bearing on it.
 */
const schema = new Schema({
  nodes: {
    doc: { content: "block+" },
    paragraph: { group: "block", content: "inline*", toDOM: () => ["p", 0] },
    heading: {
      group: "block",
      content: "inline*",
      attrs: { level: { default: 2 } },
      toDOM: (node) => [`h${node.attrs.level as number}`, 0],
    },
    bullet_list: { group: "block", content: "list_item+", toDOM: () => ["ul", 0] },
    list_item: { content: "paragraph+", toDOM: () => ["li", 0] },
    text: { group: "inline" },
  },
  marks: {},
});

const heading = (text: string) =>
  schema.nodes.heading.create({ level: 2 }, schema.text(text));
const para = (text: string) =>
  schema.nodes.paragraph.create(null, schema.text(text));
const item = (text: string) =>
  schema.nodes.list_item.create(null, para(text));

function hunk(over: Partial<DecoratedHunk>): DecoratedHunk {
  return {
    id: "h1",
    kind: "replace",
    state: "pending",
    from: 0,
    to: 0,
    after: "the proposed text",
    lost: false,
    position: 1,
    total: 1,
    agent: "Helga",
    editable: true,
    ...over,
  };
}

/**
 * One decoration, in the shape these cases read it by.
 *
 * A widget covers no text, so `from === to` is what tells the two kinds apart
 * without reaching for anything private. The class and the widget's own element
 * are read off the decoration type, which is the only place either lives.
 */
interface Probe {
  from: number;
  to: number;
  spec?: { destroy?: (node: Node) => void; key?: string };
  type: {
    attrs?: Record<string, string>;
    toDOM?: HTMLElement | ((view: unknown, getPos: () => number) => HTMLElement);
  };
}

function drawn(
  doc: PMNode,
  hunks: DecoratedHunk[],
  focused: string | null = hunks[0]?.id ?? null,
): Probe[] {
  const set = buildDecorations(doc, { hunks, focused });
  return set.find() as unknown as Probe[];
}

const removals = (decos: Probe[]) =>
  decos.filter((d) =>
    // By token rather than by substring: `hunk--del-foot` contains `hunk--del`,
    // and a match on it would make three of the cases below pass for the wrong
    // reason the moment the foot stopped being a zero-width widget.
    (d.type.attrs?.class ?? "").split(/\s+/).includes("hunk--del"),
  );

function widgetElement(deco: Probe): HTMLElement {
  const toDOM = deco.type.toDOM;
  const el = typeof toDOM === "function" ? toDOM(null, () => deco.from) : (toDOM as HTMLElement);
  mounted.push(el);
  return el;
}

/**
 * A widget's proposed text mounts an editing surface of its own, and every one
 * a case draws is freed after it.
 */
const mounted: HTMLElement[] = [];
afterEach(() => {
  for (const el of mounted.splice(0)) {
    const surface = el.querySelector(".hunk__doc") as (Element & { editor?: Editor }) | null;
    surface?.editor?.destroy();
  }
});

function editorOf(el: HTMLElement): Editor {
  const surface = el.querySelector(".hunk__doc") as (Element & { editor?: Editor }) | null;
  if (!surface?.editor) throw new Error("no editing surface in the widget");
  return surface.editor;
}

const widgets = (decos: Probe[]) => decos.filter((d) => d.from === d.to);
const blocks = (decos: Probe[], className: string) =>
  widgets(decos).filter((d) => widgetElement(d).classList.contains(className));

describe("what each kind of change draws (DCR-FR-05, DCR-FR-07)", () => {
  // heading 0..8, paragraph 8..27 ("The old body." is 13 chars: 9..22, close 23)
  const doc = () =>
    schema.nodes.doc.create(null, [
      heading("Intent"),
      para("The old body."),
      para("The tail."),
    ]);

  it("DCR-FR-07: a replacement strikes the text it replaces and proposes its own", () => {
    const decos = drawn(doc(), [hunk({ kind: "replace", from: 9, to: 22 })]);
    expect(removals(decos)).toHaveLength(1);
    expect(removals(decos)[0]).toMatchObject({ from: 9, to: 22 });
    expect(blocks(decos, "hunk--add")).toHaveLength(1);
  });

  it("DCR-FR-07: a deletion strikes the text and proposes none", () => {
    const decos = drawn(doc(), [hunk({ kind: "del", after: null, from: 9, to: 22 })]);
    expect(removals(decos)).toHaveLength(1);
    expect(blocks(decos, "hunk--add")).toHaveLength(0);
  });

  it("DCR-FR-05, DCR-FR-07: an insertion marks no text as removed", () => {
    const decos = drawn(doc(), [hunk({ kind: "add", from: 22, to: 22 })]);
    expect(removals(decos)).toHaveLength(0);
    expect(blocks(decos, "hunk--add")).toHaveLength(1);
  });

  it("DCR-FR-05: an insertion that still carries a range marks no text either", () => {
    // A record written before the kind and the text had to agree can hold a
    // `before` an insertion never applies (DCP-FR-HRQN). Striking it through
    // would show the author a removal that accepting does not perform, which is
    // the one thing this surface must never do.
    const decos = drawn(doc(), [hunk({ kind: "add", from: 9, to: 22 })]);
    expect(removals(decos)).toHaveLength(0);
  });

  it("DCR-FR-17, DCR-FR-21: a decided change draws nothing at all", () => {
    for (const state of ["accepted", "rejected"] as const) {
      expect(drawn(doc(), [hunk({ state, from: 9, to: 22 })])).toHaveLength(0);
    }
  });
});

describe("where the change's own block sits (DCR-FR-09, DCR-FR-12)", () => {
  it("DCR-FR-09: an insertion after a heading is a sibling of it, never a child", () => {
    // The heading runs 0..8, its text 1..7. Mounted at the end of that text the
    // block would be a `div` inside the `h2` and would read at the heading's
    // size and weight.
    const doc = schema.nodes.doc.create(null, [heading("Intent"), para("Body.")]);
    const decos = drawn(doc, [hunk({ kind: "add", from: 7, to: 7 })]);
    const block = blocks(decos, "hunk--add");
    expect(block).toHaveLength(1);
    expect(block[0].from).toBe(8);
    expect(doc.resolve(block[0].from).parent.type.name).toBe("doc");
  });

  it("DCR-FR-09: a replacement's block follows the whole block it replaces text in", () => {
    const doc = schema.nodes.doc.create(null, [para("The old body."), para("Tail.")]);
    const block = blocks(drawn(doc, [hunk({ kind: "replace", from: 1, to: 14 })]), "hunk--add");
    expect(block[0].from).toBe(15);
    expect(doc.resolve(block[0].from).parent.type.name).toBe("doc");
  });

  it("DCR-FR-09: a block inside a list item stays inside that item", () => {
    // The boundary is the innermost textblock's, so the block lands where the
    // text it is about is rather than after the whole list.
    const doc = schema.nodes.doc.create(null, [
      schema.nodes.bullet_list.create(null, [item("First."), item("Second.")]),
    ]);
    const block = blocks(drawn(doc, [hunk({ kind: "add", from: 4, to: 4 })]), "hunk--add");
    const parent = doc.resolve(block[0].from).parent;
    // The rule is that the block is never inside a textblock, whatever holds it.
    expect(parent.isTextblock).toBe(false);
    expect(parent.type.name).toBe("list_item");
  });

  it("DCR-FR-05, DCR-FR-09: a change anchored at a block's head draws above it", () => {
    // An insertion naming no text it follows is written ABOVE the first block
    // (`placeHunk` gives it position 1). Drawn below it, the author would read
    // the change landing a paragraph further down than accepting puts it.
    const doc = schema.nodes.doc.create(null, [para("First para."), para("Second.")]);
    const block = blocks(drawn(doc, [hunk({ kind: "add", from: 1, to: 1 })]), "hunk--add");
    expect(block[0].from).toBe(0);
    expect(doc.resolve(block[0].from).parent.type.name).toBe("doc");
  });

  it("DCR-FR-09: the block is a real block element, so its height is its own", () => {
    // A deletion's foot reserves the chip's space by being tall. An inline box
    // takes no height from a rule, whatever the rule says — so the element the
    // rule lands on has to be a block.
    const doc = schema.nodes.doc.create(null, [para("The old body.")]);
    const decos = drawn(doc, [hunk({ kind: "del", after: null, from: 1, to: 14 })]);
    expect(widgetElement(blocks(decos, "hunk--del-foot")[0]).tagName).toBe("DIV");
    expect(widgetElement(blocks(drawn(doc, [hunk({ kind: "add", from: 1, to: 1 })]), "hunk--add")[0]).tagName).toBe("DIV");
  });

  it("DCR-FR-07: a change held for discussion carries that state on both its parts", () => {
    const doc = schema.nodes.doc.create(null, [para("The old body.")]);
    const decos = drawn(doc, [
      hunk({ kind: "del", after: null, state: "discussing", from: 1, to: 14 }),
    ]);
    expect(removals(decos)[0].type.attrs?.class).toContain("hunk--discussing");
    const foot = widgetElement(blocks(decos, "hunk--del-foot")[0]);
    expect(foot.classList.contains("hunk--discussing")).toBe(true);
  });

  it("DCR-FR-12: a change not under review is drawn, and marked as not under review", () => {
    const doc = schema.nodes.doc.create(null, [para("The old body.")]);
    const decos = drawn(doc, [hunk({ id: "h1", kind: "replace", from: 1, to: 14 })], null);
    expect(removals(decos)[0].type.attrs?.["data-focused"]).toBe("false");
    expect(widgetElement(blocks(decos, "hunk--add")[0]).dataset.focused).toBe("false");
  });

  it("DCR-FR-12: a deletion holds the chip's space open in a block of its own", () => {
    // An inline decoration reserves no height, so a deletion draws a block for
    // the chip. It carries the change's id, so the chip finds it and a press on
    // it still puts the change under review.
    const doc = schema.nodes.doc.create(null, [para("The old body."), para("Tail.")]);
    const decos = drawn(doc, [hunk({ kind: "del", after: null, id: "h7", from: 1, to: 14 })]);
    const foot = blocks(decos, "hunk--del-foot");
    expect(foot).toHaveLength(1);
    expect(foot[0].from).toBe(15);
    expect(widgetElement(foot[0]).getAttribute("data-hunk")).toBe("h7");
  });

  it("DCR-FR-12: a replacement and an insertion need no such block", () => {
    const doc = schema.nodes.doc.create(null, [para("The old body.")]);
    for (const kind of ["replace", "add"] as const) {
      const decos = drawn(doc, [hunk({ kind, from: 1, to: kind === "add" ? 1 : 14 })]);
      expect(blocks(decos, "hunk--del-foot"), kind).toHaveLength(0);
    }
  });

  it("DCR-FR-09: the proposed text renders its Markdown, and never raw HTML", () => {
    const doc = schema.nodes.doc.create(null, [para("Body.")]);
    const decos = drawn(doc, [
      hunk({ kind: "add", from: 1, to: 1, after: "## A heading <b>and</b> markup" }),
    ]);
    const el = widgetElement(blocks(decos, "hunk--add")[0]);
    expect(el.querySelector(".hunk__body .hunk__doc h2")?.textContent).toBe(
      "A heading <b>and</b> markup",
    );
    expect(el.querySelector("b")).toBeNull();
    expect(el.textContent).not.toContain("##");
  });

  it("DCR-FR-24, DCR-FR-25: the author's own text is drawn over the agent's", () => {
    const doc = schema.nodes.doc.create(null, [para("Body.")]);
    const decos = drawn(doc, [
      hunk({ kind: "add", from: 1, to: 1, after: "Agent words.", draft: "Author words." }),
    ]);
    const el = widgetElement(blocks(decos, "hunk--add")[0]);
    expect(el.querySelector(".hunk__doc")?.textContent).toBe("Author words.");
  });

  it("DCR-FR-24: an edit in the drawn text reaches the review as Markdown, keyed by its change", () => {
    const doc = schema.nodes.doc.create(null, [para("Body.")]);
    const onEdit = vi.fn();
    const set = buildDecorations(doc, {
      hunks: [hunk({ id: "h4", kind: "add", from: 1, to: 1, after: "\n\nOld" })],
      focused: "h4",
      onEdit,
    });
    const el = widgetElement(blocks(set.find() as unknown as Probe[], "hunk--add")[0]);
    const editor = editorOf(el);
    editor.commands.setTextSelection(editor.state.doc.content.size - 1);
    editor.commands.insertContent(" **new**");
    expect(onEdit).toHaveBeenLastCalledWith("h4", "\n\nOld **new**");
  });

  it("DCR-FR-KDSV, DCR-FR-10: a legacy change's text is drawn, named, and not editable", () => {
    // With an edit callback present, so it is the change's own flag that makes
    // the text read-only and not the absence of anyone to report an edit to.
    const doc = schema.nodes.doc.create(null, [para("Body.")]);
    const set = buildDecorations(doc, {
      hunks: [hunk({ kind: "add", from: 1, to: 1, editable: false })],
      focused: "h1",
      onEdit: vi.fn(),
    });
    const el = widgetElement(blocks(set.find() as unknown as Probe[], "hunk--add")[0]);
    const surface = el.querySelector(".hunk__doc");
    expect(surface?.getAttribute("contenteditable")).toBe("false");
    expect(surface?.getAttribute("aria-readonly")).toBe("true");
    expect(surface?.getAttribute("aria-label")).toBe(
      "Proposed text of Change 1 of 1, insertion proposed by Helga",
    );
  });

  it("DCR-FR-24: the drawn text's editing surface goes with its widget", () => {
    // ProseMirror calls a widget's `destroy` as it drops the node. A surface
    // left mounted after that is an editor nobody can reach and nothing frees.
    const doc = schema.nodes.doc.create(null, [para("Body.")]);
    const deco = blocks(drawn(doc, [hunk({ kind: "add", from: 1, to: 1 })]), "hunk--add")[0];
    const el = widgetElement(deco);
    const editor = editorOf(el);
    const destroy = deco.spec?.destroy;
    expect(destroy).toBeTypeOf("function");
    destroy?.(el);
    expect(editor.isDestroyed).toBe(true);
    expect(() => destroy?.(el)).not.toThrow();
    expect(() => destroy?.(document.createElement("div"))).not.toThrow();
  });

  it("DCR-FR-12, DCR-FR-24: putting a change under review does not rebuild its proposed text", () => {
    // A press in the proposed text puts its change under review. A widget keyed
    // on that would be rebuilt by the press, and the rebuild destroys the
    // surface the press has just put the caret in.
    const doc = schema.nodes.doc.create(null, [para("Body.")]);
    const one = [hunk({ id: "h1", kind: "add", from: 1, to: 1 })];
    const keyOf = (focused: string | null) =>
      blocks(drawn(doc, one, focused), "hunk--add")[0].spec?.key;
    expect(keyOf("h1")).toBeDefined();
    expect(keyOf("h1")).toBe(keyOf(null));
  });

  it("DCR-FR-12: the mark of the change under review is brought up to date in place", () => {
    const root = document.createElement("div");
    root.innerHTML =
      '<div class="hunk hunk--add" data-hunk="h1" data-focused="true"></div>' +
      '<div class="hunk hunk--add" data-hunk="h2" data-focused="false"></div>';
    syncFocused(root, "h2");
    const marks = [...root.querySelectorAll<HTMLElement>("[data-hunk]")].map((el) => el.dataset.focused);
    expect(marks).toEqual(["false", "true"]);
    syncFocused(root, null);
    expect([...root.querySelectorAll<HTMLElement>("[data-hunk]")].map((el) => el.dataset.focused)).toEqual([
      "false",
      "false",
    ]);
  });

  it("DCR-FR-24, DCR-FR-02: the tab leaves an undo issued in the proposed text to that text", () => {
    // The tab claims undo for the prompt. Claimed inside the proposed text, the
    // undo would reverse an edit to the prompt while a proposal stands.
    const doc = schema.nodes.doc.create(null, [para("Body.")]);
    const el = widgetElement(blocks(drawn(doc, [hunk({ kind: "add", from: 1, to: 1 })]), "hunk--add")[0]);
    const inner = el.querySelector(".hunk__doc p");
    expect(inner?.closest(REVIEW_SELECTOR)).not.toBeNull();
  });
});
