/**
 * The selected run's action row
 * (`../../../specifications/ui/GRU-graduation-runs.md` GRU-FR-XQVG,
 * GRU-FR-DVWY, GRU-FR-WDWB, GRU-FR-KQPE, GRU-FR-ZMHB, GRU-FR-PVXD,
 * GRU-FR-CLFA).
 *
 * The row's controls, the refusal of an act one of them issued, and where
 * keyboard focus goes when that act settles stand here together, beside the
 * controls the rules are about.
 */

import { useEffect, useRef, useState, type MutableRefObject } from "react";

import * as api from "../../api";
import {
  actionsFor,
  canRestart,
  isResume,
  isTerminalRun,
  removalStatement,
  runsRemovedByDiscard,
  runTitle,
} from "../../state/graduation";
import type { RunAction } from "../../state/graduation/runState";
import type { GraduationQueue, GraduationRun } from "../../types";
import { RestartRegionControl } from "./restart";

/** The action-row control an act was issued from, and the run it acted on. */
export interface PressedAction {
  runId: string;
  action: RunAction;
}

/**
 * One act on a run. `pressed` names the row control that issued it, and
 * `onRefused` hands a refusal to a control that renders it itself. The answer
 * says whether the act was accepted.
 */
export type RunAct = (
  what: string,
  action: () => Promise<unknown>,
  pressed?: PressedAction,
  onRefused?: (message: string) => void,
) => Promise<boolean>;

/** GRU-FR-PVXD: the control that takes another's place in the row. */
const COUNTERPART: Partial<Record<RunAction, RunAction>> = {
  pause: "continue",
  continue: "pause",
};

export interface RunActionRowProps {
  run: GraduationRun;
  /** GRU-FR-CLFA: the runs the project holds, for the runs a discard removes. */
  queue?: GraduationQueue | null;
  busy: boolean;
  act: RunAct;
  onOpenDraft?: (draftId: string) => void;
  /** GRT-FR-XHLN: the run an accepted restart created. */
  onRestarted: (created: GraduationRun) => void;
  /** The refusal of this run's last row act, where it was refused. */
  refusal: string | null;
  /** The row control the act in flight was issued from. */
  pressedRef: MutableRefObject<PressedAction | null>;
  /** GRT-FR-AMHS: the rail's Restart asks this run's confirmation to open. */
  openRestart: boolean;
  onRestartOpened: () => void;
}

export function RunActionRow({
  run,
  queue = null,
  busy,
  act,
  onOpenDraft,
  onRestarted,
  refusal,
  pressedRef,
  openRestart,
  onRestartOpened,
}: RunActionRowProps) {
  const rowRef = useRef<HTMLDivElement>(null);
  const refusalRef = useRef<HTMLParagraphElement>(null);
  // GRU-FR-CLFA: the act that waits for the author's confirmation, where it
  // would remove runs queued behind this one.
  const [confirming, setConfirming] = useState<"discard" | "revert" | null>(null);
  const removed = runsRemovedByDiscard(queue, run);
  // A revert of a run that has ended ends nothing, so it removes nothing.
  const removedByRevert = isTerminalRun(run.state) ? [] : removed;
  useEffect(() => {
    setConfirming(null);
  }, [run.id, run.state]);

  // GRU-FR-ZMHB / GRU-FR-PVXD: the row's controls were disabled while the act
  // ran, which drops focus to the page body. Once it settles, focus comes back
  // to this run's row — but only where it was lost, never from where the author
  // moved it, and never into a run selected meanwhile.
  useEffect(() => {
    if (busy) return;
    const pressed = pressedRef.current;
    // A press for another run is left for the row of that run: an accepted
    // restart names the run it created before that run's row stands
    // (GRT-FR-XHLN). The section drops a press no selected run owns.
    if (!pressed || pressed.runId !== run.id) return;
    pressedRef.current = null;
    const row = rowRef.current;
    if (!row) return;
    const active = document.activeElement;
    if (active && active !== document.body && !row.contains(active)) return;
    const offered = (action: RunAction | undefined) =>
      action
        ? row.querySelector<HTMLElement>(`[data-action="${action}"]:not(:disabled)`)
        : null;
    const target =
      offered(pressed.action) ??
      offered(COUNTERPART[pressed.action]) ??
      row.querySelector<HTMLElement>("button:not(:disabled)");
    target?.focus();
  }, [busy, run.id, pressedRef]);

  // GRU-FR-KQPE: a refusal the author cannot see is no refusal, so the run
  // region brings it into view beside the row that issued it.
  useEffect(() => {
    if (refusal) refusalRef.current?.scrollIntoView?.({ block: "nearest" });
  }, [refusal]);

  const from = (action: RunAction): PressedAction => ({ runId: run.id, action });

  return (
    <>
      <div className="graduation__actions" ref={rowRef}>
        {actionsFor(run).map((action) => {
          switch (action) {
            case "open-draft":
              // GRU-FR-ZMHB: it only navigates, so it stays enabled.
              return (
                <button
                  key={action}
                  type="button"
                  className="btn btn--sm btn--default"
                  data-action={action}
                  onClick={() => onOpenDraft?.(run.input.draftId)}
                >
                  Open draft
                </button>
              );
            case "pause":
              return (
                <button
                  key={action}
                  type="button"
                  className="btn btn--sm btn--default"
                  data-action={action}
                  disabled={busy}
                  onClick={() =>
                    act("Paused the run.", () => api.pauseGraduationRun(run.id), from(action))
                  }
                >
                  Pause
                </button>
              );
            case "continue":
              return (
                <button
                  key={action}
                  type="button"
                  className="btn btn--sm btn--primary"
                  data-action={action}
                  disabled={busy}
                  onClick={() =>
                    act(
                      "Continued the run.",
                      () => api.continueGraduationRun(run.id),
                      from(action),
                    )
                  }
                >
                  {isResume(run) ? "Resume" : "Continue"}
                </button>
              );
            case "revert":
              return (
                <button
                  key={action}
                  type="button"
                  className="btn btn--sm btn--default"
                  data-action={action}
                  disabled={busy}
                  onClick={() =>
                    removedByRevert.length > 0
                      ? setConfirming("revert")
                      : act(
                          "Reverted the run's commits.",
                          () => api.revertGraduationRun(run.id),
                          from(action),
                        )
                  }
                >
                  Revert
                </button>
              );
            case "discard":
              return (
                <button
                  key={action}
                  type="button"
                  className="btn btn--sm btn--danger"
                  data-action={action}
                  disabled={busy}
                  onClick={() =>
                    removed.length > 0
                      ? setConfirming("discard")
                      : act(
                          "Discarded the run.",
                          () => api.discardGraduationRun(run.id),
                          from(action),
                        )
                  }
                >
                  Discard run
                </button>
              );
            case "answer":
              // The form above is where an escalation is answered, so the
              // action row carries no second route to it.
              return null;
            case "restart":
              return canRestart(run) ? (
                <RestartRegionControl
                  // Keyed by the run as well as by the act: a control
                  // holding an open confirmation must not be handed to
                  // the next run the author selects.
                  key={`${action}-${run.id}`}
                  run={run}
                  busy={busy}
                  act={act}
                  onRestarted={onRestarted}
                  openRequested={openRestart}
                  onOpenRequestTaken={onRestartOpened}
                />
              ) : null;
            default:
              return null;
          }
        })}
      </div>
      {/* GRU-FR-CLFA: the runs the act removes are named before it is invoked,
          and their drafts are said to return to the active drafts. */}
      {confirming && (
        <div
          className="graduation-restart"
          role="group"
          aria-label={confirming === "discard" ? "Confirm discard" : "Confirm revert"}
          data-testid="graduation-removal-confirm"
        >
          <p className="graduation-restart__ask">
            {confirming === "discard"
              ? `Discard “${runTitle(run)}”? `
              : `Revert “${runTitle(run)}” and end it? `}
            {removalStatement(confirming === "discard" ? removed : removedByRevert)}
          </p>
          <div className="graduation-restart__actions">
            <button
              type="button"
              className="btn btn--sm btn--ghost"
              onClick={() => setConfirming(null)}
              disabled={busy}
            >
              Cancel
            </button>
            <button
              type="button"
              className="btn btn--sm btn--danger"
              data-action={confirming}
              disabled={busy}
              onClick={async () => {
                await (confirming === "discard"
                  ? act(
                      "Discarded the run.",
                      () => api.discardGraduationRun(run.id),
                      from("discard"),
                    )
                  : act(
                      "Reverted the run's commits.",
                      () => api.revertGraduationRun(run.id),
                      from("revert"),
                    ));
                setConfirming(null);
              }}
            >
              {confirming === "discard" ? "Discard run" : "Revert run"}
            </button>
          </div>
        </div>
      )}
      {/* GRU-FR-KQPE: beside the row that issued it, where the author is
          looking, rather than at the top of a region they scrolled down. */}
      {refusal && (
        <p
          className="graduation__error graduation__warning"
          role="alert"
          data-testid="graduation-action-refusal"
          ref={refusalRef}
        >
          {refusal}
        </p>
      )}
    </>
  );
}
