/**
 * Restarting a discarded run
 * (`../../../specifications/ui/GRT-graduation-restart.md`).
 *
 * It confirms before it invokes anything, and the confirmation is where the
 * author chooses the work stream the new run belongs to (GRT-FR-VWHM) and —
 * where that stream is occupied, so the new run must wait — what it does with
 * work standing there and under what message (GRT-FR-SQNB).
 */

import { useEffect, useRef, useState } from "react";

import * as api from "../../api";
import { logError } from "../../logging";
import {
  DEFAULT_STANDING_WORK,
  defaultStreamFor,
  graduationErrorMessage,
  isOccupied,
  RESTART_TOOLTIP,
  restartConfirmation,
  restartDirectStatement,
  restartLabel,
  restartRefusal,
  standingWorkCommits,
} from "../../state/graduation";
import type {
  GraduationRun,
  StandingWork,
  WorkStreamSummary,
} from "../../types";
import { StandingWorkChoice } from "../StandingWorkChoice";
import type { RunAct } from "./actionRow";

export interface RestartRegionControlProps {
  run: GraduationRun;
  busy: boolean;
  onRestarted: (run: GraduationRun) => void;
  /**
   * GRU-FR-ZMHB: the section's own act, where the control stands in its action
   * row. The restart then disables the row and returns focus as every other
   * act of the row does. Without one the control calls the backend itself.
   */
  act?: RunAct;
  /**
   * GRT-FR-AMHS: the rail's **Restart** asks for this confirmation to open,
   * because the two controls are one act with one confirmation.
   */
  openRequested?: boolean;
  /** The request above is taken, so it opens the confirmation once. */
  onOpenRequestTaken?: () => void;
}

export function RestartRegionControl({
  run,
  busy,
  onRestarted,
  act,
  openRequested = false,
  onOpenRequestTaken,
}: RestartRegionControlProps) {
  const [open, setOpen] = useState(false);
  const [streams, setStreams] = useState<WorkStreamSummary[]>([]);
  const [streamId, setStreamId] = useState<string>("");
  // GRT-FR-SQNB: the confirmation rests at the same position for every
  // restart, so a choice is taken rather than carried over from a run the
  // author discarded.
  const [standingWork, setStandingWork] =
    useState<StandingWork>(DEFAULT_STANDING_WORK);
  const [message, setMessage] = useState("");
  const [error, setError] = useState<string | null>(null);
  const streamRef = useRef<HTMLSelectElement>(null);
  const actionsRef = useRef<HTMLDivElement>(null);
  const confirmRef = useRef<HTMLButtonElement>(null);

  // GRT-FR-NPDC: a discarded direct run restarts where it was pinned, so it
  // asks for no stream and no standing-work choice.
  const direct = Boolean(run.directTarget);
  const occupied =
    !direct && isOccupied(streams.find((s) => s.stream.id === streamId));
  const commits = occupied && standingWorkCommits(standingWork);

  // Held on the run's id rather than on the run: every read of the queue
  // hands this control a new object for the same run, and an effect that
  // watched the object would re-read the streams — and put the author's own
  // choice back to the default — on every event a working run emits.
  const runId = run.id;
  useEffect(() => {
    if (!open || direct) return;
    let live = true;
    void api
      .listWorkStreams()
      .then((found) => {
        if (!live) return;
        // An answer that is not a list is one this control renders nothing
        // from, rather than one that takes the whole panel down with it.
        setStreams(Array.isArray(found) ? found : []);
        // GRT-FR-IMRI: the stream the discarded run ran on, where it still
        // exists. A stream the author has already chosen stands: this seeds a
        // choice rather than making one.
        setStreamId(
          (held) =>
            held ||
            defaultStreamFor(
              run,
              found.map((s) => s.stream.id),
            ) ||
            "",
        );
      })
      .catch((reason) => {
        if (!live) return;
        logError(["frontend"], "the work streams could not be read", {});
        setError(graduationErrorMessage(String(reason)));
      });
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, runId]);

  // GRT-FR-VWHM: the confirmation is taller than the run region is at its
  // resting height, and it opens at the foot of that region. What is brought
  // into view is the row that ends it, because a confirmation whose act the
  // author cannot see is one that appears to have done nothing. It runs again
  // whenever the form grows, which the standing-work question makes it do.
  //
  // The keyboard goes to the first question rather than to what was scrolled
  // to, and takes no scroll of its own with it.
  useEffect(() => {
    if (!open) return;
    actionsRef.current?.scrollIntoView?.({ block: "nearest" });
    (streamRef.current ?? confirmRef.current)?.focus({ preventScroll: true });
  }, [open, occupied, commits]);

  // GRU-FR-KQPE: another act of the row starting makes this refusal stale, and
  // the region shows one refusal alert at most. A restart's own refusal lands
  // while the row is still busy, so it is not dropped by this.
  useEffect(() => {
    if (busy) setError(null);
  }, [busy]);

  const begin = () => {
    // A refusal belongs to the attempt that was refused. Opening the
    // confirmation again is a new attempt.
    setError(null);
    setStandingWork(DEFAULT_STANDING_WORK);
    setMessage("");
    setOpen(true);
  };

  // GRT-FR-AMHS: the rail's Restart opens this confirmation, and the request is
  // taken at once so a later mount of this control does not open it again. A
  // confirmation that is already open is the same attempt, so what the author
  // chose and the refusal they are reading stay, and only focus comes back.
  useEffect(() => {
    if (!openRequested) return;
    onOpenRequestTaken?.();
    if (open) {
      actionsRef.current?.scrollIntoView?.({ block: "nearest" });
      (streamRef.current ?? confirmRef.current)?.focus({ preventScroll: true });
      return;
    }
    begin();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [openRequested]);

  const restart = async () => {
    if (!direct && !streamId) return;
    const call = () =>
      api.restartGraduationRun(
        run.id,
        // GRT-FR-NPDC: the backend restarts a direct run on its pinned
        // worktree whatever stream is named, so the run's own is sent.
        direct ? run.streamId : streamId,
        occupied ? standingWork : DEFAULT_STANDING_WORK,
        // GSD-FR-MZTB: a choice that makes no commit carries no message.
        occupied && standingWorkCommits(standingWork)
          ? message.trim() || null
          : null,
      );
    if (act) {
      // GRT-FR-KSBC: the refusal still renders in this confirmation, against
      // the discarded run, and it replaces any refusal the row showed.
      setError(null);
      const held: { created?: GraduationRun } = {};
      const accepted = await act(
        "Restarted the run.",
        async () => {
          held.created = await call();
          // GRU-FR-ZMHB: closed inside the act, so the row's Restart stands
          // again before the act settles and focus returns to it.
          setOpen(false);
        },
        { runId: run.id, action: "restart" },
        (refusal) => setError(restartRefusal(refusal)),
      );
      if (accepted && held.created) onRestarted(held.created);
      return;
    }
    try {
      const created = await call();
      setOpen(false);
      onRestarted(created);
    } catch (reason) {
      // GRT-FR-KSBC: rendered against the discarded run, which is unchanged.
      logError(["frontend"], "the run could not be restarted", {});
      setError(restartRefusal(graduationErrorMessage(String(reason))));
    }
  };

  if (!open) {
    return (
      <button
        type="button"
        className="btn btn--sm btn--default graduation__restart"
        data-action="restart"
        disabled={busy}
        onClick={begin}
        aria-label={restartLabel(run)}
        title={RESTART_TOOLTIP}
        data-testid="graduation-restart"
      >
        Restart
      </button>
    );
  }

  return (
    <div className="graduation-restart" data-testid="graduation-restart-confirm">
      <p className="graduation-restart__ask">{restartConfirmation(run)}</p>
      {direct && (
        <p className="t-ui-sm" data-testid="graduation-restart-direct">
          {restartDirectStatement(run)}
        </p>
      )}
      {!direct && (
      <label className="graduation-restart__stream">
        <span className="t-eyebrow">Work stream</span>
        <select
          className="select select--sm"
          ref={streamRef}
          value={streamId}
          onChange={(event) => setStreamId(event.target.value)}
          aria-label="Work stream"
        >
          {/* GRT-FR-IMRI: with no live stream to default to, the control says
              so. A select whose value names no option shows the first one
              instead, which would read as a stream that is chosen standing
              beside an act that refuses. */}
          {streamId === "" && <option value="">Choose a work stream</option>}
          {streams.map(({ stream }) => (
            <option key={stream.id} value={stream.id}>
              {stream.name}
            </option>
          ))}
        </select>
      </label>
      )}
      {/* GRT-FR-SQNB / GSD-FR-WQPD: asked only where the new run will wait. */}
      {occupied && (
        <StandingWorkChoice
          name={`restart-${run.id}`}
          value={standingWork}
          onChange={setStandingWork}
          message={message}
          onMessage={setMessage}
          messageDefault={run.input.draftName}
        />
      )}
      {error && (
        <p className="graduation__warning" role="alert">
          {error}
        </p>
      )}
      <div className="graduation-restart__actions" ref={actionsRef}>
        <button
          type="button"
          className="btn btn--sm btn--ghost"
          onClick={() => setOpen(false)}
          disabled={busy}
        >
          Cancel
        </button>
        <button
          type="button"
          className="btn btn--sm btn--primary"
          // GRU-FR-PVXD: the control focus returns to while the confirmation
          // stays open, as it does after a refusal.
          data-action="restart"
          onClick={restart}
          ref={confirmRef}
          disabled={(!direct && !streamId) || busy}
        >
          Restart run
        </button>
      </div>
    </div>
  );
}
