/**
 * LIB-FR-BXMZ: render only the rows in view and a small margin around them.
 *
 * Every tree row has the same height, so the position of a row is its index
 * times that height. Two spacers stand for the rows above and below the
 * window, so the scroll range shows the full length of the tree.
 */
import { useEffect, useState, type ReactNode, type RefObject } from "react";

/**
 * The height of one `.tree-row` on screen, in px: `height: 24px` plus 2 px of
 * padding above and below, as the box is sized by its content (`header.css`).
 * A change of that rule must change this value too.
 */
export const ROW_HEIGHT = 28;

/** Rows rendered above and below the window. */
export const OVERSCAN = 20;

/**
 * The viewport height used when the scroll element reports none, as jsdom
 * does. It keeps a small tree whole in a test, and still bounds a big one.
 */
const FALLBACK_VIEWPORT = 960;

interface VirtualRowsProps<T> {
  rows: T[];
  scrollRef: RefObject<HTMLElement | null>;
  renderRow: (row: T, index: number) => ReactNode;
  /** LIB-FR-18: the index to bring into view, with a nonce per request. */
  scrollTo?: { index: number; nonce: number } | null;
}

export function VirtualRows<T>({ rows, scrollRef, renderRow, scrollTo }: VirtualRowsProps<T>) {
  const [view, setView] = useState({ top: 0, height: 0 });

  // A passive effect, not a layout effect: the scroll element is the parent
  // of these rows, and React attaches a parent's ref only after the layout
  // effects of its children ran. A layout effect here reads a null ref on
  // mount and never listens to the scroll.
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const read = () => setView({ top: el.scrollTop, height: el.clientHeight });
    read();
    el.addEventListener("scroll", read, { passive: true });
    const observer =
      typeof ResizeObserver === "undefined" ? null : new ResizeObserver(() => read());
    observer?.observe(el);
    return () => {
      el.removeEventListener("scroll", read);
      observer?.disconnect();
    };
  }, [scrollRef]);

  // LIB-FR-18: a revealed row outside the window is scrolled to, so it is
  // rendered and its own `scrollIntoView` can settle it.
  const targetIndex = scrollTo?.index ?? -1;
  const targetNonce = scrollTo?.nonce;
  useEffect(() => {
    const el = scrollRef.current;
    if (!el || targetIndex < 0) return;
    const top = targetIndex * ROW_HEIGHT;
    const bottom = top + ROW_HEIGHT;
    if (top < el.scrollTop || bottom > el.scrollTop + el.clientHeight) {
      el.scrollTop = Math.max(0, top - el.clientHeight / 2);
      setView({ top: el.scrollTop, height: el.clientHeight });
    }
  }, [scrollRef, targetIndex, targetNonce]);

  const height = view.height > 0 ? view.height : FALLBACK_VIEWPORT;
  const first = Math.max(0, Math.floor(view.top / ROW_HEIGHT) - OVERSCAN);
  const count = Math.ceil(height / ROW_HEIGHT) + 2 * OVERSCAN;
  const last = Math.min(rows.length, first + count);
  const slice = rows.slice(first, last);
  return (
    <>
      {first > 0 && <div aria-hidden="true" style={{ height: first * ROW_HEIGHT }} />}
      {slice.map((row, i) => renderRow(row, first + i))}
      {last < rows.length && (
        <div aria-hidden="true" style={{ height: (rows.length - last) * ROW_HEIGHT }} />
      )}
    </>
  );
}
