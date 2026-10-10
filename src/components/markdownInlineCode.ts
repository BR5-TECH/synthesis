/**
 * The open inline-code span of the WYSIWYG surface (EDT-FR-LLBU), read from
 * the editor state alone.
 *
 * A span is open while text typed at the cursor joins it: the code mark is in
 * the stored marks, or the cursor sits in or at the end of code-marked text
 * (the code mark is inclusive). Nothing here keeps state of its own, so a span
 * that is still empty disappears when the cursor moves, because ProseMirror
 * clears the stored marks on every selection change (EDT-FR-VCOH).
 */
import type { Mark, MarkType, Node as PMNode, ResolvedPos } from "@tiptap/pm/model";
import type { EditorState } from "@tiptap/pm/state";
import { Decoration, DecorationSet } from "@tiptap/pm/view";

export const BACKTICK = "`";

/** The class of the virtual closing backtick. */
export const GHOST_CLASS = "code-ghost";

export function codeMarkType(state: EditorState): MarkType | undefined {
  return state.schema.marks.code;
}

export function hasCode(node: PMNode | null | undefined, code: MarkType): boolean {
  return !!node && code.isInSet(node.marks) !== undefined;
}

/** The empty cursor, outside a fenced code block, or null. */
export function proseCursor(state: EditorState): ResolvedPos | null {
  const { selection } = state;
  if (!selection.empty) return null;
  const $cursor = selection.$from;
  if ($cursor.parent.type.spec.code) return null;
  return $cursor;
}

/** The marks that text typed at the cursor takes. */
function marksAtCursor(state: EditorState, $cursor: ResolvedPos): readonly Mark[] {
  return state.storedMarks ?? $cursor.marks();
}

/** Whether text typed at the cursor joins an inline-code span. */
export function isCodeOpen(state: EditorState): boolean {
  const code = codeMarkType(state);
  const $cursor = proseCursor(state);
  if (!code || !$cursor) return false;
  return code.isInSet(marksAtCursor(state, $cursor)) !== undefined;
}

/**
 * Whether the open span holds no text yet: it was opened by a backtick a
 * moment ago. A span opened directly after inline code joins that code and so
 * is not empty (EDT-FR-VCOH); inline code after the cursor does not count.
 */
export function isEmptyOpenSpan(state: EditorState): boolean {
  const code = codeMarkType(state);
  const $cursor = proseCursor(state);
  if (!code || !$cursor || !state.storedMarks) return false;
  return (
    code.isInSet(state.storedMarks) !== undefined &&
    !hasCode($cursor.nodeBefore, code)
  );
}

/** The position where the code-marked run at and after the cursor ends. */
function spanEnd($cursor: ResolvedPos, code: MarkType): number {
  const parent = $cursor.parent;
  let pos = $cursor.pos - $cursor.textOffset;
  let index = $cursor.index();
  while (index < parent.childCount && hasCode(parent.child(index), code)) {
    pos += parent.child(index).nodeSize;
    index += 1;
  }
  return pos;
}

function ghostElement(): HTMLElement {
  const ghost = document.createElement("span");
  ghost.className = GHOST_CLASS;
  ghost.textContent = BACKTICK;
  ghost.setAttribute("aria-hidden", "true");
  ghost.contentEditable = "false";
  return ghost;
}

/**
 * EDT-FR-LLBU: the virtual closing backtick at the end of the open span. It is
 * a decoration, so the document, and so the buffer and the saved file, never
 * hold it.
 */
export function ghostDecorations(state: EditorState): DecorationSet {
  const code = codeMarkType(state);
  const $cursor = proseCursor(state);
  if (!code || !$cursor || !isCodeOpen(state)) return DecorationSet.empty;
  const mark = code.isInSet(marksAtCursor(state, $cursor)) ?? code.create();
  const at = hasCode($cursor.nodeAfter, code) ? spanEnd($cursor, code) : $cursor.pos;
  return DecorationSet.create(state.doc, [
    Decoration.widget(at, ghostElement, {
      side: 1,
      marks: [mark],
      ignoreSelection: true,
      key: GHOST_CLASS,
    }),
  ]);
}
