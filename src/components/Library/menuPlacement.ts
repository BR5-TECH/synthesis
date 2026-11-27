import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";

/** The gap kept between a menu and the window edge. */
const EDGE = 4;

/** LCM-FR-UCJY: how long the pointer rests before the open submenu changes. */
export const SUBMENU_INTENT_MS = 300;

/**
 * LCM-FR-PDXH: the position of a menu opened at `x`, `y`, moved up or left
 * until its box fits inside the window. Never pushed off the opposite edge.
 */
export function useInsideWindow(ref: React.RefObject<HTMLElement | null>, x: number, y: number) {
  const [position, setPosition] = useState({ left: x, top: y });
  useLayoutEffect(() => {
    const box = ref.current?.getBoundingClientRect();
    if (!box) return;
    const left = Math.max(EDGE, Math.min(x, window.innerWidth - box.width - EDGE));
    const top = Math.max(EDGE, Math.min(y, window.innerHeight - box.height - EDGE));
    setPosition((p) => (p.left === left && p.top === top ? p : { left, top }));
  }, [ref, x, y]);
  return position;
}

/**
 * LCM-FR-PDXH: the vertical offset of an open submenu from the top of its
 * parent entry, moved up until the submenu fits above the window bottom.
 */
export function useSubmenuShift(open: boolean) {
  const ref = useRef<HTMLDivElement | null>(null);
  const [top, setTop] = useState(0);
  useLayoutEffect(() => {
    if (!open) {
      setTop(0);
      return;
    }
    const box = ref.current?.getBoundingClientRect();
    if (!box) return;
    const natural = box.top - top;
    const overhang = natural + box.height - (window.innerHeight - EDGE);
    const shift = overhang > 0 ? -Math.min(overhang, Math.max(0, natural - EDGE)) : 0;
    if (shift !== top) setTop(shift);
  }, [open, top]);
  return { ref, style: { top } as React.CSSProperties };
}

/**
 * LCM-FR-UCJY: which nested submenu is open, with hover intent. With no
 * submenu open, hover opens one at once. Once one is open, hover on another
 * parent entry switches only after the pointer rests there, and leaving the
 * open parent closes it only after the pointer stays out. A click acts at once.
 */
export function useSubmenuIntent<K extends string>() {
  const [open, setOpen] = useState<K | null>(null);
  const openRef = useRef<K | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const cancel = useCallback(() => {
    if (timer.current) clearTimeout(timer.current);
    timer.current = null;
  }, []);
  const set = useCallback(
    (key: K | null) => {
      cancel();
      openRef.current = key;
      setOpen(key);
    },
    [cancel],
  );
  const later = useCallback(
    (key: K | null) => {
      cancel();
      timer.current = setTimeout(() => set(key), SUBMENU_INTENT_MS);
    },
    [cancel, set],
  );

  useEffect(() => cancel, [cancel]);

  const enter = useCallback(
    (key: K) => {
      if (openRef.current === null || openRef.current === key) set(key);
      else later(key);
    },
    [set, later],
  );
  // Leaving any parent entry starts the close of the open submenu. Entering
  // its own parent again, or its submenu, cancels that.
  const leave = useCallback(() => {
    if (openRef.current !== null) later(null);
    else cancel();
  }, [later, cancel]);

  return { open, enter, leave, click: set };
}
