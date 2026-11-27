/**
 * The jump to a heading of a rich surface (`specifications/ui/EDT-editor.md`
 * EDT-FR-YSJX, EDT-FR-HMJI), shared by the table of contents and by links.
 */
import type { Editor as TiptapEditor } from "@tiptap/react";
import { TextSelection } from "@tiptap/pm/state";

/**
 * EDT-FR-YSJX / EDT-FR-NHDM: put the caret at the start of the heading at
 * `pos` and scroll it to the top of the view. A change of selection only, so
 * the buffer, the dirty state, and the undo history stay as they are.
 */
export function revealRichHeading(editor: TiptapEditor, pos: number): boolean {
  const node = editor.state.doc.nodeAt(pos);
  if (!node || node.type.name !== "heading") return false;
  const { view } = editor;
  view.dispatch(
    view.state.tr.setSelection(TextSelection.create(view.state.doc, pos + 1)),
  );
  // Focus goes to the surface now, not in a later frame as the focus command
  // of the editor does. A focus that comes after the smooth scroll starts
  // puts the caret on screen in WebKit, and that cuts the scroll short.
  view.focus();
  const dom = view.nodeDOM(pos);
  if (dom instanceof HTMLElement && typeof dom.scrollIntoView === "function") {
    dom.scrollIntoView({ behavior: "smooth", block: "start" });
  }
  return true;
}
