/**
 * The runs of one change on its stream, as the change workspace follows them
 * (`../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-DTJD,
 * OCW-FR-NNRY, OCW-FR-EKYF, OCW-FR-BNGB, OCW-FR-JHAB, OCW-FR-YTGJ,
 * OCW-FR-BTSI, OCW-FR-SFYL).
 *
 * It reads the runs of the project when the tab mounts and again on each
 * `"graduation queue changed"` for the stream and each `"graduation run
 * changed"` for a run of the stream, and it settles from them the run the
 * strip shows and the availability of Apply and Rework….
 */
import { useCallback, useEffect, useMemo, useState } from "react";

import * as api from "../../api";
import {
  onChangeActionRequestsChanged,
  onGraduationQueueChanged,
  onGraduationRunChanged,
} from "../../events";
import { logInfo, logWarn } from "../../logging";
import { changeRunPhase, runTitle } from "../../state/graduation";
import { useApplyProgress } from "../../state/openspec/applyProgress";
import {
  changeRunById,
  noteStartedRun,
  rereadChangeRunQueue,
  useChangeRunQueue,
} from "../../state/openspec/changeRunQueue";
import { changeViewOf, keepChangeView } from "../../state/openspec/changeView";
import { rereadChangeActionRequests } from "../../state/openspec/changeActionRequests";
import { useChangeProposals } from "../../state/openspec/changeProposals";
import { isTerminalRun, type GraduationRun } from "../../types";
import { startRefusal, type StartRefusal } from "./changeRunStart";

/** OCW-FR-JHAB: the newest apply run or rework run of the change on this stream. */
export function newestChangeRun(
  runs: readonly GraduationRun[],
  streamId: string,
  change: string,
): GraduationRun | null {
  let newest: GraduationRun | null = null;
  for (const run of runs) {
    if (run.streamId !== streamId || run.openspec?.change !== change) continue;
    if (changeRunPhase(run) === null) continue;
    // The later of two runs created at the same moment is the one listed later.
    if (newest === null || run.createdAt >= newest.createdAt) newest = run;
  }
  return newest;
}

/** OCW-FR-NNRY / OCW-FR-BNGB: a run of the change on this stream that is not terminal. */
export function activeChangeRun(
  runs: readonly GraduationRun[],
  streamId: string,
  change: string,
): GraduationRun | null {
  return (
    runs.find(
      (run) =>
        run.streamId === streamId && run.openspec?.change === change && !isTerminalRun(run.state),
    ) ?? null
  );
}

/** The title of a run the store holds, or null. */
export function knownRunTitle(runId: string): string | null {
  const run = changeRunById(runId);
  return run ? runTitle(run) : null;
}

export function useChangeRuns({
  tabId,
  streamId,
  change,
}: {
  tabId: string;
  /** The stream of the location, or null where the location is not a stream. */
  streamId: string | null;
  change: string;
}) {
  // The tab takes a fresh read of its own when it mounts (below), so the
  // store's first read is not asked for here.
  const runs = useChangeRunQueue(false);
  const strip = useMemo(
    () => (streamId === null ? null : newestChangeRun(runs, streamId, change)),
    [runs, streamId, change],
  );
  const active = useMemo(
    () => (streamId === null ? null : activeChangeRun(runs, streamId, change)),
    [runs, streamId, change],
  );
  const proposals = useChangeProposals(streamId, change);
  const pendingProposal = proposals.some((p) => p.state === "pending");
  // OCW-FR-NNRY: a closed proposal whose commit failed keeps Apply away too.
  const commitOwed = proposals.some((p) => p.state !== "pending" && p.commitOwed);
  const [closedRunId, setClosedRunId] = useState<string | null>(
    () => changeViewOf(tabId).closedRunId,
  );
  const [starting, setStarting] = useState(false);
  const [applyRefusal, setApplyRefusal] = useState<StartRefusal | null>(null);

  // OCW-FR-NNRY / OCW-FR-BTSI: the apply progress, read when the tab mounts,
  // when the strip shows another run, on each change of the run of the strip,
  // and on Refresh. A read that fails keeps the last answer. Each read starts
  // one OpenSpec CLI process, so a read of the change alone does not ask for
  // one.
  const { progress, reread } = useApplyProgress({
    streamId: streamId ?? "",
    change,
    runId: strip?.id ?? null,
    enabled: streamId !== null,
    keepOnFailure: true,
  });

  // OCW-FR-SFYL: a fresh read on mount, and one on each event for the stream.
  useEffect(() => {
    if (streamId === null) return;
    void rereadChangeRunQueue();
    let cancelled = false;
    const offs: Array<() => void> = [];
    const hold = (off: () => void) => {
      if (cancelled) off();
      else offs.push(off);
    };
    const refused = (e: unknown) =>
      logWarn(["frontend"], "a run subscription of the change workspace was refused", {
        streamId,
        reason: String(e).slice(0, 80),
      });
    onGraduationQueueChanged((payload) => {
      if (payload.streamId === streamId) void rereadChangeRunQueue();
    })
      .then(hold)
      .catch(refused);
    onGraduationRunChanged((payload) => {
      // A run this tab does not hold yet comes with a queue event.
      if (changeRunById(payload.runId)?.streamId === streamId) void rereadChangeRunQueue();
    })
      .then(hold)
      .catch(refused);
    // OCW-FR-GDZT: the requests of the change are read again on each event.
    onChangeActionRequestsChanged((payload) => {
      if (payload.streamId === streamId && payload.change === change) {
        void rereadChangeActionRequests(streamId, change);
      }
    })
      .then(hold)
      .catch(refused);
    return () => {
      cancelled = true;
      offs.forEach((off) => off());
    };
  }, [streamId, change]);

  /** OCW-FR-YTGJ: Close takes the strip away until a newer run starts. */
  const closeStrip = useCallback(() => {
    if (!strip) return;
    setClosedRunId(strip.id);
    keepChangeView(tabId, { closedRunId: strip.id });
    logInfo(["frontend"], "the run strip of a change was closed", { streamId, runId: strip.id });
  }, [strip, tabId, streamId]);

  /** OCW-FR-DTJD: Apply asks nothing before it. */
  const startApply = useCallback(async () => {
    if (streamId === null) return;
    setStarting(true);
    setApplyRefusal(null);
    logInfo(["frontend"], "an apply run is being started", { streamId });
    try {
      const run = await api.startChangeApply(streamId, change);
      noteStartedRun(run);
      logInfo(["frontend"], "an apply run was started", { streamId, runId: run.id });
      void rereadChangeRunQueue();
    } catch (e) {
      const refusal = startRefusal(e, knownRunTitle, "apply");
      setApplyRefusal(refusal);
      // OCW-FR-EKYF: the run that holds the change shows in the strip.
      if (refusal.runId) void rereadChangeRunQueue();
      logWarn(["frontend"], "an apply run was refused", { streamId, code: refusal.code });
    } finally {
      setStarting(false);
    }
  }, [streamId, change]);

  const activeTitle = active ? runTitle(active) : "";
  const runBusy = active ? `The run “${activeTitle}” of this change has not ended.` : null;
  const proposalBlock = pendingProposal
    ? "The change holds a pending edit proposal. Decide it first."
    : null;
  // OCW-FR-NNRY: why Apply is unavailable, or null.
  const applyBlock = starting
    ? "The apply run is starting."
    : runBusy ??
      proposalBlock ??
      (commitOwed
        ? "The accepted edits of an edit proposal are not committed yet."
        : null) ??
      (progress?.state === "blocked"
        ? `The change is missing artifacts: ${progress.missingArtifacts.join(", ") || "unknown"}.`
        : null);
  // OCW-FR-BNGB: why Rework… is unavailable, or null.
  const reworkBlock = runBusy ?? proposalBlock;

  return {
    strip: strip && strip.id !== closedRunId ? strip : null,
    closeStrip,
    progress,
    starting,
    startApply,
    applyBlock,
    applyRefusal,
    clearApplyRefusal: () => setApplyRefusal(null),
    reworkBlock,
    /** OCW-FR-NNRY: Refresh reads the apply progress again. */
    rereadProgress: reread,
  };
}

export type ChangeRuns = ReturnType<typeof useChangeRuns>;
