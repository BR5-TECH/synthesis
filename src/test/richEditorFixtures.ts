/**
 * A WYSIWYG surface for tests of its keyboard behavior (EDT-FR-FDGH and the
 * inline-code requirements of `EDT-editor.md`).
 *
 * The surface is mounted with the same extension set as every rich surface
 * (`markdownExtensions`). Key presses go to it as DOM key events. Text that is
 * not a key press of interest goes in through the text-input chain, as the
 * platform's own typing does.
 */
import { Editor } from "@tiptap/react";
import { GHOST_CLASS } from "../components/markdownInlineCode";
import { markdownExtensions } from "../components/markdownFidelity";

let mounted: Editor | null = null;

/** Mount a surface on Markdown `content`. `destroyRichEditor` removes it. */
export function mountRichEditor(content: string, editable = true): Editor {
  destroyRichEditor();
  mounted = new Editor({ extensions: markdownExtensions(), content, editable });
  return mounted;
}

export function destroyRichEditor(): void {
  mounted?.destroy();
  mounted = null;
}

/** Put the cursor at the end of the document's last textblock. */
export function cursorAtEnd(ed: Editor): void {
  ed.commands.setTextSelection(ed.state.doc.content.size - 1);
}

export interface KeyOptions {
  key?: string;
  code?: string;
  shiftKey?: boolean;
  metaKey?: boolean;
  ctrlKey?: boolean;
  altKey?: boolean;
  isComposing?: boolean;
  keyCode?: number;
}

/** Send one keydown to the surface. Returns true when its default was prevented. */
export function press(ed: Editor, options: KeyOptions = {}): boolean {
  const { keyCode, ...init } = options;
  const event = new KeyboardEvent("keydown", {
    key: "`",
    bubbles: true,
    cancelable: true,
    ...init,
  });
  if (keyCode !== undefined) {
    Object.defineProperty(event, "keyCode", { value: keyCode });
  }
  return !ed.view.dom.dispatchEvent(event);
}

export function pressBackticks(ed: Editor, count: number): void {
  for (let i = 0; i < count; i += 1) press(ed);
}

/** Type text the way the platform's text input puts it in. */
export function typeText(ed: Editor, text: string): void {
  for (const ch of text) {
    const view = ed.view;
    const { from, to } = view.state.selection;
    const insert = () => view.state.tr.insertText(ch, from, to);
    const handled = view.someProp("handleTextInput", (handle) =>
      handle(view, from, to, ch, insert),
    );
    if (!handled) view.dispatch(insert());
  }
}

/**
 * Press a key as the platform does: when the surface does not take the key,
 * the platform types `platformText` for it.
 */
export function pressAsPlatform(
  ed: Editor,
  platformText: string,
  options: KeyOptions = {},
): void {
  if (!press(ed, options)) typeText(ed, platformText);
}

/** The text that carries the inline-code mark, in document order. */
export function codeMarkedText(ed: Editor): string {
  const codeType = ed.schema.marks.code;
  let marked = "";
  ed.state.doc.descendants((node) => {
    if (node.isText && codeType.isInSet(node.marks)) marked += node.text;
  });
  return marked;
}

/** The virtual closing backtick the surface shows, or null. */
export function ghost(ed: Editor): HTMLElement | null {
  return ed.view.dom.querySelector<HTMLElement>(`.${GHOST_CLASS}`);
}
