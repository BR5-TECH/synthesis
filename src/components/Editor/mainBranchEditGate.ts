/**
 * EDT-FR-QXEK / EDT-FR-UPNU: the rich surface's half of the first-change hold.
 *
 * A change of the author is not applied while the gate holds, and the
 * confirmation shows. A write the editor makes itself, a load, a reseed or a
 * mode switch, goes through `setContent` with no update event, which marks the
 * transaction `preventUpdate`, and passes.
 */
import { Extension } from "@tiptap/react";
import { Plugin, PluginKey } from "@tiptap/pm/state";

import { holdEdit } from "../../state/mainBranchWrite";

export const MainBranchEditGate = Extension.create({
  name: "mainBranchEditGate",
  addProseMirrorPlugins() {
    return [
      new Plugin({
        key: new PluginKey("mainBranchEditGate"),
        filterTransaction: (tr) =>
          !(tr.docChanged && !tr.getMeta("preventUpdate") && holdEdit()),
      }),
    ];
  },
});
