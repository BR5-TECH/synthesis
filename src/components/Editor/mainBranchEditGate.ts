/**
 * EDT-FR-QXEK / EDT-FR-UPNU: the rich surface's half of the first-change hold.
 *
 * A change of the author is not applied while the gate holds, and the
 * confirmation shows. A write the editor makes itself, a load, a reseed or a
 * mode switch, goes through `setContent` with no update event, which marks the
 * transaction `preventUpdate`, and passes. So does a transaction that a plugin
 * appends to a transaction that passed, for example the trailing node that
 * follows a heading, a list or a code fence, and a transaction that stays out
 * of the history. Opening a file therefore never shows the confirmation
 * (MBW-FR-QKVA).
 */
import { Extension } from "@tiptap/react";
import { Plugin, PluginKey, type Transaction } from "@tiptap/pm/state";

import { holdEdit } from "../../state/mainBranchWrite";

/**
 * Whether the transaction is a change of the author's, not of the editor.
 *
 * An appended transaction is never one: ProseMirror filters it only after its
 * root transaction passed, and the root carries the mark of the editor or the
 * gate was open. ProseMirror sets the `appendedTransaction` mark after the
 * filter, so the filter cannot read it. The caller tells the two apart.
 */
export function isAuthorChange(tr: Transaction, appended = false): boolean {
  if (appended || !tr.docChanged) return false;
  return !tr.getMeta("preventUpdate") && tr.getMeta("addToHistory") !== false;
}

export const MainBranchEditGate = Extension.create({
  name: "mainBranchEditGate",
  addProseMirrorPlugins() {
    const editor = this.editor;
    return [
      new Plugin({
        key: new PluginKey("mainBranchEditGate"),
        // A root transaction is filtered against the state of the view. An
        // appended one is filtered against the state that its root made.
        filterTransaction: (tr, state) => !(isAuthorChange(tr, state !== editor.state) && holdEdit()),
      }),
    ];
  },
});
