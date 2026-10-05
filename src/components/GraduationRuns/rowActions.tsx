/**
 * The controls a rail row carries
 * (`../../../specifications/ui/GRU-graduation-runs.md` GRU-FR-XQVG).
 *
 * Every control here is offered only where the backend accepts the operation
 * behind it, so a control the author can reach is never one whose only outcome
 * is a refusal.
 *
 * All four stand in **one cluster** and in one treatment — the same button
 * form, the same size, and the same icon-with-accessible-name-and-tooltip
 * convention — so a row reads as a run and its controls rather than as a run
 * and three visitors. A control the run's state does not offer is absent from
 * the row rather than rendered dead, and every one of them states in words what
 * it does and, where it holds a value, what that value is (GRU-FR-NBRO).
 */

import {
  runTitle,
  canArchive,
  canContinue,
  canPause,
  canRestart,
  canSetAutoStart,
  isResume,
  RESTART_TOOLTIP,
  restartLabel,
} from "../../state/graduation";
import type { GraduationRun } from "../../types";
import { Icon } from "../icons";

/** The one class every control of the cluster carries. */
const CONTROL = "btn btn--icon btn--sm btn--ghost graduation__archive";

export interface RowActionsProps {
  run: GraduationRun;
  busy: boolean;
  onPause: () => void;
  onContinue: () => void;
  onAutoStart: (enabled: boolean) => void;
  onArchive: (archived: boolean) => void;
  /** GRT-FR-AMHS: open the restart confirmation for this run. */
  onRestart: () => void;
}

export function RowActions({
  run,
  busy,
  onPause,
  onContinue,
  onAutoStart,
  onArchive,
  onRestart,
}: RowActionsProps) {
  // Named for the run, because the rail draws one cluster per row and a column
  // of controls all called "Pause" says nothing about which run each acts on.
  const name = runTitle(run);
  return (
    <span className="graduation__cluster">
      {canPause(run) && (
        <button
          type="button"
          className={CONTROL}
          disabled={busy}
          onClick={onPause}
          aria-label={`Pause “${name}”`}
          title="Pause: stops the turn this run is doing now and gives its work stream back, so the next run in that queue starts. Its stage, its change set and its checkpoint are kept."
        >
          <Icon.Pause size={12} />
        </button>
      )}
      {canContinue(run) && (
        <button
          type="button"
          className={CONTROL}
          disabled={busy}
          onClick={onContinue}
          aria-label={`${isResume(run) ? "Resume" : "Continue"} “${name}”`}
          title="Returns this run to its stream's queue. It starts when that stream is free, and nothing running now is interrupted."
        >
          <Icon.Resume size={12} />
        </button>
      )}
      {canSetAutoStart(run) && (
        <button
          type="button"
          className={`${CONTROL} graduation__auto-start`}
          // The value in words rather than in the mark alone, so nothing about
          // it has to be read out of a fill or a colour.
          aria-label={
            run.autoStart
              ? `Auto-start is on for “${name}”. Turn it off`
              : `Auto-start is off for “${name}”. Turn it on`
          }
          title={
            run.autoStart
              ? "Auto-start on: this run's stream starts it when it reaches it. Activate to turn auto-start off."
              : "Auto-start off: this run stays in its place and its stream starts the runs behind it instead. Activate to turn auto-start on."
          }
          data-auto-start={run.autoStart ? "on" : "off"}
          disabled={busy}
          onClick={() => onAutoStart(!run.autoStart)}
        >
          {run.autoStart ? (
            <Icon.AutoStartOn size={12} />
          ) : (
            <Icon.AutoStartOff size={12} />
          )}
        </button>
      )}
      {/* GRT-FR-CTNO / GRT-FR-AMHS: the rail's way to the one restart act. It
          selects the run and opens the confirmation in the run region. */}
      {canRestart(run) && (
        <button
          type="button"
          className={CONTROL}
          disabled={busy}
          onClick={onRestart}
          aria-label={restartLabel(run)}
          title={RESTART_TOOLTIP}
        >
          <Icon.Restart size={12} />
        </button>
      )}
      {canArchive() && (
        <button
          type="button"
          className={CONTROL}
          disabled={busy}
          onClick={() => onArchive(!run.archived)}
          aria-label={
            run.archived ? `Restore “${name}”` : `Archive “${name}”`
          }
          // Archiving is a filing act and nothing else, and the control says so
          // where the question is asked: an author who files a run that is
          // working must never be left wondering whether they stopped it.
          title={
            run.archived
              ? "Restore: returns the run to the view its state puts it in."
              : "Archive: hides the run from this list. It does not stop it, change its state, release the stream it is holding, or delete it."
          }
        >
          {run.archived ? <Icon.Unarchive size={12} /> : <Icon.Archive size={12} />}
        </button>
      )}
    </span>
  );
}
