/**
 * The body of the selected run's region
 * (`../../../specifications/ui/GRU-graduation-runs.md`).
 *
 * Everything the region says about one run, draft run or merge run: its title
 * and state, where it runs, the stage row, what blocks or stopped it, the
 * escalation it waits on, the paths it changed, the pass history, how it ended,
 * and the foot bar of actions. A merge run renders on the same terms as every
 * run, with its own title, provenance, publication, unresolved paths and
 * result added (GRU-FR-CXLB).
 */

import type { MutableRefObject } from "react";

import { logInfo } from "../../logging";

import {
  aheadOf,
  blockerStatement,
  blockerDetail,
  pushRefusalStatement,
  commitSummary,
  conditionOf,
  conditionSentence,
  failureStatement,
  hasReadableProgress,
  iterationLabel,
  retryStatement,
  outcomeStatement,
  provenanceLine,
  slotWaitSentence,
  waitsForSlot,
  targetHoldStatement,
  restartedAsStatement,
  restartedFromStatement,
  runTitle,
  stageHistory,
  stageOf,
  stageDescriptorsFor,
  stateLabelOf,
} from "../../state/graduation";
import { stagesWithLogAccess } from "../../state/graduation/logScopes";
import type {
  GraduationCapacity,
  GraduationQueue,
  GraduationRun,
} from "../../types";
import { RunProgress } from "../RunProgress";
import { RunActionRow, type PressedAction, type RunAct } from "./actionRow";
import { RunEscalationForm } from "./escalation";
import { RunColumns } from "./columns";
import { MergePaths, MergePublication, MergeResult } from "./merge";

/**
 * GRU-FR-CKOB: the states whose own paragraph already says what the run is
 * doing, so the condition line that stands in for a missing stage row does not
 * say it twice.
 */
const SAYS_ITSELF = ["blocked", "interrupted", "failed", "discarded"];

export interface RunRegionProps {
  run: GraduationRun;
  /** Every run the project holds, for what a run says about its restarts. */
  runs: GraduationRun[];
  queue: GraduationQueue | null;
  capacity: GraduationCapacity | null;
  busy: boolean;
  act: RunAct;
  onOpenDraft?: (draftId: string) => void;
  /** GRU-FR-QLRQ: opens a project settings section. */
  onOpenSettings?: (section: string) => void;
  /** GLW-FR-ALZI: the author activated a stage that holds a log. */
  onActivateStage: (stageId: string) => void;
  /** GEA-FR-VIPR: the backend accepted an answer set. */
  onAnswered: () => void;
  /** GEA-FR-TEMG: a refusal of an answer set. */
  onEscalationError: (message: string) => void;
  /** GRT-FR-XHLN: the run an accepted restart created. */
  onRestarted: (created: GraduationRun) => void;
  /** GRU-FR-KQPE: the refusal of this run's last row act. */
  refusal: string | null;
  pressedRef: MutableRefObject<PressedAction | null>;
  openRestart: boolean;
  onRestartOpened: () => void;
}

export function RunRegion({
  run,
  runs,
  queue,
  capacity,
  busy,
  act,
  onOpenDraft,
  onOpenSettings,
  onActivateStage,
  onAnswered,
  onEscalationError,
  onRestarted,
  refusal,
  pressedRef,
  openRestart,
  onRestartOpened,
}: RunRegionProps) {
  const title = runTitle(run);
  return (
    <>
      <div className="graduation__head">
        {/* GRU-FR-HDPQ: a merge run's title stands where a draft run's draft
            name does. */}
        <span className="graduation__name">{title}</span>
        <span className="spacer" />
        {/* GRU-FR-NBRO: the state in words and in accessible semantics rather
            than by colour, and announced when it changes. */}
        <span
          className="badge"
          role="status"
          aria-live="polite"
          data-testid="graduation-state"
        >
          {stateLabelOf(run)}
        </span>
      </div>
      {/* GRU-FR-UKNC: what this run is writing against. */}
      <p className="t-meta graduation__provenance" data-testid="graduation-provenance">
        {provenanceLine(run, Boolean(run.streamId))}
      </p>
      {/* GRU-FR-NWEC: the publication choice a merge run holds. */}
      <MergePublication run={run} />
      {/* GRU-FR-PFBY: a queued direct run whose pinned branch is not checked
          out says which branch it waits for. */}
      {targetHoldStatement(run) && (
        <p
          className="graduation__warning"
          role="status"
          data-testid="graduation-target-hold"
        >
          {targetHoldStatement(run)}
        </p>
      )}

      {/* GRU-FR-KMNF: a queued run that waits for a project slot says so, with
          the limit and the slots held. A run that waits behind a run of its own
          queue keeps the sentence for that queue instead. */}
      {capacity && waitsForSlot(run, capacity) && (
        <p className="t-meta" role="status" data-testid="graduation-run-slot-wait">
          {slotWaitSentence(capacity)}
        </p>
      )}

      {/* GRU-FR-IZKI / GRU-FR-CKOB: a record at a version this build does not
          recognise renders without a stage row rather than a guessed one.
          Everything else about the run still renders. */}
      {hasReadableProgress(run.observability) ? (
        <div className="graduation__stage-row">
          <RunProgress
            // GRU-FR-ZBMU / GRU-FR-TQJW / GRU-FR-YSTV: the ordered stages of
            // this kind of run, each carrying whether the run's log indexes
            // hold a segment for it.
            stages={stagesWithLogAccess(stageDescriptorsFor(run), run.logs)}
            onActivateStage={onActivateStage}
            currentStage={stageOf(run) ?? "queued"}
            condition={conditionOf(run)}
            conditionSentence={conditionSentence(
              run,
              queue ? aheadOf(queue, run.id) : null,
              waitsForSlot(run, capacity),
            )}
            iterationLabel={iterationLabel(run)}
            history={stageHistory(run)}
            // RPV-FR-10: the host renders the moves itself — the review's
            // revisions in the pass history beneath, and the author's retries
            // in the line under this row — so the component draws none and
            // they are read once.
            showLoops={false}
            outcome={outcomeStatement(run)}
            label={`Progress of ${title}`}
          />
          {/* GRU-FR-LBPR: an author repeating a Continue can otherwise see
              nothing that says they are repeating it. */}
          {retryStatement(run) && (
            <p className="t-meta" data-testid="graduation-retries">
              {retryStatement(run)}
            </p>
          )}
        </div>
      ) : (
        // GRU-FR-CKOB: a record this build does not recognise costs the run
        // its stage row and nothing else. What the run is doing is said in the
        // region's own words rather than lost with the row that used to carry
        // it. A state that states itself below says nothing twice.
        !SAYS_ITSELF.includes(run.state) && (
          <p className="graduation__provenance" data-testid="graduation-condition">
            {conditionSentence(
              run,
              queue ? aheadOf(queue, run.id) : null,
              waitsForSlot(run, capacity),
            )}
          </p>
        )
      )}

      {/* GRU-FR-LBPR / GRU-FR-FZCN / GRU-FR-DBUS: a run resting on a blocker
          states it in either state that carries one. */}
      {blockerStatement(run) && (
        <p
          className="graduation__blocker graduation__warning"
          data-testid="graduation-blocker"
        >
          {blockerStatement(run)}
          {blockerDetail(run) && (
            <span
              className="graduation__blocker-detail t-meta"
              data-testid="graduation-blocker-detail"
            >
              {blockerDetail(run)}
            </span>
          )}
        </p>
      )}

      {/* GRU-FR-BHJO: a headline that names why it stopped, and then what the
          backend said about it. GRU-FR-QLRQ: a run stopped at its time limit
          routes to the setting that sets the limit, so the author can raise it
          before Continue. */}
      {run.state === "interrupted" && (
        <p className="graduation__warning" data-testid="graduation-interruption">
          {conditionSentence(run)}
          {run.interruption?.detail ? (
            <span className="t-meta"> {run.interruption.detail}</span>
          ) : null}
          {run.interruption?.reason === "execution_timeout" && onOpenSettings ? (
            <>
              {" "}
              <button
                type="button"
                className="btn btn--sm btn--ghost graduation__warning-action"
                data-testid="graduation-change-time-limit"
                onClick={() => {
                  logInfo(["frontend"], "graduation run routed to the time limit setting", {
                    runId: run.id,
                  });
                  onOpenSettings("graduation");
                }}
              >
                Change the time limit
              </button>
            </>
          ) : null}
        </p>
      )}

      {/* GRU-FR-TFEZ: a push the remote did not take, said once. */}
      {pushRefusalStatement(run) && (
        <p className="graduation__warning" data-testid="graduation-push-refused">
          {pushRefusalStatement(run)}
        </p>
      )}

      {/* GRU-FR-AJGM: the paths a merge changes, with the ones Git could not
          settle named. A draft run renders none. */}
      <MergePaths run={run} />

      {/* GRU-FR-FZCN: an escalation, and the answers it is waiting for. The
          questions and the fields are one surface: questions the author cannot
          answer are a run nothing can release. */}
      {run.escalation && (
        <RunEscalationForm
          run={run}
          busy={busy}
          onAnswered={onAnswered}
          // GRU-FR-KQPE: one refusal alert at most in the region.
          onError={onEscalationError}
        />
      )}

      {/* GRU-FR-MCYF: what the run changed and how it got there, side by side
          where the region is wide enough and stacked where not. */}
      <RunColumns run={run} />

      {/* GRU-FR-QYEE / GRU-FR-QMWX: a failed run states the typed failure. It
          is read from the run rather than from the stage row, so a record this
          build cannot render never takes it away. */}
      {run.state === "failed" && (
        <p className="graduation__warning" data-testid="graduation-failure">
          {failureStatement(run)}
        </p>
      )}

      {run.state === "completed" && commitSummary(run) && (
        <p className="graduation__outcome" data-testid="graduation-commits">
          {commitSummary(run)}
        </p>
      )}

      {/* GRU-FR-JRMA: what a merge run landed. */}
      <MergeResult run={run} />

      {restartedFromStatement(run) && (
        <p className="t-meta" data-testid="graduation-restarted-from">
          {restartedFromStatement(run)}
        </p>
      )}
      {restartedAsStatement(run, runs) && (
        <p className="t-meta" data-testid="graduation-restarted-as">
          {restartedAsStatement(run, runs)}
        </p>
      )}

      {/* GRU-FR-OZAR: the action row is a foot bar pinned to the bottom edge of
          the region, and the refusal it issues stands in it (GRU-FR-KQPE), so
          neither needs a scroll to reach. */}
      <div className="graduation__foot">
        <RunActionRow
          run={run}
          busy={busy}
          act={act}
          onOpenDraft={onOpenDraft}
          onRestarted={onRestarted}
          refusal={refusal}
          pressedRef={pressedRef}
          openRestart={openRestart}
          onRestartOpened={onRestartOpened}
        />
      </div>
    </>
  );
}
