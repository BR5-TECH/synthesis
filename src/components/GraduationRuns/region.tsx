/**
 * The body of the selected run's region
 * (`../../../specifications/ui/GRU-graduation-runs.md`).
 *
 * Everything the region says about one run, draft run or merge run: its title
 * and state, where it runs, the stage row, what blocks or stopped it, the
 * escalation it waits on, the paths it changed, the pass history, how it ended,
 * and the foot bar of actions. A merge run renders on the same terms as every
 * run, with its own title, provenance, publication, unresolved paths and
 * result added (GRU-FR-CXLB). An apply run and a rework run render with the
 * words of their phase, their change, and, for an apply run, the task counts
 * and the check state (GRU-FR-SOFY, GRU-FR-PXCW, GRU-FR-HZLV).
 */

import { useMemo, type MutableRefObject } from "react";

import { logInfo } from "../../logging";
import { formatSectionAddress } from "../../settingsWindow";
import {
  STAGE_TIME_LIMIT_ITEM,
  TOTAL_TIME_LIMIT_ITEM,
} from "../../state/graduationLoopSettings";

import {
  aheadOf,
  blockerStatement,
  blockerDetail,
  blockerFrame,
  openSpecBlockerFailure,
  pushRefusalStatement,
  commitSummary,
  conditionOf,
  conditionSentence,
  failureStatement,
  FAILED_HOLD_STATEMENT,
  runsRemovedByDiscard,
  hasReadableProgress,
  isPrSummaryRun,
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
  totalTimeLimitDetail,
  totalTimeLimitHeadline,
  totalTimeLimitRestOf,
  checksNotPassedRestOf,
  planRestOf,
  changeRunPhase,
  checkStateOf,
  tasksSentence,
} from "../../state/graduation";
import { useApplyProgress } from "../../state/openspec/applyProgress";
import { stagesWithLogAccess } from "../../state/graduation/logScopes";
import type {
  ApplyProgress,
  GraduationCapacity,
  GraduationQueue,
  GraduationRun,
  OpenSpecFailure,
} from "../../types";
import { RunProgress } from "../RunProgress";
import { OpenSpecFailureNotice } from "../OpenSpecFailureNotice";
import { RunActionRow, type PressedAction, type RunAct } from "./actionRow";
import { ChecksNotPassedRest } from "./checksRest";
import { PlanRestNotice } from "./planRest";
import { RunEscalationForm } from "./escalation";
import { EscalationsLog } from "./escalationsLog";
import { RunColumns } from "./columns";
import { MergePaths, MergePublication, MergeResult } from "./merge";
import { MilestoneList } from "./milestones";
import { ProgressLine } from "./progressLine";
import { PrSummaryResult } from "./prSummary";

/**
 * GRU-FR-CKOB: the states whose own paragraph already says what the run is
 * doing, so the condition line that stands in for a missing stage row does not
 * say it twice.
 */
const SAYS_ITSELF = ["blocked", "interrupted", "failed", "discarded"];

/**
 * GRU-FR-CKOB / GRU-FR-WKTL / GRU-FR-WVDV / GRU-FR-TCSM: whether the run's
 * own paragraph says what it is doing. A run at rest on the total time limit,
 * on checks that did not pass, or on a plan rest has one too.
 */
function saysItself(run: GraduationRun): boolean {
  return (
    SAYS_ITSELF.includes(run.state) ||
    totalTimeLimitRestOf(run) !== null ||
    checksNotPassedRestOf(run) !== null ||
    planRestOf(run) !== null
  );
}

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
  onOpenSettings?: (address: string) => void;
  /** GLW-FR-ALZI: the author activated a stage that holds a log. */
  onActivateStage: (stageId: string) => void;
  /** GEA-FR-VIPR: the backend accepted an answer set. */
  onAnswered: () => void;
  /** GRT-FR-XHLN: the run an accepted restart created. */
  onRestarted: (created: GraduationRun) => void;
  /** GRU-FR-KQPE: the refusal of this run's last row act. */
  refusal: string | null;
  /** GRU-FR-BPNJ: the OpenSpec failure of that refusal, for its notice. */
  refusalFailure?: OpenSpecFailure | null;
  pressedRef: MutableRefObject<PressedAction | null>;
  openRestart: boolean;
  onRestartOpened: () => void;
}

/**
 * GRU-FR-PXCW: the sentence of the current stage. The Tasks stage of an apply
 * run that works states the task counts and the next open task, where the
 * last read of its apply progress answered.
 */
function stageSentence(
  run: GraduationRun,
  progress: ApplyProgress | null,
  queue: GraduationQueue | null,
  capacity: GraduationCapacity | null,
): string {
  const tasksStage = run.state === "working" && changeRunPhase(run) === "apply";
  if (progress && tasksStage) {
    return tasksSentence(progress);
  }
  // GRU-FR-LBPR / OFN-FR-PFYW: the notice below states an OpenSpec blocker,
  // so the stage row states no sentence of its own for it.
  if (openSpecBlockerFailure(run)) return "Blocked";
  return conditionSentence(
    run,
    queue ? aheadOf(queue, run.id) : null,
    waitsForSlot(run, capacity),
  );
}

/**
 * GRU-FR-PXCW: a failed read adds the short form of its notice
 * (OFN-FR-QZEC) to the ordinary sentence of the Tasks stage. It is read after
 * the sentence and shows as one line under the stage row.
 */
function stageNote(run: GraduationRun, progressFailure: string | null): string | null {
  const tasksStage = run.state === "working" && changeRunPhase(run) === "apply";
  return tasksStage ? progressFailure : null;
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
  onRestarted,
  refusal,
  refusalFailure = null,
  pressedRef,
  openRestart,
  onRestartOpened,
}: RunRegionProps) {
  const title = runTitle(run);
  const phase = changeRunPhase(run);
  // GRU-FR-PXCW: read when the run is selected and on each change of it. A
  // read that fails leaves the stage on its ordinary sentence, with the short
  // form of its notice.
  const { progress, failure: progressFailure } = useApplyProgress({
    streamId: run.streamId,
    change: run.openspec?.change ?? "",
    runId: run.id,
    enabled: phase === "apply" && Boolean(run.streamId),
    keepOnFailure: false,
    surface: "graduation_run_tasks",
  });
  const checkState = checkStateOf(run);
  // GRU-FR-LBPR: an OpenSpec blocker is stated in its failure notice. A
  // reload gives a new run object with the same blocker, so the failure is
  // read once for each blocker, by its content, and its notice appears once
  // (OFN-FR-LKAT).
  const blockerKey = `${run.state}|${JSON.stringify(run.blocker ?? null)}`;
  const openspecBlocker = useMemo(
    () => openSpecBlockerFailure(run),
    // The key holds everything the failure is read from.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [blockerKey, run.openspec?.change],
  );
  return (
    <>
      <div className="graduation__head">
        {/* GRU-FR-HDPQ: a merge run's title stands where a draft run's draft
            name does. */}
        <span className="graduation__name">{title}</span>
        {/* GRU-FR-YHFQ: a propose run names the change it writes, beside its
            source draft, in every state. An apply run and a rework run name
            their change in the place of a source draft (GRU-FR-HABG). */}
        {run.openspec && (
          <span className="graduation__change t-meta" data-testid="graduation-openspec-change">
            Change {run.openspec.change}
          </span>
        )}
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
      {/* GRU-FR-YHFQ: a rework run renders the author's instruction as
          escaped plain text. */}
      {phase === "rework" && run.openspec?.instruction && (
        <p className="graduation__instruction" data-testid="graduation-rework-instruction">
          <span className="t-eyebrow">Instruction</span>
          <span className="graduation__instruction-text">{run.openspec.instruction}</span>
        </p>
      )}
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
            conditionSentence={stageSentence(run, progress, queue, capacity)}
            conditionNote={stageNote(run, progressFailure)}
            iterationLabel={iterationLabel(run)}
            history={stageHistory(run)}
            // RPV-FR-10: the host renders the moves itself — the review's
            // revisions in the pass history beneath, and the author's retries
            // in the line under this row — so the component draws none and
            // they are read once.
            showLoops={false}
            outcome={outcomeStatement(run)}
            label={`Progress of ${title}`}
            // GRU-FR-HZLV: the check state of an apply run, in words.
            detail={
              checkState ? (
                <p className="t-meta graduation__check-state" data-testid="graduation-check-state">
                  {checkState}
                </p>
              ) : undefined
            }
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
        !saysItself(run) && (
          <p className="graduation__provenance" data-testid="graduation-condition">
            {conditionSentence(
              run,
              queue ? aheadOf(queue, run.id) : null,
              waitsForSlot(run, capacity),
            )}
          </p>
        )
      )}

      {/* GRU-FR-WQHM: the milestones of the run's current turn, under the stage
          row and above everything that follows it. Keyed by run, so another run
          starts with an empty list. GRU-FR-AFSW: the key names the list too,
          because a sibling keyed by the run id alone would share it, and React
          then keeps one run's list in another run's region. */}
      <MilestoneList key={`milestones:${run.id}`} run={run} />
      {/* GRU-FR-NPQY: the agent's newest progress sentence, under the list.
          GRU-FR-SOIJ: keyed by run and by name, as the list is (GRU-FR-AFSW). */}
      <ProgressLine key={`progress:${run.id}`} run={run} />

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

      {/* GRU-FR-LBPR / OFN-FR-DPIJ: an OpenSpec blocker in its notice. The
          Continue of the action row is its retry, so the notice offers none.
          Open change opens the change of the run on its stream. */}
      {openspecBlocker && (
        <div
          className="graduation__blocker graduation__warning"
          data-testid="graduation-blocker"
        >
          <OpenSpecFailureNotice
            failure={openspecBlocker}
            surface="graduation_run_blocker"
            change={run.openspec?.change ?? null}
            location={run.streamId ? { kind: "stream", streamId: run.streamId } : null}
            streamName={run.streamName || undefined}
            frame={blockerFrame(run)}
          />
        </div>
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
                    setting: STAGE_TIME_LIMIT_ITEM,
                  });
                  onOpenSettings(
                    formatSectionAddress("project", "graduation", STAGE_TIME_LIMIT_ITEM),
                  );
                }}
              >
                Change the time limit
              </button>
            </>
          ) : null}
        </p>
      )}

      {/* GRU-FR-WKTL / GRU-FR-TTLR: a run at rest on its total time limit says
          the limit and the time it used, and routes to the setting that sets
          the limit, so the author can raise it before Continue. It spent no
          pass budget, so it says none. */}
      <TotalTimeLimitRest run={run} onOpenSettings={onOpenSettings} />

      {/* GRU-FR-WVDV: a run whose required checks did not pass says why, names
          each check that did not pass, and says that nothing went to review
          and nothing was committed. */}
      <ChecksNotPassedRest run={run} />

      {/* GRU-FR-TCSM / GRU-FR-TVDF / GRU-FR-PZJY: an apply run at rest on a
          plan rest says what it asks, and where the author answers it. It
          holds no escalation, so no question form renders (GRU-FR-FZCN). */}
      <PlanRestNotice run={run} />

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
          answer are a run nothing can release. GRU-FR-XKCB: only while the
          run waits on the author, so a record that still names questions
          while a turn runs offers no answer to them. */}
      {run.escalation && run.state === "awaiting_author" && (
        <RunEscalationForm
          run={run}
          busy={busy}
          onAnswered={onAnswered}
        />
      )}

      {/* GRU-FR-NQLG: the decisions the author made during this run, once,
          read-only and collapsed. It is keyed by run, so another run starts
          collapsed. */}
      <EscalationsLog key={`escalations:${run.id}`} run={run} />

      {/* GRU-FR-MCYF: what the run changed and how it got there, side by side
          where the region is wide enough and stacked where not.
          GRU-FR-AQKZ: a PR summary run shows no change set and no pass. */}
      {!isPrSummaryRun(run) && <RunColumns run={run} />}

      {/* GRU-FR-QYEE / GRU-FR-QMWX: a failed run states the typed failure. It
          is read from the run rather than from the stage row, so a record this
          build cannot render never takes it away. */}
      {run.state === "failed" && (
        <p className="graduation__warning" data-testid="graduation-failure">
          {failureStatement(run)}
        </p>
      )}
      {/* GRU-FR-QYEE: a failed run holds its queue until the author discards
          it, so the runs behind it say they wait and what the discard does. */}
      {run.state === "failed" && runsRemovedByDiscard(queue, run).length > 0 && (
        <p className="t-meta" data-testid="graduation-failed-hold">
          {FAILED_HOLD_STATEMENT}
        </p>
      )}

      {run.state === "completed" && commitSummary(run) && (
        <p className="graduation__outcome" data-testid="graduation-commits">
          {commitSummary(run)}
        </p>
      )}

      {/* GRU-FR-JRMA: what a merge run landed. */}
      <MergeResult run={run} />

      {/* GRU-FR-DQSA: what a completed PR summary run wrote. */}
      <PrSummaryResult run={run} />

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
          queue={queue}
          busy={busy}
          act={act}
          onOpenDraft={onOpenDraft}
          onRestarted={onRestarted}
          refusal={refusal}
          refusalFailure={refusalFailure}
          pressedRef={pressedRef}
          openRestart={openRestart}
          onRestartOpened={onRestartOpened}
        />
      </div>
    </>
  );
}

/** GRU-FR-WKTL / GRU-FR-TTLR: the paragraph of a run at rest on its total time limit. */
function TotalTimeLimitRest({
  run,
  onOpenSettings,
}: {
  run: GraduationRun;
  onOpenSettings?: (address: string) => void;
}) {
  const rest = totalTimeLimitRestOf(run);
  if (!rest) return null;
  return (
    <p className="graduation__warning" data-testid="graduation-rest">
      {totalTimeLimitHeadline(rest)}
      <span className="t-meta" data-testid="graduation-rest-detail">
        {" "}
        {totalTimeLimitDetail(rest)}
      </span>
      {onOpenSettings ? (
        <>
          {" "}
          <button
            type="button"
            className="btn btn--sm btn--ghost graduation__warning-action"
            data-testid="graduation-change-total-time-limit"
            onClick={() => {
              logInfo(["frontend"], "graduation run routed to the time limit setting", {
                runId: run.id,
                setting: TOTAL_TIME_LIMIT_ITEM,
              });
              onOpenSettings(
                formatSectionAddress("project", "graduation", TOTAL_TIME_LIMIT_ITEM),
              );
            }}
          >
            Change the time limit
          </button>
        </>
      ) : null}
    </p>
  );
}
