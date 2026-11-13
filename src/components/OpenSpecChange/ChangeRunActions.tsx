/**
 * Apply and Rework… in the header of a change workspace on a stream
 * (`../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-DTJD,
 * OCW-FR-NNRY, OCW-FR-EKYF, OCW-FR-LWFQ, OCW-FR-BNGB, OCW-FR-FEBP).
 *
 * An action that is unavailable keeps its focus stop and says why in a line
 * under the header, which its description names. A refused start is stated
 * beside the action that started it.
 */
import { useId, useRef, useState } from "react";

import { openGraduationRunRoute } from "../../state/graduation/runRoute";
import { OpenSpecFailureNotice } from "../OpenSpecFailureNotice";
import { NO_RUN_STARTED, type StartRefusal } from "./changeRunStart";
import type { ChangeRuns } from "./useChangeRuns";

/** OCW-FR-EKYF: the route a refusal offers: the run that holds the change. */
export function RefusalRoutes({ refusal }: { refusal: StartRefusal }) {
  if (!refusal.runId) return null;
  return (
    <button
      type="button"
      className="btn btn--sm btn--ghost"
      onClick={() => openGraduationRunRoute(refusal.runId!)}
    >
      Open run
    </button>
  );
}

/**
 * OCW-FR-EKYF: a refused start, as the surface states it. An OpenSpec
 * failure renders its notice and states that no run started. The tab is the
 * tab of the change, so the notice offers no Open change (OFN-FR-RLSO).
 * `onRetry` starts the same run again (OFN-FR-CBLG).
 */
export function RefusalStatement({
  refusal,
  surface,
  change,
  onRetry,
  className,
  testId,
  children,
}: {
  refusal: StartRefusal;
  surface: string;
  /** The change of the tab, where the failure names none. */
  change: string;
  onRetry: () => Promise<unknown> | void;
  className: string;
  testId: string;
  /** The other actions of the surface, such as Dismiss. */
  children?: React.ReactNode;
}) {
  if (refusal.failure) {
    return (
      <div className={className} data-testid={testId}>
        <OpenSpecFailureNotice
          failure={refusal.failure}
          surface={surface}
          change={change}
          onRetry={onRetry}
          frame={NO_RUN_STARTED}
          className="openspec-change__notice"
        />
        {children}
      </div>
    );
  }
  return (
    <div className={className} role="alert" data-testid={testId}>
      <span>{refusal.sentence}</span>
      <RefusalRoutes refusal={refusal} />
      {children}
    </div>
  );
}

/** The two header actions. */
export function ChangeRunButtons({
  runs,
  notes,
  onRework,
  reworkRef,
}: {
  runs: ChangeRuns;
  /** The ids of the two lines of `ChangeRunNotes`. */
  notes: { apply: string; rework: string };
  onRework: () => void;
  reworkRef: React.RefObject<HTMLButtonElement | null>;
}) {
  const applyNote = notes.apply;
  const reworkNote = notes.rework;
  return (
    <>
      <button
        type="button"
        className="btn btn--sm btn--default"
        aria-disabled={runs.applyBlock ? "true" : undefined}
        aria-describedby={runs.applyBlock ? applyNote : undefined}
        title={runs.applyBlock ?? undefined}
        onClick={() => {
          if (!runs.applyBlock) void runs.startApply();
        }}
      >
        Apply
      </button>
      <button
        ref={reworkRef}
        type="button"
        className="btn btn--sm btn--default"
        aria-disabled={runs.reworkBlock ? "true" : undefined}
        aria-describedby={runs.reworkBlock ? reworkNote : undefined}
        title={runs.reworkBlock ?? undefined}
        onClick={() => {
          if (!runs.reworkBlock) onRework();
        }}
      >
        Rework…
      </button>
    </>
  );
}

/**
 * OCW-FR-NNRY / OCW-FR-BNGB / OCW-FR-EKYF: why an action is unavailable, and
 * a refused Apply, on one line under the facts. The same reason for both
 * actions is said once.
 */
export function ChangeRunNotes({
  runs,
  notes,
  change,
}: {
  runs: ChangeRuns;
  notes: { apply: string; rework: string };
  /** The change of the tab, for the sentence of a refusal. */
  change: string;
}) {
  const applyNote = notes.apply;
  const reworkNote = notes.rework;
  const same = runs.applyBlock !== null && runs.applyBlock === runs.reworkBlock;
  if (!runs.applyBlock && !runs.reworkBlock && !runs.applyRefusal) return null;
  return (
    <div className="openspec-change__run-notes" data-testid="openspec-change-run-notes">
      {runs.applyBlock && (
        <p className="openspec-change__run-note t-ui-xs" id={applyNote} data-testid="openspec-apply-block">
          {same ? `Apply and Rework… are unavailable. ${runs.applyBlock}` : `Apply is unavailable. ${runs.applyBlock}`}
        </p>
      )}
      {runs.reworkBlock && (
        <p
          className={same ? "sr-only" : "openspec-change__run-note t-ui-xs"}
          id={reworkNote}
          data-testid="openspec-rework-block"
        >
          {`Rework… is unavailable. ${runs.reworkBlock}`}
        </p>
      )}
      {runs.applyRefusal && (
        <RefusalStatement
          refusal={runs.applyRefusal}
          surface="change_workspace_apply"
          change={change}
          onRetry={runs.startApply}
          className="openspec-change__run-refusal"
          testId="openspec-apply-refusal"
        >
          <button type="button" className="btn btn--sm btn--ghost" onClick={runs.clearApplyRefusal}>
            Dismiss
          </button>
        </RefusalStatement>
      )}
    </div>
  );
}

/** OCW-FR-LWFQ: the state of the Rework… dialog, its opener, and the ids of the notes. */
export function useReworkDialog() {
  const [open, setOpen] = useState(false);
  const opener = useRef<HTMLButtonElement | null>(null);
  const apply = useId();
  const rework = useId();
  return { open, setOpen, opener, notes: { apply, rework } };
}
