/**
 * The Comments panel's filter text (CMP-FR-14 / CMP-FR-15).
 *
 * This lives in a hook held by `VPanel` rather than inside `Comments` for the
 * reason the Library's and the Notes panel's state does: choosing another
 * vertical-panel surface unmounts `<Comments>` outright, and CMP-FR-15 requires
 * the text to survive exactly that. `VPanel`'s lifetime is one project + one
 * content root, which is also the boundary the text must not cross.
 *
 * Deliberately *not* persisted, unlike the Notes panel's filter: CMP-FR-15 makes
 * this session memory, so the field is empty on a fresh launch and no
 * project-settings round-trip stands between the panel and its first render.
 */
import { useState } from "react";

/** What `Comments` renders from and mutates. */
export interface CommentsPanel {
  text: string;
  setText: (text: string) => void;
}

export function useCommentsPanelState(): CommentsPanel {
  const [text, setText] = useState("");
  return { text, setText };
}
