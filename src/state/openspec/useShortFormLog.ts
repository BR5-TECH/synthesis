/**
 * The log record of a short form that a surface renders
 * (`../../../specifications/ui/OFN-openspec-failure-notices.md` OFN-FR-QZEC,
 * OFN-FR-LKAT).
 *
 * A surface that renders the short form of a notice from its state calls
 * this hook with the failure on show. The hook emits one record when the
 * notice appears, and none for a render or a read again of the same failure.
 * A failure that goes and comes back is a notice that appears again.
 */
import { useEffect, useRef } from "react";

import type { OpenSpecFailure } from "../../types";
import { hasNotice, logFailureNotice, noticeKey } from "./failureNotice";

export function useShortFormLog(surface: string, failure: OpenSpecFailure | null): void {
  const key = failure && hasNotice(failure) ? noticeKey(failure) : null;
  const latest = useRef(failure);
  latest.current = failure;
  useEffect(() => {
    if (key === null || latest.current === null) return;
    logFailureNotice(surface, latest.current);
  }, [surface, key]);
}
