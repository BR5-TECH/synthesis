import { useEffect, useRef, useState } from "react";
import type { Operation } from "../types";

/** STB-FR-NAEX: how long work must stay in flight before the bar shows it. */
export const SHOW_DELAY_MS = 400;
/** STB-FR-JWLN: the shortest time the bar stays once it shows. */
export const MIN_VISIBLE_MS = 600;
/** STB-FR-JWLN: how long the bar stays after the last operation ends. */
export const HIDE_DELAY_MS = 400;

/**
 * STB-FR-NAEX / STB-FR-JWLN / STB-FR-KPRU: the operation the center region
 * shows, or `undefined` when it shows nothing (STB-FR-07).
 *
 * `operations` is the real in-flight set, newest first (STB-FR-05). The bar
 * shows only after that set has stayed non-empty for {@link SHOW_DELAY_MS}, so
 * the many short operations the project watcher causes never flash it. Once
 * shown, it stays until the set has been empty for {@link HIDE_DELAY_MS} and
 * it has been shown for {@link MIN_VISIBLE_MS}. During that hold it keeps the
 * last operation it displayed, and an operation that starts replaces that one
 * in place, so back-to-back work reads as one steady bar.
 */
export function useSteadyProgress(operations: Operation[]): Operation | undefined {
  const head = operations[0];
  const busy = head !== undefined;
  const [visible, setVisible] = useState(false);
  const shownAt = useRef(0);
  // STB-FR-JWLN: the operation the hold keeps once nothing is in flight.
  const lastHead = useRef<Operation | undefined>(undefined);
  if (head) lastHead.current = head;

  // STB-FR-NAEX: the delay runs while the set stays non-empty. It does not
  // restart when one operation follows another, and it stops when the set
  // goes empty, so work that ends sooner never shows.
  useEffect(() => {
    if (visible || !busy) return;
    const timer = setTimeout(() => {
      shownAt.current = Date.now();
      setVisible(true);
    }, SHOW_DELAY_MS);
    return () => clearTimeout(timer);
  }, [visible, busy]);

  // STB-FR-JWLN: the hide waits for both the idle delay and the minimum
  // visible time. An operation that starts in that time cancels it.
  useEffect(() => {
    if (!visible || busy) return;
    const minLeft = MIN_VISIBLE_MS - (Date.now() - shownAt.current);
    const timer = setTimeout(
      () => setVisible(false),
      Math.max(HIDE_DELAY_MS, minLeft),
    );
    return () => clearTimeout(timer);
  }, [visible, busy]);

  return visible ? (head ?? lastHead.current) : undefined;
}
