/**
 * The backtick key and inline code in the WYSIWYG surface (EDT-FR-FDGH,
 * EDT-FR-VCOH, EDT-FR-CCBX, EDT-FR-APAL, EDT-FR-XTGK, EDT-FR-EKEZ,
 * EDT-FR-FKJP), and the virtual closing backtick (EDT-FR-LLBU).
 *
 * The surface acts on the backtick key itself: it cancels the key event, so
 * the platform's text input cannot add a character of its own. The macOS
 * webview can add a second backtick for one press. Then the press does what
 * the cursor's place asks for: it opens an inline-code span, closes the open
 * one, wraps the selection, or types a literal backtick.
 *
 * A key press that types a backtick counts on any layout, also when the layout
 * needs Option or AltGr (which arrives as Control and Alt together) for it. A
 * composed backtick (a dead key reports `Dead`, an input method reports keyCode
 * 229), a Command or Control shortcut, and a node or cell selection stay with
 * the platform. ProseMirror itself does not send a key to this handler while
 * the view is read-only or composing.
 */
import { Extension } from "@tiptap/react";
import type { MarkType } from "@tiptap/pm/model";
import { NodeSelection, Plugin, PluginKey, TextSelection } from "@tiptap/pm/state";
import { CellSelection } from "@tiptap/pm/tables";
import type { EditorView } from "@tiptap/pm/view";
import {
  BACKTICK,
  codeMarkType,
  ghostDecorations,
  hasCode,
  isCodeOpen,
  isEmptyOpenSpan,
  proseCursor,
} from "./markdownInlineCode";

/** Whether this key event types a backtick, rather than composing one or being a shortcut. */
function typesBacktick(event: KeyboardEvent): boolean {
  if (event.key !== BACKTICK) return false;
  if (event.metaKey || (event.ctrlKey && !event.altKey)) return false;
  return !event.isComposing && event.keyCode !== 229;
}

/** The arrow key that moves forward in the text direction of the cursor's paragraph. */
function forwardArrow(view: EditorView): string {
  const { node } = view.domAtPos(view.state.selection.from);
  const element = node instanceof Element ? node : node.parentElement;
  const direction = element
    ? element.ownerDocument.defaultView?.getComputedStyle(element).direction
    : undefined;
  return direction === "rtl" ? "ArrowLeft" : "ArrowRight";
}

function isPlainKey(event: KeyboardEvent, key: string): boolean {
  return (
    event.key === key &&
    !event.shiftKey &&
    !event.metaKey &&
    !event.ctrlKey &&
    !event.altKey
  );
}

/** Put `text` in at the selection as literal text, with the marks typed text takes. */
function insertLiteral(view: EditorView, text: string): void {
  const { from, to } = view.state.selection;
  view.dispatch(view.state.tr.insertText(text, from, to).scrollIntoView());
}

/** EDT-FR-CCBX: close the open span. The cursor stays, and nothing is inserted. */
function closeSpan(view: EditorView, code: MarkType): void {
  view.dispatch(view.state.tr.removeStoredMark(code));
}

/**
 * EDT-FR-FKJP: offer the backtick to the input rules, as typed text is
 * offered. The inline-code rule closes a literal opening backtick this way.
 */
function offerToInputRules(view: EditorView): boolean {
  const { from, to } = view.state.selection;
  const insert = () => view.state.tr.insertText(BACKTICK, from, to);
  return (
    view.someProp("handleTextInput", (handle) =>
      handle(view, from, to, BACKTICK, insert),
    ) ?? false
  );
}

/** What a backtick does at an empty selection in prose. */
function backtickAtCursor(view: EditorView, code: MarkType): void {
  const { state } = view;
  const $cursor = state.selection.$from;
  if (isCodeOpen(state)) {
    // EDT-FR-XTGK: an empty span becomes two literal backticks.
    if (isEmptyOpenSpan(state)) {
      const tr = state.tr.removeStoredMark(code);
      view.dispatch(tr.insertText(BACKTICK + BACKTICK).scrollIntoView());
      return;
    }
    // EDT-FR-FKJP: inside a span, before its end, a backtick is literal code.
    if (hasCode($cursor.nodeAfter, code)) return insertLiteral(view, BACKTICK);
    return closeSpan(view, code);
  }
  if (offerToInputRules(view)) return;
  // EDT-FR-XTGK: a backtick right after a literal one is literal, so a fence
  // can be typed.
  const before = $cursor.nodeBefore;
  if (before?.isText && !hasCode(before, code) && before.text?.endsWith(BACKTICK)) {
    return insertLiteral(view, BACKTICK);
  }
  // EDT-FR-VCOH: open an empty span. Nothing goes into the document.
  view.dispatch(state.tr.addStoredMark(code.create()));
}

/** EDT-FR-APAL: what a backtick does with a non-empty selection. */
function backtickOnSelection(view: EditorView, code: MarkType): void {
  const { state } = view;
  const { $from, $to, from, to } = state.selection;
  if (!$from.sameParent($to) || !$from.parent.inlineContent) {
    view.dispatch(state.tr.deleteSelection());
    if (proseCursor(view.state)) backtickAtCursor(view, code);
    else insertLiteral(view, BACKTICK);
    return;
  }
  const tr = state.tr.addMark(from, to, code.create());
  // The cursor at the end of code-marked text keeps the span open.
  tr.setSelection(TextSelection.create(tr.doc, to));
  view.dispatch(tr.scrollIntoView());
}

function onBacktick(view: EditorView): void {
  const { state } = view;
  const code = codeMarkType(state);
  // EDT-FR-FKJP: in a fenced code block, a backtick is literal text.
  if (!code || state.selection.$from.parent.type.spec.code) {
    return insertLiteral(view, BACKTICK);
  }
  if (state.selection.empty) backtickAtCursor(view, code);
  else backtickOnSelection(view, code);
}

/** EDT-FR-CCBX: the forward arrow at the end of the open span closes it in place. */
function onForwardArrow(view: EditorView): boolean {
  const code = codeMarkType(view.state);
  const $cursor = proseCursor(view.state);
  if (!code || !$cursor || !isCodeOpen(view.state)) return false;
  if (hasCode($cursor.nodeAfter, code)) return false;
  closeSpan(view, code);
  return true;
}

/** Whether `run` ends with exactly one backtick. */
function endsWithOneBacktick(run: string): boolean {
  return run.endsWith(BACKTICK) && !run.endsWith(BACKTICK + BACKTICK);
}

/** Whether `run` starts with exactly one backtick. */
function startsWithOneBacktick(run: string): boolean {
  return run.startsWith(BACKTICK) && !run.startsWith(BACKTICK + BACKTICK);
}

/**
 * EDT-FR-EKEZ: text typed between exactly one literal backtick on each side
 * becomes inline code, and the span stays open. Returns false where the text
 * is ordinary typing.
 */
function pairToCode(view: EditorView, from: number, to: number, text: string): boolean {
  const { state } = view;
  const code = codeMarkType(state);
  if (!code || from !== to || text.includes(BACKTICK)) return false;
  const $cursor = proseCursor(state);
  if (!$cursor || $cursor.pos !== from || isCodeOpen(state)) return false;
  const before = $cursor.nodeBefore;
  const after = $cursor.nodeAfter;
  if (!before?.isText || !after?.isText) return false;
  if (hasCode(before, code) || hasCode(after, code)) return false;
  if (!endsWithOneBacktick(before.text ?? "")) return false;
  if (!startsWithOneBacktick(after.text ?? "")) return false;
  const tr = state.tr.replaceWith(
    from - 1,
    from + 1,
    state.schema.text(text, [code.create()]),
  );
  // The cursor at the end of code-marked text keeps the span open.
  tr.setSelection(TextSelection.create(tr.doc, from - 1 + text.length));
  view.dispatch(tr.scrollIntoView());
  return true;
}

export const LiteralBacktick = Extension.create({
  name: "literalBacktick",
  // Ahead of the table keymap, which takes the arrow keys in a table cell, so
  // the forward arrow closes a span in a cell too (EDT-FR-CCBX).
  priority: 1000,
  addProseMirrorPlugins() {
    return [
      new Plugin({
        key: new PluginKey("literalBacktick"),
        props: {
          handleKeyDown(view, event) {
            if (isPlainKey(event, forwardArrow(view))) return onForwardArrow(view);
            if (!typesBacktick(event)) return false;
            // A cell or node selection keeps the platform's own handling.
            const { selection } = view.state;
            if (selection instanceof NodeSelection || selection instanceof CellSelection) {
              return false;
            }
            event.preventDefault();
            onBacktick(view);
            return true;
          },
          handleTextInput(view, from, to, text) {
            return pairToCode(view, from, to, text);
          },
          decorations: ghostDecorations,
        },
      }),
    ];
  },
});
