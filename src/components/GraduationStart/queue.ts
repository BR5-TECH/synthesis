/**
 * What the start dialog knows about the project's graduation runs
 * (`../../../specifications/ui/GSD-graduation-start-dialog.md` GSD-FR-HVDN).
 *
 * The hook reads `list_graduation_queue` when the dialog opens and again on
 * every "graduation queue changed" event. A read that fails leaves the runs
 * unread, and the commit message then has no default. The dialog shows no
 * refusal for it.
 */

import { useCallback, useEffect, useRef, useState } from "react";

import * as api from "../../api";
import { onGraduationQueueChanged } from "../../events";
import { logError, logWarn } from "../../logging";
import { splitTyped } from "../../state/graduation";
import type { GraduationRun } from "../../types";

/**
 * GSD-FR-HVDN: the project's runs in run order, or `null` while they are
 * unread. A late answer of an older read never replaces a newer one.
 */
export function useGraduationQueueRuns(draftId: string): GraduationRun[] | null {
  const [runs, setRuns] = useState<GraduationRun[] | null>(null);
  const sequence = useRef(0);
  const live = useRef(true);

  const readQueue = useCallback(async () => {
    const mine = ++sequence.current;
    try {
      const found = await api.listGraduationQueue();
      if (!live.current || mine !== sequence.current) return;
      if (!found || !Array.isArray(found.runs)) {
        logWarn(["frontend"], "the graduation queue read returned nothing", {
          draftId,
        });
        return;
      }
      setRuns(found.runs);
    } catch (reason) {
      if (!live.current || mine !== sequence.current) return;
      // The typed code alone: a refusal's detail can carry project paths.
      const [code] = splitTyped(String(reason));
      logError(["frontend"], "the graduation queue could not be read", {
        draftId,
        code,
      });
    }
  }, [draftId]);

  useEffect(() => {
    live.current = true;
    void readQueue();
    let unlisten: (() => void) | null = null;
    void onGraduationQueueChanged(() => void readQueue())
      .then((fn) => {
        if (live.current) unlisten = fn;
        else fn();
      })
      .catch(() => {
        logWarn(["frontend"], "the graduation queue events could not be followed", {
          draftId,
        });
      });
    return () => {
      live.current = false;
      unlisten?.();
    };
  }, [readQueue, draftId]);

  return runs;
}
