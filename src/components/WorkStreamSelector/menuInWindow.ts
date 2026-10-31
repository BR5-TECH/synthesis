/**
 * WSS-FR-OSVE: keep the dropdown inside the window.
 *
 * The dropdown is anchored at its control, and a long branch label moves that
 * control to the right. An edge past the window would put a control out of
 * reach, and focus on it would scroll the whole shell sideways.
 */
import { useLayoutEffect, type RefObject } from "react";

const GUTTER = 8;

export function useMenuInWindow(
  menu: RefObject<HTMLElement | null>,
  open: boolean,
  /** Changes when the dropdown's width can. */
  contentKey: number,
): void {
  useLayoutEffect(() => {
    const element = menu.current;
    if (!open || !element) return;
    element.style.left = "";
    const box = element.getBoundingClientRect();
    const over = box.right - (window.innerWidth - GUTTER);
    if (over > 0) {
      element.style.left = `${-Math.min(over, box.left - GUTTER)}px`;
    }
  }, [menu, open, contentKey]);
}
