/**
 * DCR-FR-30: moving the review to a change scrolls the **document** to it.
 *
 * Without this the counter and the chip move to a change that may be a screen
 * away, and the author is told which change they are on while looking at a
 * different one — which is worse than not moving at all, because the accept
 * accelerator then acts on something off screen.
 *
 * The first change the review lands on is **not** scrolled to. A tab that
 * opened with a proposal standing is already showing the author where they
 * left the document, and yanking it on mount would move a page nobody asked to
 * move (DDS-FR-WCHN's rule for the other column, held here for the same
 * reason).
 */
import { useEffect, useRef } from "react";

import { HUNK_ATTR } from "./hunkDecorations";

/** How much of the scroller is kept above the change, so it is not at the lip. */
const HEADROOM = 96;

export function useScrollToHunk(
  hostRef: React.RefObject<HTMLElement | null>,
  /**
   * The proposal under review, which is what "opened" means here.
   *
   * A draft may hold a second proposal on the same open tab once the first is
   * resolved, and the surface is not rebuilt for it. Keyed on the host alone,
   * the rule below would read the second proposal's first change as a move
   * from the first proposal's last one and yank a document nobody asked to
   * move — the one thing this hook exists to avoid.
   */
  proposalId: string | null,
  focusedId: string | null,
): void {
  const seen = useRef<{ proposal: string; hunk: string } | null>(null);
  useEffect(() => {
    const host = hostRef.current;
    if (!host || focusedId === null || proposalId === null) return;
    // The change this review opened on, which the author is already looking at.
    if (seen.current === null || seen.current.proposal !== proposalId) {
      seen.current = { proposal: proposalId, hunk: focusedId };
      return;
    }
    if (seen.current.hunk === focusedId) return;
    seen.current = { proposal: proposalId, hunk: focusedId };

    const marked = host.querySelector<HTMLElement>(
      `[${HUNK_ATTR}="${CSS.escape(focusedId)}"]`,
    );
    // A change the document could not place has nowhere to scroll to. The
    // review still moves to it, because rejecting is how such a change is
    // cleared (DCR-FR-31) and the author reaches it through the bar.
    if (!marked) return;

    const a = marked.getBoundingClientRect();
    const b = host.getBoundingClientRect();
    const top = Math.max(0, a.top - b.top + host.scrollTop - HEADROOM);
    // `scrollTo` is not everywhere, and the eased move is a preference rather
    // than the behaviour: what must happen is that the document arrives at the
    // change, which the assignment does on its own.
    if (typeof host.scrollTo === "function") {
      const eased = !window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
      host.scrollTo({ top, behavior: eased ? "smooth" : "auto" });
    } else {
      host.scrollTop = top;
    }
  }, [hostRef, proposalId, focusedId]);
}
