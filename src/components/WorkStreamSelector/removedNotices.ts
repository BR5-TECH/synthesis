/**
 * The notices of streams the backend removed
 * (`../../../specifications/ui/WSS-work-stream-selector.md` WSS-FR-RPYH,
 * WSS-FR-OSVE).
 *
 * The backend holds them, because the selector remounts on a change of content
 * root. They are read while the dropdown is open, and again after each listing,
 * which is what can have removed a stream.
 */
import { useCallback, useEffect, useRef, useState } from "react";

import * as api from "../../api";
import { logWarn } from "../../logging";
import type { RemovedStreamNotice, WorkStreamSummary } from "../../types";

export function useRemovedNotices(
  open: boolean,
  streams: WorkStreamSummary[] | null,
) {
  const [notices, setNotices] = useState<RemovedStreamNotice[]>([]);
  /** A read that was out at a dismissal does not bring the notice back. */
  const dismissed = useRef(new Set<string>());

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    void (async () => {
      try {
        const answer = await api.listRemovedWorkStreams();
        if (cancelled) return;
        const held = Array.isArray(answer) ? answer : [];
        setNotices(held.filter((n) => !dismissed.current.has(n.streamId)));
      } catch (reason) {
        logWarn(["frontend"], "removed stream notices could not be read", {
          reason: String(reason),
        });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [open, streams]);

  const dismiss = useCallback(async (streamId: string) => {
    dismissed.current.add(streamId);
    setNotices((held) => held.filter((n) => n.streamId !== streamId));
    try {
      await api.dismissRemovedWorkStream(streamId);
    } catch (reason) {
      logWarn(["frontend"], "removed stream notice could not be dismissed", {
        streamId,
        reason: String(reason),
      });
    }
  }, []);

  return { notices, dismiss };
}
