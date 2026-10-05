/**
 * DCR-FR-09 / DCR-FR-24: a change's proposed text, rendered and edited as the
 * Markdown it is.
 *
 * The text a change removes is the document's own text, so the WYSIWYG surface
 * renders it rich where it stands. The text a change proposes is beside the
 * document rather than in it, so it gets a small editing surface of its own on
 * the Editor's extension set: a heading in it reads as a heading and a list as
 * a list, and the two halves of a replacement read in one typography.
 *
 * A plain Tiptap editor rather than a React one: ProseMirror creates and
 * destroys the widget this mounts into as the document changes, and the
 * returned teardown is what the widget's own `destroy` calls.
 */
import { Editor, Extension, type AnyExtension } from "@tiptap/react";
import { history, redo, undo } from "@tiptap/pm/history";

import { logWarn } from "../../logging";
import { getMarkdown, markdownExtensions } from "../markdownFidelity";

export interface ProposedTextOptions {
  /** DCR-FR-KDSV: a legacy proposal's text is read and never edited. */
  editable: boolean;
  /** DCR-FR-10: the accessible name of the textbox. */
  label: string;
  /** DCR-FR-24: the proposed text as the author has rewritten it. */
  onChange?: (markdown: string) => void;
}

/**
 * DCR-FR-24: an undo issued in the proposed text reverses an edit to it.
 *
 * The Editor's extension set runs without a history of its own, because an
 * Editor tab serves undo from the tab-wide history. The proposed text is not
 * that artifact, so it keeps its own, and the tab leaves an undo issued here to
 * it (`REVIEW_SELECTOR` in `../Editor/props.ts`). Without this, the tab would
 * take the undo and reverse an edit to the prompt instead (DCR-FR-02).
 */
const LocalHistory = Extension.create({
  name: "proposedTextHistory",
  addProseMirrorPlugins() {
    return [history()];
  },
  addKeyboardShortcuts() {
    return {
      "Mod-z": () => undo(this.editor.state, this.editor.view.dispatch),
      "Shift-Mod-z": () => redo(this.editor.state, this.editor.view.dispatch),
      "Mod-y": () => redo(this.editor.state, this.editor.view.dispatch),
    };
  },
});

/**
 * DCR-FR-09: the Editor's extension set, for a surface that holds one change.
 *
 * Without the trailing empty paragraph the Editor keeps after a list or a
 * heading: in a block of its own, that paragraph appears on the first edit as
 * an empty band at the foot of the change, and it writes nothing.
 */
function proposedTextExtensions() {
  return markdownExtensions([LocalHistory]).map((extension) =>
    extension.name === "starterKit"
      ? (extension as AnyExtension).configure({ trailingNode: false })
      : extension,
  );
}

/** The class a proposed text's own surface carries, inside `hunk__body`. */
export const PROPOSED_DOC_CLASS = "hunk__doc";

/** The class a proposed text carries when it could only be shown as written. */
export const PROPOSED_PLAIN_CLASS = "hunk__plain";

/**
 * Mount the proposed text into `host`. Returns the teardown.
 *
 * `markdown` is drawn without the blank lines that separate it from what it
 * follows, and an edit is reported with them put back: they are part of the
 * change and are written when it is accepted, so an edit must not drop them.
 */
export function mountProposedText(
  host: HTMLElement,
  markdown: string,
  opts: ProposedTextOptions,
): () => void {
  const lead = /^(?:\r?\n)*/.exec(markdown)?.[0] ?? "";
  // Read off what follows the lead, so text that is all blank lines is one
  // separator and not two.
  const rest = markdown.slice(lead.length);
  const trail = /(?:\r?\n)*$/.exec(rest)?.[0] ?? "";
  const body = rest.slice(0, rest.length - trail.length);
  // The surface writes LF. A text written with CRLF gets its edit back in CRLF,
  // so one change never holds both.
  const eol = markdown.includes("\r\n") ? "\r\n" : "\n";
  const editable = opts.editable && opts.onChange !== undefined;
  const attributes: Record<string, string> = {
    // `doc` is what typesets the Editor tab's WYSIWYG surface, so the proposed
    // text reads exactly as the prompt around it does (DCR-FR-09).
    class: `doc ${PROPOSED_DOC_CLASS}`,
    role: "textbox",
    "aria-multiline": "true",
    "aria-label": opts.label,
    spellcheck: "false",
  };
  if (!editable) attributes["aria-readonly"] = "true";

  let editor: Editor;
  try {
    editor = new Editor({
      element: host,
      extensions: proposedTextExtensions(),
      content: body,
      editable,
      editorProps: { attributes },
      onUpdate: ({ editor: self, transaction }) => {
        // Only a change to the text is an edit. Tiptap also emits `update` for
        // transactions that change nothing the author wrote, and each of those
        // would otherwise store the serialisation as though they had typed it.
        if (!transaction.docChanged) return;
        const written = getMarkdown(self)
          .replace(/^(?:\r?\n)+|(?:\r?\n)+$/g, "")
          .replace(/\r?\n/g, eol);
        opts.onChange?.(`${lead}${written}${trail}`);
      },
    });
  } catch (error) {
    // The text is still the author's to read: shown as it was written, and not
    // offered for editing, since nothing could serialise an edit back.
    logWarn(["frontend"], "could not render a proposed change as Markdown", {
      error: error instanceof Error ? error.name : "unknown",
    });
    host.textContent = body;
    host.classList.add(PROPOSED_PLAIN_CLASS);
    return () => {};
  }
  return () => editor.destroy();
}
