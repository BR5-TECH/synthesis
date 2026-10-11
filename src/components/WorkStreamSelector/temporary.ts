/**
 * The life of a temporary stream, seen from the selector
 * (`../../../specifications/ui/WSS-work-stream-selector.md` WSS-FR-PRVK,
 * WSS-FR-GBTD, WSS-FR-YHCT, WSS-FR-NXCH, WSS-FR-NZRB).
 *
 * The hook follows the project's runs and the stream listing. It decides when
 * the merge prompt is due for a temporary stream, and when the stream is
 * deleted after the merge run of its merge has completed. It performs no merge
 * itself. It deletes through the function the selector gives it, so a refusal
 * lands on the row like every other refusal.
 */

import { useCallback, useEffect, useRef, useState } from "react";

import * as api from "../../api";
import { onGraduationQueueChanged } from "../../events";
import { logError, logInfo } from "../../logging";
import { splitTyped, runsOfStream, streamWorkIsDone } from "../../state/graduation";
import type { GraduationRun, WorkStreamSummary } from "../../types";
import { useOtherOverlayOpen } from "./overlayIdle";

export interface TemporaryStreams {
  /** The stream to ask about now. Null while none is due or an overlay is open. */
  prompt: WorkStreamSummary | null;
  /** Take the prompt of a stream down. It is asked about again only on new work. */
  answerPrompt: (streamId: string) => void;
  /** WSS-FR-NZRB: follow the merge run a conflicted merge of a temporary stream made. */
  followMergeRun: (runId: string) => void;
  /**
   * WSS-FR-KFSA: delete a temporary stream after its merge. The stream is not
   * asked about again while the deletion is out or after it was refused, until
   * the stream has new work.
   */
  remove: (summary: WorkStreamSummary, force: boolean) => void;
  /** The project's runs as last read, or null before the first read. */
  runs: GraduationRun[] | null;
}

export function useTemporaryStreams(
  streams: WorkStreamSummary[] | null,
  deleteStream: (summary: WorkStreamSummary, force: boolean) => void,
): TemporaryStreams {
  const [runs, setRuns] = useState<GraduationRun[] | null>(null);
  const [pending, setPending] = useState<string[]>([]);
  /** Streams seen with work that is not done, since this surface started. */
  const seenWorking = useRef<Set<string>>(new Set());
  /** WSS-FR-NZRB: merge runs whose completion deletes their stream. */
  const followed = useRef<Set<string>>(new Set());
  /** Streams whose deletion was asked for. They get no prompt until new work. */
  const removing = useRef<Set<string>>(new Set());
  /** Bumped when a merge run is followed, so the listing is checked again. */
  const [followTick, setFollowTick] = useState(0);
  const deleteRef = useRef(deleteStream);
  deleteRef.current = deleteStream;

  const remove = useCallback((summary: WorkStreamSummary, force: boolean) => {
    removing.current.add(summary.stream.id);
    setPending((held) => held.filter((id) => id !== summary.stream.id));
    deleteRef.current(summary, force);
  }, []);
  const readSeq = useRef(0);

  const readRuns = useCallback(() => {
    const mine = ++readSeq.current;
    api
      .listGraduationQueue()
      .then((queue) => {
        if (mine === readSeq.current && queue && Array.isArray(queue.runs)) {
          setRuns(queue.runs);
        }
      })
      .catch((reason) => {
        logError(["frontend"], "the graduation runs could not be read for temporary streams", {
          code: splitTyped(String(reason))[0],
        });
      });
  }, []);

  const hasTemporary = (streams ?? []).some((s) => s.stream.temporary);

  // The runs are read only while the project holds a temporary stream.
  useEffect(() => {
    if (!hasTemporary) return;
    readRuns();
    let live = true;
    let unlisten: (() => void) | null = null;
    void onGraduationQueueChanged(() => readRuns())
      .then((fn) => {
        if (live) unlisten = fn;
        else fn();
      })
      .catch(() => {});
    return () => {
      live = false;
      unlisten?.();
    };
  }, [hasTemporary, readRuns]);

  // The listing changes with the runs, so it is read again with each of them.
  useEffect(() => {
    if (hasTemporary) readRuns();
  }, [streams, hasTemporary, readRuns]);

  useEffect(() => {
    if (!streams || !runs) return;
    const temporary = streams.filter((s) => s.stream.temporary && !s.stream.isMissing);
    const live = new Set(temporary.map((s) => s.stream.id));
    const due: string[] = [];
    const settled: string[] = [];
    for (const summary of temporary) {
      const { stream, mergeRun } = summary;
      if (mergeRun) {
        if (mergeRun.state === "completed") {
          if (followed.current.delete(mergeRun.runId)) {
            logInfo(["frontend"], "a temporary stream's merge run completed", {
              streamId: stream.id,
            });
            remove(summary, false);
          }
        } else if (mergeRun.state === "failed") {
          followed.current.delete(mergeRun.runId);
        } else {
          followed.current.add(mergeRun.runId);
        }
      }
      const working =
        summary.queuedRunCount > 0 ||
        Boolean(stream.busyRunId) ||
        runsOfStream(runs, stream).some((run) => run.state !== "completed");
      if (working) {
        seenWorking.current.add(stream.id);
        removing.current.delete(stream.id);
        settled.push(stream.id);
      } else if (
        seenWorking.current.has(stream.id) &&
        !removing.current.has(stream.id) &&
        streamWorkIsDone(runs, stream)
      ) {
        seenWorking.current.delete(stream.id);
        due.push(stream.id);
      }
    }
    if (due.length > 0) {
      logInfo(["frontend"], "the merge prompt is due for temporary streams", {
        count: due.length,
      });
    }
    setPending((held) => {
      const kept = held.filter((id) => live.has(id) && !settled.includes(id));
      const next = [...kept, ...due.filter((id) => !kept.includes(id))];
      return next.length === held.length && next.every((id, at) => id === held[at])
        ? held
        : next;
    });
  }, [streams, runs, followTick, remove]);

  const head = pending[0] ?? null;
  const blocked = useOtherOverlayOpen(head !== null);
  const prompt =
    head && !blocked
      ? ((streams ?? []).find((s) => s.stream.id === head) ?? null)
      : null;

  const answerPrompt = useCallback((streamId: string) => {
    setPending((held) => held.filter((id) => id !== streamId));
  }, []);
  const followMergeRun = useCallback((runId: string) => {
    followed.current.add(runId);
    setFollowTick((tick) => tick + 1);
  }, []);

  return { prompt, answerPrompt, followMergeRun, remove, runs };
}
