import { useEffect } from "react";
import { pruneTabScroll } from "../../state/tabScroll";
import type { Tab } from "../../types";

/**
 * EDT-FR-UERT / NAW-FR-IHDU / DFV-FR-YMJA: a scroll position belongs to the
 * tab, so it is dropped when the tab leaves the strip, by whichever route. A new
 * tab for the same content then starts at the top.
 */
export function useTabScrollPruning(tabs: Tab[]): void {
  useEffect(() => {
    pruneTabScroll(new Set(tabs.map((t) => t.id)));
  }, [tabs]);
}
