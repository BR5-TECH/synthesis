/**
 * EDT-FR-SCSP / EDT-FR-IPMX / EDT-FR-HAGB: the chart types that the Insert
 * diagram button offers, and the insert of a diagram fence that holds the
 * example of one of them.
 */
import type { Editor as TiptapEditor } from "@tiptap/react";
import { TextSelection } from "@tiptap/pm/state";
import { HINT_SETS } from "../DiagramEditor/hints";
import { DIAGRAM_LANGUAGE, mermaidKey } from "../markdownMermaid";

export interface DiagramTemplate {
  key: string;
  /** The name of the chart type that the menu shows. */
  name: string;
  /**
   * EDT-FR-OWNT: the Mermaid keyword that declares the chart type, such as
   * `sequenceDiagram`, which the filter of the menu also matches.
   */
  keyword: string;
  /** A short example that the bundled Mermaid version parses. */
  example: string;
}

/**
 * EDT-FR-SCSP: the chart types that the menu lists first, in this order. The
 * other chart types of the hint sets come after them.
 */
export const LEADING_TEMPLATE_KEYS: readonly string[] = [
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

/** The hint sets that draw no diagram: `info` shows the library version. */
const NOT_A_DIAGRAM = new Set(["info"]);

/**
 * EDT-FR-SCSP: one template per chart type of the hint sets that draws a
 * diagram. The examples are the ones the hint sets bundle, which a test checks
 * against the bundled Mermaid version (EDT-FR-QJJP).
 */
export const DIAGRAM_TEMPLATES: readonly DiagramTemplate[] = (() => {
  const sets = HINT_SETS.filter((set) => !NOT_A_DIAGRAM.has(set.key));
  const rank = (key: string) => {
    const at = LEADING_TEMPLATE_KEYS.indexOf(key);
    return at === -1 ? LEADING_TEMPLATE_KEYS.length : at;
  };
  return sets
    .map((set, index) => ({ set, index }))
    .sort((a, b) => rank(a.set.key) - rank(b.set.key) || a.index - b.index)
    .map(({ set }) => ({
      key: set.key,
      name: set.name,
      keyword: set.declaration.code.split(/\s/)[0] ?? "",
      example: set.example,
    }));
})();

/**
 * EDT-FR-IPMX / EDT-FR-HAGB: insert a diagram fence that holds `source`.
 *
 * When the caret is in an empty paragraph, the fence replaces it. Otherwise
 * the fence is a new block after the top-level block that holds the caret. The
 * fence shows its source with the caret in it. The insert is one transaction,
 * so the surface records it as one ordinary edit. Returns false when the
 * surface accepts no input.
 */
export function insertDiagramFence(editor: TiptapEditor, source: string): boolean {
  if (!editor.isEditable) return false;
  const { state } = editor;
  const codeBlock = state.schema.nodes.codeBlock;
  if (!codeBlock) return false;
  const fence = codeBlock.create(
    { language: DIAGRAM_LANGUAGE },
    source === "" ? null : state.schema.text(source),
  );
  const { selection } = state;
  const { $from } = selection;
  const tr = state.tr;
  let at: number;

  // Only a top-level block is replaced or followed. A fence in a table cell
  // cannot be written as Markdown, and one in a list or a quote is not where
  // the author asked for a new block.
  if ($from.depth === 1 && $from.parent.type.name === "paragraph" && $from.parent.content.size === 0) {
    at = $from.before(1);
    tr.replaceWith(at, $from.after(1), fence);
  } else {
    // A selection of a whole top-level block (an image, a fence) is at depth
    // 0, and the fence goes after the end of that selection.
    at = $from.depth > 0 ? $from.after(1) : selection.to;
    tr.insert(at, fence);
  }

  tr.setSelection(TextSelection.create(tr.doc, at + 1));
  // EDT-FR-IPMX: the new fence shows its source, so the author can edit the
  // example at once. The view is view state, carried by the same transaction.
  tr.setMeta(mermaidKey, { type: "show", pos: at, view: "source" });
  tr.scrollIntoView();
  editor.view.dispatch(tr);
  editor.view.focus();
  return true;
}
