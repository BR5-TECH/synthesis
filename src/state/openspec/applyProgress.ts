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
 */
import { useCallback, useEffect, useRef, useState } from "react";

import { readApplyProgress } from "../../api";
import { onGraduationRunChanged } from "../../events";
import { logDebug, logWarn } from "../../logging";
import type { ApplyProgress } from "../../types";

export interface ApplyProgressFollow {
  /** The last answer, or null where none is held. */
  progress: ApplyProgress | null;
  /** Read again now. */
  reread: () => void;
}

export function useApplyProgress({
  streamId,
  change,
  runId,
  enabled,
  keepOnFailure,
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
}): ApplyProgressFollow {
  const [progress, setProgress] = useState<ApplyProgress | null>(null);
  const seq = useRef(0);
  const live = useRef(true);

  const read = useCallback(() => {
    if (!enabled) return;
    const mine = ++seq.current;
    readApplyProgress(streamId, change).then(
      (answer) => {
        if (!live.current || mine !== seq.current) return;
        setProgress(answer ?? null);
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
        logWarn(["frontend"], "the apply progress of a change could not be read", {
          streamId,
          code: String(error).slice(0, 80),
        });
      },
    );
  }, [enabled, streamId, change, keepOnFailure]);

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

  return { progress, reread: read };
}
