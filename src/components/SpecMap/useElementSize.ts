import { useEffect, useState } from "react";

export interface ElementSize {
  width: number;
  height: number;
  /** False until the element has been measured once. */
  measured: boolean;
}

/**
 * SMZ-FR-NUOB: the canvas measured on mount, on two animation frames after it,
 * and on every resize. The canvas has no height in its first frame, so a
 * framing computed from that one measurement would paint the map centred with
 * the first card above the fold.
 */
export function useElementSize(el: HTMLElement | null): ElementSize {
  const [size, setSize] = useState<ElementSize>({ width: 0, height: 0, measured: false });
  useEffect(() => {
    if (!el) {
      setSize((prev) => (prev.measured ? { width: 0, height: 0, measured: false } : prev));
      return;
    }
    const measure = () => {
      const rect = el.getBoundingClientRect();
      setSize((prev) =>
        prev.measured && prev.width === rect.width && prev.height === rect.height
          ? prev
          : { width: rect.width, height: rect.height, measured: true },
      );
    };
    measure();
    let second = 0;
    const first = requestAnimationFrame(() => {
      measure();
      second = requestAnimationFrame(measure);
    });
    const observer = typeof ResizeObserver === "function" ? new ResizeObserver(measure) : null;
    observer?.observe(el);
    window.addEventListener("resize", measure);
    return () => {
      cancelAnimationFrame(first);
      cancelAnimationFrame(second);
      observer?.disconnect();
      window.removeEventListener("resize", measure);
    };
  }, [el]);
  return size;
}

/** SMP-FR-GJEW / SMI-FR-QWOP: the responsive rules follow the window width. */
export function useWindowWidth(): number {
  const [width, setWidth] = useState(() => window.innerWidth);
  useEffect(() => {
    const onResize = () => setWidth(window.innerWidth);
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);
  return width;
}
