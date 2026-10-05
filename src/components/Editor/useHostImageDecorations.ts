/**
 * Telling the editing surface what each of the document's image destinations
 * resolved to (`../../../specifications/ui/EDT-editor.md` EDT-FR-85,
 * `../../../specifications/ui/NAW-new-artifact.md` NAW-FR-51).
 *
 * What a destination resolved to is not part of the document — the author
 * typed a path, and whether that path names a file the draft holds is a fact
 * about the draft rather than about the text. So it is pushed in as
 * decorations, on a transaction carrying meta alone: no byte changes, nothing
 * is marked dirty, and no undo step is taken.
 */
import { useEffect } from "react";
import type { Editor as TiptapEditor } from "@tiptap/react";

import { hostImageKey } from "../hostImages";
import type { EditorImageHost } from "../hostImages";

export function useHostImageDecorations(
  editor: TiptapEditor | null,
  /** Absent on a surface that lends no image host, which resolves nothing. */
  imageHost: EditorImageHost | undefined,
  resolutions: Record<string, unknown>,
  /** The destination an insertion is in flight for, if one is. */
  insertingAt: number | null,
): void {
  useEffect(() => {
    if (!editor) return;
    const tr = editor.state.tr.setMeta(hostImageKey, {
      hosted: Boolean(imageHost),
      resolutions,
      inserting: insertingAt,
      insertingLabel: "Inserting image…",
    });
    tr.setMeta("addToHistory", false);
    editor.view.dispatch(tr);
  }, [editor, resolutions, insertingAt, imageHost]);
}
