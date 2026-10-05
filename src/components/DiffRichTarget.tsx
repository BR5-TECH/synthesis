/**
 * The editable **target** of a comparison, rendered rich (`DFV-diff-viewer.md`
 * DFV-FR-47, `DCR-draft-change-review.md` DCR-FR-07).
 *
 * DFV-FR-47 asks for "the same Markdown editing semantics the Editor's WYSIWYG
 * surface carries". That is a requirement about *which editor*, and the honest
 * reading of it is the one taken here: a stretch of target document is the
 * Editor's own surface — one ProseMirror instance over the whole contiguous run
 * of it, seeded from those lines, written back through the same serialisation
 * (EDT-FR-66, EDT-FR-67, EDT-FR-69).
 *
 * The alternative — mounting a little editor over whichever single block has
 * focus — reads as a widget rather than as a document: the caret cannot cross
 * from one paragraph to the next, Enter cannot open a new one, and the surface
 * appears and disappears under the pointer. So the unit of editing here is the
 * **run**: every consecutive target block the visualization renders together,
 * edited as one document, spliced back into the lines it occupied.
 *
 * What the tab adds over the Editor is that the diff survives the editing. The
 * change marking is not baked into the content — it cannot be, or the marking
 * would be typed into and serialised out to disk — so it is carried as
 * ProseMirror **decorations**, computed from the comparison and mapped through
 * the author's own transactions until the comparison is re-derived. A block the
 * edit changes keeps its mark attached to the text that moved, rather than to
 * the position it used to be at.
 */
import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
  type ReactNode,
} from "react";
import { EditorContent, Extension, useEditor } from "@tiptap/react";
import { Plugin, PluginKey } from "@tiptap/pm/state";
import { Decoration, DecorationSet } from "@tiptap/pm/view";
import type { Node as PmNode } from "@tiptap/pm/model";
import { logDebug } from "../logging";
import { getMarkdown, markdownExtensions } from "./markdownFidelity";
import type { TargetEditing } from "./DiffTarget";
import type { Range } from "../diff/wordDiff";

/** How a rendered block reads against the revision it is being compared to. */
export type BlockMark = "added" | "removed" | "none";

/**
 * One block's marking, in the order the run's blocks occupy the document.
 *
 * Kept separate from the block itself because the run does not render blocks —
 * it renders a document, and this is what says which of that document's units
 * carry which treatment.
 */
export interface RichUnitMark {
  mark: BlockMark;
  /**
   * The text `ranges` were measured against, when it is not simply the unit's
   * own text.
   *
   * The block parse reads a task item's `- [ ] ` checkbox as part of the item's
   * text; the schema reads it as an attribute of the node, so the node's text is
   * that much shorter and every range measured against the parse would fall past
   * the end of it — the marking silently dropped rather than misplaced. Given
   * the basis, the ranges are shifted back onto the text that is actually there.
   */
  basis?: string;
  /** The block kind the parse read, carried through so the marking reads alike
      in both renderings of the target (`data-kind` on a static block). */
  kind: string;
  /** DFV-FR-19: the words that differ inside a replaced block. */
  ranges?: Range[];
}

/**
 * Beyond this many runs on one screen, the runs mount on demand instead of all
 * at once.
 *
 * A ProseMirror view is not free — a document observer, a plugin stack, and its
 * own DOM per instance — and side-by-side pairs every block into a row of its
 * own, so a wholesale rewrite of a long file would otherwise ask for a view per
 * block before the tab could paint. Measured in the browser, a mount is well
 * under a millisecond, so the limit is set where the arithmetic stops being
 * comfortable rather than where it starts to cost anything: under it (which is
 * every ordinary diff, side-by-side included) the target is live throughout and
 * there is nothing to activate; over it, a run becomes live when the author
 * reaches for it, with the caret placed where they clicked so the swap is not
 * something they have to notice.
 */
export const LIVE_RUN_LIMIT = 200;

// ---------------------------------------------------------------------------
// Marking, as decorations
// ---------------------------------------------------------------------------

const diffMarksKey = new PluginKey<DecorationSet>("diffMarks");

/**
 * The document's blocks, in the granularity `parseMarkdownBlocks` marks at
 * (DFV-FR-19): a list is not one unit but one per item, and a table is one per
 * row, so a one-word edit marks the item rather than the list.
 *
 * Walked in document order, which is the order the block parse produced, so the
 * two zip index-for-index without either having to describe itself to the other.
 */
function markableUnits(doc: PmNode): { pos: number; node: PmNode }[] {
  const units: { pos: number; node: PmNode }[] = [];
  doc.descendants((node, pos, parent) => {
    const name = node.type.name;
    if (
      name === "heading" ||
      name === "codeBlock" ||
      name === "horizontalRule" ||
      name === "blockquote" ||
      name === "tableRow"
    ) {
      units.push({ pos, node });
      // A quote and a table row are one unit each, so their inner paragraphs
      // are not units of their own.
      return false;
    }
    if (name === "listItem" || name === "taskItem") {
      units.push({ pos, node });
      // Descended into, because a nested list's items are units too — and they
      // follow their parent item in the parse exactly as they do here.
      return true;
    }
    if (name === "paragraph") {
      if (parent?.type.name === "doc") units.push({ pos, node });
      return false;
    }
    return true;
  });
  return units;
}

/** The `<mark>` runs inside one unit, from offsets into its rendered text. */
function wordDecorations(
  unit: PmNode,
  base: number,
  ranges: Range[],
  basis?: string,
): Decoration[] {
  const out: Decoration[] = [];
  // What the ranges have to be moved by to address this node's own text.
  const own = unit.textBetween(0, unit.content.size, "\n", "");
  const shift =
    basis && basis !== own && basis.endsWith(own) ? basis.length - own.length : 0;
  let offset = 0;
  unit.descendants((child, pos) => {
    if (!child.isText) return true;
    const start = offset;
    offset += child.text?.length ?? 0;
    for (const [rangeStart, rangeEnd] of ranges) {
      const from = Math.max(start, rangeStart - shift);
      const to = Math.min(offset, rangeEnd - shift);
      if (from >= to) continue;
      // `pos` is relative to the unit's content, which begins one position
      // inside the unit itself.
      out.push(
        Decoration.inline(
          base + 1 + pos + (from - start),
          base + 1 + pos + (to - start),
          { nodeName: "mark", class: "diff-word" },
        ),
      );
    }
    return false;
  });
  return out;
}

/**
 * Every unit carries `data-mark`, including an unchanged one, so the treatment
 * is a property of the rendering rather than something only changed blocks have
 * — which is what lets the marker glyph and the read/write distinction be
 * expressed once in the stylesheet.
 */
/** The node a block of this kind is, in the schema the run is edited with. */
const NODE_OF: Record<string, readonly string[]> = {
  heading: ["heading"],
  paragraph: ["paragraph"],
  "list-item": ["listItem", "taskItem"],
  code: ["codeBlock"],
  quote: ["blockquote"],
  rule: ["horizontalRule"],
  "table-row": ["tableRow"],
};

function buildDecorations(doc: PmNode, units: RichUnitMark[]): DecorationSet {
  const found = markableUnits(doc);
  const decorations: Decoration[] = [];
  for (const [index, { pos, node }] of found.entries()) {
    const unit = units[index];
    if (!unit) break;
    // The zip is by position, so it holds only while the two readings of the
    // same lines agree on what is there. They can disagree: the block parse is
    // a renderer's parse rather than a specification-complete one, and a
    // construct it splits (a setext heading, which it reads as a paragraph
    // followed by a rule) is one node to the schema. From the first block where
    // they part company the positions have slipped, and marking on regardless
    // would say a paragraph nobody touched is the one that changed — so the
    // marking stops there. What is marked is right; what is past the
    // disagreement is simply unmarked.
    if (!NODE_OF[unit.kind]?.includes(node.type.name)) break;
    decorations.push(
      Decoration.node(pos, pos + node.nodeSize, {
        "data-mark": unit.mark,
        "data-kind": unit.kind,
        // DFV-FR-41 / DFV-FR-19: the sign character, so an addition and a
        // removal are told apart without the tint. Drawn by the stylesheet from
        // this attribute rather than inserted into the document, because
        // anything inserted into the document is content the author can type
        // into and the serialiser would write to disk.
        ...(unit.mark === "added"
          ? { "data-glyph": "+" }
          : unit.mark === "removed"
            ? { "data-glyph": "−" }
            : {}),
      }),
    );
    if (unit.ranges?.length) {
      decorations.push(...wordDecorations(node, pos, unit.ranges, unit.basis));
    }
  }
  return DecorationSet.create(doc, decorations);
}

interface DiffMarksOptions {
  /** Read late, so a re-derivation does not have to rebuild the extension. */
  units: () => RichUnitMark[];
}

/**
 * The marking, held as decorations and **mapped** through every transaction.
 *
 * Mapping rather than recomputing is what keeps the marking honest while the
 * author types: the comparison behind it is re-derived only once they stop
 * (DFV-FR-43), and in between, a mark that stayed at a fixed document position
 * would slide onto whatever text the edit pushed into that position.
 */
const DiffMarks = Extension.create<DiffMarksOptions>({
  name: "diffMarks",
  addOptions() {
    return { units: () => [] };
  },
  addProseMirrorPlugins() {
    const read = this.options.units;
    return [
      new Plugin<DecorationSet>({
        key: diffMarksKey,
        state: {
          init: (_config, state) => buildDecorations(state.doc, read()),
          apply: (tr, set, _old, next) =>
            tr.getMeta(diffMarksKey)
              ? buildDecorations(next.doc, read())
              : set.map(tr.mapping, tr.doc),
        },
        props: {
          decorations: (state) => diffMarksKey.getState(state) ?? null,
        },
      }),
    ];
  },
});

/**
 * The document position under a pointer, or null where there is not one to be
 * had.
 *
 * `posAtCoords` reads the layout, which a surface mounted this tick may not have
 * yet — and asks the document for `elementFromPoint`, which not every host
 * implements. Neither is a reason to lose the click: the caller falls back to
 * the end of the run, which is where a caret with nowhere better to go belongs.
 */
function posAt(
  view: { posAtCoords: (at: { left: number; top: number }) => { pos: number } | null },
  at: { left: number; top: number },
): number | null {
  try {
    return view.posAtCoords(at)?.pos ?? null;
  } catch (error) {
    logDebug(["frontend"], "diff rich target: no position under the pointer", {
      error: String(error),
    });
    return null;
  }
}

/** A cheap identity for a marking, so an unchanged one is not rebuilt. */
function markingKey(units: RichUnitMark[]): string {
  return units
    .map(
      (u) =>
        `${u.mark}/${u.kind}:${(u.ranges ?? []).map((r) => r.join("-")).join(",")}`,
    )
    .join("|");
}

// ---------------------------------------------------------------------------
// The run
// ---------------------------------------------------------------------------

/**
 * One contiguous stretch of the target, edited as the document it is.
 *
 * The run owns the lines `[from, to)` of the whole target and rewrites all of
 * them on every edit — the same bargain an Editor tab makes with the file it has
 * open, and the reason DFV-FR-47 can promise the Editor's serialisation
 * guarantees rather than a subset of them.
 */
export function EditableRichRun({
  source,
  from,
  to,
  units,
  editing,
  deferred = false,
  children,
}: {
  /** The target's own lines `[from, to)`, verbatim. */
  source: string;
  from: number;
  to: number;
  /** One entry per block in the run, in document order. */
  units: RichUnitMark[];
  editing: TargetEditing;
  /** Mount on demand rather than immediately (see `LIVE_RUN_LIMIT`). */
  deferred?: boolean;
  /** The static rendering, shown while a deferred run has not been reached. */
  children: ReactNode;
}) {
  /**
   * Whether the author has reached for this run.
   *
   * Kept apart from `deferred` rather than seeding a `live` state from it: the
   * comparison behind the count is re-derived, so a screen that was over the
   * limit for one derivation can be under it for the next, and a state seeded
   * once would leave every run on that screen waiting for a click it no longer
   * needs.
   */
  const [reached, setReached] = useState(false);
  const live = reached || !deferred;
  const editingRef = useRef(editing);
  editingRef.current = editing;
  const unitsRef = useRef(units);
  unitsRef.current = units;
  /**
   * The lines this run currently occupies.
   *
   * Advanced by the run's own writes as well as by the comparison: an edit that
   * adds a line moves the run's end immediately, while the comparison behind the
   * props is deliberately not re-derived until the author rests (DFV-FR-43). A
   * range that waited for the derivation would splice the *next* edit over one
   * line too few.
   */
  const rangeRef = useRef({ from, to });
  useEffect(() => {
    rangeRef.current = { from, to };
  }, [from, to]);
  /** The last text this run and the target agreed on, in either direction. */
  const seenRef = useRef(source);
  const clickRef = useRef<{ left: number; top: number } | null>(null);

  const editor = useEditor(
    {
      // The Editor's own extension set, so a construct that survives a
      // round-trip there survives one here (EDT-FR-68).
      extensions: markdownExtensions([
        DiffMarks.configure({ units: () => unitsRef.current }),
      ]),
      editorProps: {
        attributes: {
          // `doc` is what typesets an Editor tab's WYSIWYG surface, so the
          // target reads here exactly as it reads there (DFV-FR-17).
          class: "doc diff-run__doc",
          // DFV-FR-50 / DCR-FR-28: which revision the keystrokes reach.
          "aria-label": editing.label,
        },
      },
      content: source,
      editable: !editing.disabled,
      onUpdate: ({ editor, transaction }) => {
        // Only a transaction that changed the DOCUMENT is an edit to the target.
        // Tiptap emits `update` for more than that — turning the surface
        // editable emits one, and so does a transaction carrying nothing but the
        // meta this component dispatches to refresh the marking — and every one
        // of those would write the run's serialisation back over its own lines.
        // With one run that is a spurious rewrite of the file; with one run per
        // block, as side-by-side has, each write shifts the lines the *next*
        // run addresses and the target is destroyed a block at a time.
        if (!transaction.docChanged) return;
        const markdown = getMarkdown(editor).replace(/\n+$/, "");
        seenRef.current = markdown;
        const { from: start, to: end } = rangeRef.current;
        const height = markdown === "" ? 0 : markdown.split("\n").length;
        rangeRef.current = { from: start, to: start + height };
        editingRef.current.replaceLines(start, end, markdown);
      },
    },
    [live],
  );

  // A target replaced from outside this surface — an undo that reached back
  // past the run, a resolved conflict, a load adopted into the session — is
  // adopted here. Our own writes are excluded by `seenRef`, which is what keeps
  // the caret still under typing; `editing.seed` is what says the replacement
  // came from elsewhere, because the text alone cannot: a traversal can restore
  // the very text this run was seeded with, and the run has been typed into
  // since.
  useEffect(() => {
    if (!editor || editor.isDestroyed) return;
    if (source === seenRef.current) return;
    seenRef.current = source;
    editor.commands.setContent(source, { emitUpdate: false });
  }, [editor, source, editing.seed]);

  // DFV-FR-53 / DCR-FR-29: an unresolved conflict makes the surface inert
  // without unmounting it, so nothing about the reading changes.
  useEffect(() => {
    if (!editor || editor.isDestroyed) return;
    const next = !editing.disabled;
    if (editor.isEditable === next) return;
    // `false`: do not announce this as an update. It is not one.
    editor.setEditable(next, false);
  }, [editor, editing.disabled]);

  const key = markingKey(units);
  useEffect(() => {
    if (!editor || editor.isDestroyed) return;
    editor.view.dispatch(
      editor.state.tr.setMeta(diffMarksKey, true).setMeta("preventUpdate", true),
    );
  }, [editor, key]);

  // A deferred run that has just been reached takes the caret where the pointer
  // put it, so activating it is not something the author can feel.
  useEffect(() => {
    if (!deferred || !live || !editor || editor.isDestroyed) return;
    const at = clickRef.current;
    clickRef.current = null;
    editor.commands.focus(at ? (posAt(editor.view, at) ?? "end") : "end");
  }, [deferred, live, editor]);

  const reach = useCallback((event?: ReactMouseEvent) => {
    if (event) clickRef.current = { left: event.clientX, top: event.clientY };
    setReached(true);
  }, []);

  if (!live) {
    // A run nobody has reached yet renders the static blocks it is, and says
    // what it will be: a textbox carrying the target's name, reachable by the
    // keyboard as well as the pointer. An unresolved conflict makes it inert
    // rather than something else (DFV-FR-53 / DCR-FR-29).
    return (
      <div
        className="diff-run"
        data-target="true"
        role="textbox"
        aria-multiline
        aria-label={editing.label}
        {...(editing.disabled
          ? { "aria-readonly": true }
          : {
              tabIndex: 0,
              onMouseDown: reach,
              onFocus: () => reach(),
            })}
      >
        {children}
      </div>
    );
  }
  return (
    <div className="diff-run" data-target="true" data-editing="true">
      <EditorContent editor={editor} />
    </div>
  );
}
