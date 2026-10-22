import type { Dispatch, RefObject, SetStateAction } from "react";
import { useRememberedScroll } from "../../hooks/useRememberedScroll";
import type { EditMode } from "../../state/editHistory";
import { documentSurface } from "../../state/tabScroll";
import { FRONTMATTER_TOP_THRESHOLD } from "./frontmatter";

export interface EditorScrollMemoryInput {
  artifactId: string;
  mode: EditMode;
  /** The rich body's scroller. */
  bodyScrollRef: RefObject<HTMLDivElement | null>;
  /** The source surface, which is its own scroller. */
  sourceRef: RefObject<HTMLTextAreaElement | null>;
  /** The highlight layer drawn behind the source surface. */
  sourceHlRef: RefObject<HTMLPreElement | null>;
  /** Whether the comment rail follows the source surface (CMT-FR-27). */
  railOnSource: boolean;
  /** What each surface shows, so a restore that waits for content retries. */
  docVersion: number;
  sourceText: string;
  setScrollTop: (top: number) => void;
  setAtTop: Dispatch<SetStateAction<boolean>>;
  setExpandOverride: Dispatch<SetStateAction<boolean | null>>;
}

/**
 * EDT-FR-UERT / EDT-FR-NYCP / NAW-FR-IHDU: the Editor tab keeps the scroll
 * position of its document, in the mode it is in.
 *
 * Returns the handler that both scrollers call from their own scroll handler.
 */
export function useEditorScrollMemory({
  artifactId,
  mode,
  bodyScrollRef,
  sourceRef,
  sourceHlRef,
  railOnSource,
  docVersion,
  sourceText,
  setScrollTop,
  setAtTop,
  setExpandOverride,
}: EditorScrollMemoryInput): () => void {
  return useRememberedScroll({
    scroller: () =>
      mode === "text" ? sourceRef.current : bodyScrollRef.current,
    surface: documentSurface(artifactId, mode),
    content: mode === "text" ? sourceText : docVersion,
    // EDT-FR-NYCP: the restored offset puts the comment rail and the
    // frontmatter region where a scroll to that offset puts them.
    onRestored: (top) => {
      if (mode === "text") {
        const hl = sourceHlRef.current;
        if (hl) hl.scrollTop = top;
        if (railOnSource) setScrollTop(top);
        return;
      }
      setScrollTop(top);
      setAtTop(top <= FRONTMATTER_TOP_THRESHOLD);
      setExpandOverride(null);
    },
  });
}
