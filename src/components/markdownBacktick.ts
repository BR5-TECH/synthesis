/**
 * EDT-FR-FDGH: one press of the backtick key puts exactly one backtick into the
 * WYSIWYG surface.
 *
 * The macOS webview can put a second backtick into the page for one key press,
 * and then neither the inline-code rule nor the code-fence rule matches what
 * the author typed. This extension takes the key away from the platform's text
 * input: it cancels the key event and puts the backtick in itself, through the
 * same text-input chain that native typing goes through. Tiptap's input rules
 * are on that chain, so `` `text` `` still becomes inline code and ```` ``` ````
 * then Space or Enter still opens a code block.
 *
 * A key press that types a backtick counts on any layout, also when the layout
 * needs Option or AltGr (which arrives as Control and Alt together) for it. A
 * composed backtick (a dead key reports `Dead`, an input method reports keyCode
 * 229), a Command or Control shortcut, and a node or cell selection stay with
 * the platform. ProseMirror itself does not send a key to this handler while
 * the view is read-only or composing.
 */
import { Extension } from "@tiptap/react";
import { Plugin, PluginKey, TextSelection } from "@tiptap/pm/state";
import type { EditorView } from "@tiptap/pm/view";

const BACKTICK = "`";

/** Whether this key event types a backtick, rather than composing one or being a shortcut. */
function typesBacktick(event: KeyboardEvent): boolean {
  if (event.key !== BACKTICK) return false;
  if (event.metaKey || (event.ctrlKey && !event.altKey)) return false;
  return !event.isComposing && event.keyCode !== 229;
}

/** Insert one backtick at the selection, as typed text is inserted. */
function typeBacktick(view: EditorView): void {
  const { from, to } = view.state.selection;
  const insert = () => view.state.tr.insertText(BACKTICK, from, to);
  const handled = view.someProp("handleTextInput", (handle) =>
    handle(view, from, to, BACKTICK, insert),
  );
  if (!handled) view.dispatch(insert().scrollIntoView());
}

export const LiteralBacktick = Extension.create({
  name: "literalBacktick",
  addProseMirrorPlugins() {
    return [
      new Plugin({
        key: new PluginKey("literalBacktick"),
        props: {
          handleKeyDown(view, event) {
            if (!typesBacktick(event)) return false;
            // A cell or node selection keeps the platform's own handling.
            if (!(view.state.selection instanceof TextSelection)) return false;
            event.preventDefault();
            typeBacktick(view);
            return true;
          },
        },
      }),
    ];
  },
});
