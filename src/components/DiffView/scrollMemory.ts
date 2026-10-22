import { useMemo, useRef } from "react";
import { useRememberedScroll } from "../../hooks/useRememberedScroll";

/**
 * DFV-FR-YMJA: the comparison comes back where the author left it when the tab
 * is activated again.
 *
 * Side-by-side brings its own scroller (DFV-FR-13), and every other mode
 * scrolls the body. The tab keeps one offset whatever the mode: the modes are
 * the application's (DFV-FR-52), so another Diff tab can change them while this
 * one is in the background.
 *
 * `content` changes whenever the comparison can have grown, and `layout`
 * names the visualization, which decides which element scrolls.
 */
export function useDiffScrollMemory(content: unknown, layout: string) {
  const bodyRef = useRef<HTMLDivElement>(null);
  const watched = useMemo(() => [content, layout], [content, layout]);
  const onScrollCapture = useRememberedScroll({
    scroller: () => {
      const body = bodyRef.current;
      return body?.querySelector<HTMLElement>(".diff-sbs__scroll") ?? body;
    },
    surface: "comparison",
    content: watched,
  });
  return { bodyRef, onScrollCapture };
}
