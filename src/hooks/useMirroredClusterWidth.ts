import { useEffect, useRef, useState, type CSSProperties } from "react";

/**
 * SNV-FR-HMCZ: the width of the wider top-chrome cluster, as the CSS custom
 * property `--top-chrome-side`. The stylesheet gives both side tracks this
 * width, so the search bar between them is on the window's midline. CSS grid
 * cannot give two tracks the width of the wider of two items without it.
 *
 * The clusters fit their content (`justify-self` in the stylesheet), so their
 * measured width does not depend on the width of their track and the
 * measurement cannot feed back on itself.
 */
export function useMirroredClusterWidth<
  L extends HTMLElement,
  R extends HTMLElement,
>() {
  const leftRef = useRef<L>(null);
  const rightRef = useRef<R>(null);
  const [side, setSide] = useState(0);

  useEffect(() => {
    const left = leftRef.current;
    const right = rightRef.current;
    if (!left || !right) return;
    const measure = () => {
      const widest = Math.ceil(
        Math.max(
          left.getBoundingClientRect().width,
          right.getBoundingClientRect().width,
        ),
      );
      setSide((prev) => (prev === widest ? prev : widest));
    };
    measure();
    const observer =
      typeof ResizeObserver === "function" ? new ResizeObserver(measure) : null;
    observer?.observe(left);
    observer?.observe(right);
    return () => observer?.disconnect();
  }, []);

  // Before the first measurement the stylesheet's default applies, and each
  // side track takes the width of its own cluster.
  const style =
    side > 0
      ? ({ "--top-chrome-side": `${side}px` } as CSSProperties)
      : undefined;
  return { leftRef, rightRef, style };
}
