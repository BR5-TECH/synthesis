/**
 * Where the comment cards go when the tab is too narrow to hold them beside the
 * page (CMT-FR-64).
 *
 * The tab spends its width in a fixed order. The page keeps its measure always;
 * what it gives up first is its *position* — the leading field, which is empty
 * anyway, is handed to the margin so the cards land beside the prose instead of
 * over it. That much is pure CSS (`--page-lead` in `kit.css`), because it is a
 * continuous function of the tab's width and needs no decision.
 *
 * The second step is a decision, and this is where it is made: once the page is
 * as far leading as it will go and a card still does not fit, the cards leave
 * the margin altogether for a column below the page. That is a different
 * arrangement rather than a different size — the cards stop aligning to their
 * anchors and stop travelling with the body — so the rail has to be told, and a
 * media query cannot tell it.
 */
import { useEffect, useState, type RefObject } from "react";

/** Beside the page, or in a column under it (CMT-FR-64). */
export type CommentArrangement = "beside" | "below";

/**
 * The metrics below mirror tokens in `colors_and_type.css`. They are duplicated
 * here because the arrangement is decided in JS and the geometry is declared in
 * CSS; `style-invariants.test.ts` asserts the two still agree, so a token edited
 * on one side fails rather than silently moving the breakpoint on the other.
 */
/** `--doc-page-outer`: the page's whole box, its hairline included. */
export const PAGE_OUTER = 918;
/** `--doc-page-inset`: how far the page is inset from the tab (EDT-FR-63). */
export const PAGE_INSET = 16;
/** `--comment-card-min-w`: the narrowest measure a card stays readable at. */
export const CARD_MIN_WIDTH = 232;
/** `--sp-6` against the frame plus `--sp-4` between a card and the prose. */
export const MARGIN_GUTTERS = 24 + 16;

/**
 * The narrowest tab that can still hold a card beside the page — with the page
 * slid as far toward the leading edge as its own inset allows, which is what
 * makes this a smaller number than the width at which a *centred* page would
 * leave the same margin.
 */
export const BESIDE_MIN_WIDTH =
  PAGE_OUTER + PAGE_INSET + CARD_MIN_WIDTH + MARGIN_GUTTERS;

/**
 * CMT-FR-64: beside the page while the margin can hold a card, below it once it
 * cannot.
 *
 * A width of zero is not a narrow tab, it is an unmeasured one — a tab that has
 * not been laid out yet, or one under a test renderer that reports no geometry
 * at all. Reading it as narrow would flip every rail into the stacked
 * arrangement for one frame on the way in, so it is read as the ordinary one,
 * and so is anything else that is not a real measurement. `NaN` in particular
 * fails every comparison, so it would otherwise fall through to the stacked
 * arrangement — the exact opposite of what the zero case is for.
 */
export function railArrangement(tabWidth: number): CommentArrangement {
  if (!Number.isFinite(tabWidth) || tabWidth <= 0) return "beside";
  return tabWidth >= BESIDE_MIN_WIDTH ? "beside" : "below";
}

/**
 * Watch a tab's own width and report which arrangement its comments take.
 *
 * A `ResizeObserver` rather than a `resize` listener, because the tab is not the
 * window: dragging the Library panel wider narrows the tab without the window
 * changing size at all, and the cards have to move for that too. The listener is
 * the fallback for environments that have no observer.
 */
export function useCommentArrangement(
  ref: RefObject<HTMLElement | null>,
): CommentArrangement {
  const [arrangement, setArrangement] = useState<CommentArrangement>("beside");

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const measure = () =>
      setArrangement(railArrangement(el.getBoundingClientRect().width));
    measure();
    if (typeof ResizeObserver === "undefined") {
      window.addEventListener("resize", measure);
      return () => window.removeEventListener("resize", measure);
    }
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, [ref]);

  return arrangement;
}
