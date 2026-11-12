/**
 * The apply progress of one change, as a surface reads it
 * (`../../../specifications/core/OSM-openspec-mode.md` OSM-FR-PEAK;
 * `../../../specifications/ui/GRU-graduation-runs.md` GRU-FR-PXCW;
 * `../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-NNRY,
 * OCW-FR-BTSI).
 *
 * A hook rather than a store: the Runs panel reads it for the selected apply
 * run, and the change workspace for its change, each on its own terms. It
 * reads when it mounts and on each `"graduation run changed"` for the run it
 * follows. An older answer that lands after a newer read is dropped.
 *
 * A read that fails keeps its failure, so the surface adds the short form of
 * its notice to its stage sentence
 * (`../../../specifications/ui/OFN-openspec-failure-notices.md` OFN-FR-QZEC).
 */
import { useCallback, useEffect, useRef, useState } from "react";

import { readApplyProgress } from "../../api";
import { onGraduationRunChanged } from "../../events";
import { logDebug, logWarn } from "../../logging";
import type { ApplyProgress, OpenSpecFailure } from "../../types";
import { logFailureNotice, shortForm, toOpenSpecFailure } from "./failureNotice";

export interface ApplyProgressFollow {
  /** The last answer, or null where none is held. */
  progress: ApplyProgress | null;
  /** OFN-FR-QZEC: the short form of the failure of the last read, or null. */
  failure: string | null;
  /** Read again now. */
  reread: () => void;
}

export function useApplyProgress({
  streamId,
  change,
  runId,
  enabled,
  keepOnFailure,
  surface,
}: {
  streamId: string;
  change: string;
  /** The run whose `"graduation run changed"` makes it read again, if any. */
  runId: string | null;
  /** Nothing is read while this is false. */
  enabled: boolean;
  /**
   * OCW-FR-BTSI: a read that fails keeps the last answer. GRU-FR-PXCW: where
   * this is false, a read that fails drops it, so the stage takes its
   * ordinary sentence.
   */
  keepOnFailure: boolean;
  /** OFN-FR-LKAT: the surface of the short form, for its log record. */
  surface: string;
}): ApplyProgressFollow {
  const [progress, setProgress] = useState<ApplyProgress | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  /** The code of the failure on show, so a repeated failure logs once. */
  const shownCode = useRef<string | null>(null);
  const seq = useRef(0);
  const live = useRef(true);

  const read = useCallback(() => {
    if (!enabled) return;
    const mine = ++seq.current;
    readApplyProgress(streamId, change).then(
      (answer) => {
        if (!live.current || mine !== seq.current) return;
        setProgress(answer ?? null);
        setFailure(null);
        shownCode.current = null;
        logDebug(["frontend"], "the apply progress of a change was read", {
          streamId,
          state: answer?.state ?? "",
          total: answer?.total ?? 0,
          complete: answer?.complete ?? 0,
        });
      },
      (error: unknown) => {
        if (!live.current || mine !== seq.current) return;
        if (!keepOnFailure) setProgress(null);
        const read: OpenSpecFailure = toOpenSpecFailure(error);
        const words = shortForm(read, change);
        setFailure(words);
        const key = `${read.code}/${read.cause ?? ""}`;
        // OFN-FR-LKAT: the short form appears once for one failure.
        if (words !== null && shownCode.current !== key) logFailureNotice(surface, read);
        else if (words === null) {
          logWarn(["frontend"], "the apply progress of a change could not be read", {
            streamId,
            code: read.code.slice(0, 80),
          });
        }
        shownCode.current = key;
      },
    );
  }, [enabled, streamId, change, keepOnFailure, surface]);

  useEffect(() => {
    live.current = true;
    return () => {
      live.current = false;
    };
  }, []);

  // A read when it starts to follow, and when what it follows changes.
  useEffect(() => {
    if (!enabled) {
      setProgress(null);
      setFailure(null);
      shownCode.current = null;
      return;
    }
    read();
  }, [enabled, read, runId]);

  // GRU-FR-PXCW / OCW-FR-BTSI: each change of the run it follows.
  useEffect(() => {
    if (!enabled || runId === null) return;
    let cancelled = false;
    let off: (() => void) | undefined;
    onGraduationRunChanged((payload) => {
      if (payload.runId === runId) read();
    })
      .then((unlisten) => {
        if (cancelled) unlisten();
        else off = unlisten;
      })
      .catch((e: unknown) =>
        logWarn(["frontend"], "the run subscription of the apply progress was refused", {
          reason: String(e).slice(0, 80),
        }),
      );
    return () => {
      cancelled = true;
      off?.();
    };
  }, [enabled, runId, read]);

  return { progress, failure, reread: read };
}
