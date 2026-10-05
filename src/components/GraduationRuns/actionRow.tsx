/**
 * The selected run's action row
 * (`../../../specifications/ui/GRU-graduation-runs.md` GRU-FR-XQVG,
 * GRU-FR-DVWY, GRU-FR-WDWB, GRU-FR-KQPE, GRU-FR-ZMHB, GRU-FR-PVXD).
 *
 * The row's controls, the refusal of an act one of them issued, and where
 * keyboard focus goes when that act settles stand here together, beside the
 * controls the rules are about.
 */

import { useEffect, useRef, type MutableRefObject } from "react";

import * as api from "../../api";
import { actionsFor, canRestart, isResume } from "../../state/graduation";
import type { RunAction } from "../../state/graduation/runState";
import type { GraduationRun } from "../../types";
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
                    act(
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
                    act(
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
