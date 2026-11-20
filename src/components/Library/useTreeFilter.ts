/**
 * The Project panel's text filter, answered by the backend (LIB-FR-PPAT).
 *
 * The panel holds only the folders it has opened, so it cannot find a match
 * in a folder it has never loaded. The backend searches the whole project
 * index instead, after a short rest in typing, and returns the matches with
 * their ancestor folders.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import * as api from "../../api";
import { logWarn } from "../../logging";
import type { FilterResult, TreeLens } from "../../types";

/** LIB-FR-PPAT: the most matches one filter request returns. */
export const FILTER_LIMIT = 2000;

/** LIB-FR-PPAT: the rest in typing after which the request starts. */
export const FILTER_DEBOUNCE_MS = 200;

export interface TreeFilter {
  /** The result for the current text, or null with an empty text. */
  result: FilterResult | null;
  /** Run the current filter again, as a tree change asks (LIB-FR-10). */
  refresh: () => void;
}

export function useTreeFilter(text: string, lens: TreeLens): TreeFilter {
  const [result, setResult] = useState<FilterResult | null>(null);
  const [bump, setBump] = useState(0);
  const latest = useRef(0);
  const trimmed = text.trim();

  useEffect(() => {
    const token = ++latest.current;
    if (trimmed === "") {
      setResult(null);
      return;
    }
    const timer = setTimeout(() => {
      api
        .filterProjectTree(trimmed, lens, FILTER_LIMIT)
        .then((next) => {
          if (token === latest.current) setResult(next);
        })
        .catch((e: unknown) => {
          if (token !== latest.current) return;
          logWarn(["frontend"], "project panel: filter failed", { reason: String(e) });
          setResult({ root: [], truncated: false });
        });
    }, FILTER_DEBOUNCE_MS);
    return () => clearTimeout(timer);
  }, [trimmed, lens, bump]);

  const refresh = useCallback(() => setBump((n) => n + 1), []);
  return { result: trimmed === "" ? null : result, refresh };
}
