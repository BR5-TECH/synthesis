import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useLayoutEffect,
  useRef,
} from "react";
import type { ReactNode } from "react";
import { logDebug } from "../logging";
import { holdTabScroll, tabScrollOf } from "../state/tabScroll";

/** The id of the tab whose content renders below, or null outside a tab. */
const TabScrollContext = createContext<string | null>(null);

/**
 * Names the tab that the surfaces below belong to, so each of them can keep its
 * scroll position for that tab (EDT-FR-UERT, NAW-FR-IHDU, DFV-FR-YMJA).
 */
export function TabScrollScope({
  tabId,
  children,
}: {
  tabId: string;
  children: ReactNode;
}) {
  return (
    <TabScrollContext.Provider value={tabId}>
      {children}
    </TabScrollContext.Provider>
  );
}

/** A scroller that comes to rest this near the target has reached it. */
const SLACK = 1;

/** Input from the author that ends a restore that still waits for content. */
const AUTHOR_INPUT = ["wheel", "pointerdown", "keydown", "touchstart"];

export interface RememberedScrollInput {
  /** The element that scrolls now, or null while it is not rendered. */
  scroller: () => HTMLElement | null;
  /**
   * Which of the tab's scrolled surfaces shows now. Each surface keeps its own
   * offset.
   */
  surface: string;
  /**
   * Changes whenever the content can have grown, so a restore that is still
   * waiting for a tall enough document tries again.
   */
  content: unknown;
  /** Called after each restore step with the offset the scroller shows. */
  onRestored?: (top: number) => void;
}

/**
 * EDT-FR-UERT / EDT-FR-NYCP / NAW-FR-IHDU / DFV-FR-YMJA: keep a tab's scroll
 * position while the tab is open.
 *
 * The first surface that renders in a mount goes back to the offset the tab
 * held for it, and the restore holds it there until the author scrolls or
 * gives input. Content that arrives later (the rest of the document, an image,
 * the decorations of a proposed change) changes or resizes what is on screen,
 * and the restore applies the offset again each time. That also covers content
 * that is not yet tall enough for the offset. While the restore holds, the
 * browser's scroll anchoring is off, so it does not move the scroller to a
 * place the author never saw. A surface that replaces another one in the same
 * mount (a mode switch) starts at the top. The same surface on a new scroller
 * keeps its offset.
 *
 * Returns the handler that the scroller's own scroll handler calls. Outside a
 * `TabScrollScope` it does nothing.
 */
export function useRememberedScroll({
  scroller,
  surface,
  content,
  onRestored,
}: RememberedScrollInput): () => void {
  const tabId = useContext(TabScrollContext);
  /** The tab and the surface this mount shows, once it has restored. */
  const shown = useRef<{ tabId: string; surface: string } | null>(null);
  /** The offset a restore holds the scroller at, or null once the author took over. */
  const pending = useRef<number | null>(null);
  /** The offset the last restore step put the scroller at. */
  const applied = useRef(0);
  /** The scroller the last layout effect saw. */
  const lastScroller = useRef<HTMLElement | null>(null);
  /** Ends the restore that holds the scroller, and turns anchoring back on. */
  const releaseRef = useRef<(() => void) | null>(null);
  const scrollerRef = useRef(scroller);
  scrollerRef.current = scroller;
  const restoredRef = useRef(onRestored);
  restoredRef.current = onRestored;

  const apply = useCallback(() => {
    const el = scrollerRef.current();
    const target = pending.current;
    if (!el || target === null) return;
    if (Math.abs(el.scrollTop - target) > SLACK) el.scrollTop = target;
    applied.current = el.scrollTop;
    restoredRef.current?.(applied.current);
  }, []);

  // Layout rather than effect: the restored offset is on screen in the frame
  // the tab comes back in, not one frame after it showed the top.
  useLayoutEffect(() => {
    if (tabId === null) return;
    const el = scrollerRef.current();
    if (!el) return;
    const moved = lastScroller.current !== null && lastScroller.current !== el;
    lastScroller.current = el;
    const was = shown.current;
    if (was === null || was.tabId !== tabId) {
      shown.current = { tabId, surface };
      pending.current = null;
      const held = tabScrollOf(tabId, surface);
      if (held <= 0) {
        // Another tab in the same mount starts at the top. A first mount
        // already does.
        if (was !== null) el.scrollTop = 0;
        return;
      }
      pending.current = held;
      apply();
      logDebug(["frontend"], "a tab restored its scroll position", {
        offset: held,
        waiting: applied.current < held - SLACK,
      });
      return;
    }
    if (was.surface !== surface) {
      // EDT-FR-UERT: a mode switch is not a return to the tab. The new surface
      // starts at the top, and so does its next restore.
      shown.current = { tabId, surface };
      pending.current = null;
      el.scrollTop = 0;
      holdTabScroll(tabId, surface, 0);
      return;
    }
    if (moved && pending.current === null) {
      // DFV-FR-52: the same surface on a new scroller (a Diff mode switch
      // between side-by-side and the other modes) keeps the offset.
      const held = tabScrollOf(tabId, surface);
      if (held > 0) pending.current = held;
    }
    apply();
  }, [tabId, surface, content, apply]);

  // EDT-FR-NYCP / DFV-FR-YMJA: while a restore holds, content that grows
  // without a render (an image that loads) applies it again, and input from
  // the author ends it.
  useEffect(() => {
    if (tabId === null || pending.current === null) return;
    const el = scrollerRef.current();
    if (!el) return;
    const anchoring = el.style.overflowAnchor;
    el.style.overflowAnchor = "none";
    const release = () => {
      pending.current = null;
      el.style.overflowAnchor = anchoring;
    };
    releaseRef.current = release;
    AUTHOR_INPUT.forEach((type) =>
      el.addEventListener(type, release, { passive: true }),
    );
    let observer: ResizeObserver | null = null;
    if (typeof ResizeObserver !== "undefined") {
      observer = new ResizeObserver(() => apply());
      Array.from(el.children).forEach((child) => observer!.observe(child));
    }
    return () => {
      AUTHOR_INPUT.forEach((type) => el.removeEventListener(type, release));
      observer?.disconnect();
      el.style.overflowAnchor = anchoring;
      if (releaseRef.current === release) releaseRef.current = null;
    };
  }, [tabId, surface, content, apply]);

  return useCallback(() => {
    const at = shown.current;
    if (tabId === null || at === null || at.tabId !== tabId) return;
    const el = scrollerRef.current();
    if (!el) return;
    if (pending.current !== null) {
      // The scroll event of a restore step reports the offset it applied. Any
      // other offset is the author's.
      if (Math.abs(el.scrollTop - applied.current) <= SLACK) return;
      if (releaseRef.current) releaseRef.current();
      else pending.current = null;
    }
    holdTabScroll(tabId, at.surface, el.scrollTop);
  }, [tabId]);
}
