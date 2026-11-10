/**
 * The read of one CALM source for a surface outside the rich editor: the
 * diagram editor overlay (CAD-FR-KUYJ), the viewer (CAD-FR-LLNQ), and the
 * Diagram view of an Architecture tab (CAD-FR-OPTU). It uses the same read
 * and the same kept results as a fence, so every surface agrees.
 */
import { useEffect, useState } from "react";

import { logDebug } from "../../logging";
import { cachedCalm, readCalm, releaseCalm, showCalm } from "../../calm/read";
import type { CalmResult } from "../../calm/types";

export type RenderedCalm = Extract<CalmResult, { state: "rendered" }>;

export interface CalmRead {
  /** The source is empty or only whitespace. */
  empty: boolean;
  /** The last result that came back. While a read runs, it is an earlier one. */
  settled: CalmResult | null;
  /** The last diagram that a read of this surface gave. */
  lastRendered: RenderedCalm | null;
}

export function useCalmRead(source: string): CalmRead {
  const empty = source.trim() === "";
  const [settled, setSettled] = useState<CalmResult | null>(() =>
    empty ? null : (cachedCalm(source) ?? null),
  );
  const [lastRendered, setLastRendered] = useState<RenderedCalm | null>(() =>
    settled?.state === "rendered" ? settled : null,
  );

  const [owner] = useState(() => ({}));
  useEffect(() => {
    showCalm(owner, empty ? [] : [source]);
  }, [owner, source, empty]);
  useEffect(() => () => releaseCalm(owner), [owner]);

  useEffect(() => {
    if (empty) {
      setSettled(null);
      return;
    }
    const take = (result: CalmResult) => {
      setSettled(result);
      if (result.state === "rendered") setLastRendered(result);
    };
    const known = cachedCalm(source);
    if (known) {
      take(known);
      return;
    }
    let live = true;
    readCalm(source)
      .then((result) => {
        if (live) take(result);
      })
      .catch((error: unknown) => {
        logDebug(["frontend"], "a CALM read request failed", {
          error: error instanceof Error ? error.name : typeof error,
        });
      });
    return () => {
      live = false;
    };
  }, [source, empty]);

  return { empty, settled, lastRendered };
}
