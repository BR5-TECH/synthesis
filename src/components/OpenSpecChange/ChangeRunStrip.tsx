/**
 * The run strip of a change workspace on a stream
 * (`../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-JHAB,
 * OCW-FR-YTGJ, OCW-FR-BTSI, OCW-FR-FAEL, OCW-FR-LYIX, OCW-FR-NWRS,
 * OCW-FR-YRZK, OCW-FR-ZPDC).
 *
 * The newest apply run or rework run of the change on this stream: its title,
 * its state in words, and its stage row through `RPV-run-progress.md`, with the
 * words of its phase (GOB-FR-CAXV). It takes the full width of the document
 * column, so the discussion column gives it no width. An apply run at rest on
 * a plan rest states it in words, offers Show thread for its plan thread, and
 * offers Continue, whose refusal it states itself.
 */
import { useEffect, useRef, useState } from "react";

import * as api from "../../api";
import { logInfo, logWarn } from "../../logging";
import {
  PLAN_REST_STRIP_WORDS,
  planContinueMessage,
  planRestOf,
  planThreadOf,
  changeRunPhase,
  checkStateOf,
  conditionOf,
  conditionSentence,
  hasReadableProgress,
  isTerminalRun,
  iterationLabel,
  outcomeStatement,
  passOf,
  runTitle,
  stageDescriptorsFor,
  stageHistory,
  stageOf,
  stateLabelOf,
  taskCounts,
} from "../../state/graduation";
import { openGraduationRunRoute } from "../../state/graduation/runRoute";
import { rereadChangeRunQueue } from "../../state/openspec/changeRunQueue";
import { requestChangeDiscussion } from "../../state/openspec/changeRoutes";
import { openSpecBlockerFailure } from "../../state/graduation/blockers";
import {
  logFailureNotice,
  noticeFailureOf,
  refusalString,
  shortForm,
} from "../../state/openspec/failureNotice";
import { useShortFormLog } from "../../state/openspec/useShortFormLog";
import { discussionTargetKey, type ApplyProgress, type GraduationRun } from "../../types";
import { RunProgress } from "../RunProgress";

/**
 * OCW-FR-JHAB: the state of the run in words, with its pass while it works.
 * OCW-FR-YRZK: a plan rest states the words of that rest.
 */
function stateWords(run: GraduationRun): string {
  const plan = planRestOf(run);
  if (plan) return PLAN_REST_STRIP_WORDS[plan.reason];
  const state = stateLabelOf(run);
  return run.state === "working" || run.state === "reviewing"
    ? `${state}, pass ${passOf(run)}`
    : state;
}

export function ChangeRunStrip({
  run,
  progress,
  progressFailure = null,
  onClose,
}: {
  run: GraduationRun;
  /** OCW-FR-BTSI: the last answer of `"read apply progress"`. */
  progress: ApplyProgress | null;
  /** OCW-FR-BTSI: the short form of the failure of the last read (OFN-FR-QZEC). */
  progressFailure?: string | null;
  onClose: () => void;
}) {
  const title = runTitle(run);
  const phase = changeRunPhase(run);
  // OCW-FR-BTSI: the Tasks stage of an apply run states the task counts, and
  // the next open task stands under it (OCW-FR-NWRS). The stage row keeps the
  // counts whole at every width (RPV-FR-16), and the next task alone is cut.
  const tasks = phase === "apply" && run.state === "working" && progress ? progress : null;
  // OCW-FR-BTSI: a failed read keeps the last answer and adds the short form
  // of its notice to the sentence of the Tasks stage.
  const failed = phase === "apply" && run.state === "working" ? progressFailure : null;
  const sentence = tasks ? taskCounts(tasks) : conditionSentence(run);
  // OFN-FR-LKAT: a blocked run states the short form of its OpenSpec blocker
  // in its stage sentence (GRU-FR-LBPR), so the strip logs that notice once.
  useShortFormLog(
    "change_run_strip_blocker",
    run.state === "blocked" ? openSpecBlockerFailure(run) : null,
  );
  // OCW-FR-FAEL: the check state of an apply run. A rework run states nothing.
  const checks = checkStateOf(run);
  const terminal = isTerminalRun(run.state);
  const next = tasks?.nextTask?.trim() || null;
  const state = stateWords(run);
  const plan = planRestOf(run);
  const planThread = planThreadOf(run);
  const continuing = useContinue(run);
  return (
    <section
      className="openspec-runstrip"
      aria-label={`Run of the change: ${title}`}
      data-testid="openspec-run-strip"
      data-phase={phase ?? undefined}
      data-terminal={terminal ? "true" : undefined}
    >
      {/* OCW-FR-NWRS: the title, the state, and the actions take one line. */}
      <div className="openspec-runstrip__head">
        <span className="openspec-runstrip__title" title={title}>
          {title}
        </span>
        {/* OCW-FR-NWRS: a narrow strip can cut the state, so its whole text
            is its tooltip. The text stays whole for assistive technology. */}
        <span
          className="openspec-runstrip__state t-meta"
          title={state}
          data-testid="openspec-run-strip-state"
        >
          {state}
        </span>
        {/* OCW-FR-NWRS: the actions stand together at the end of the head.
            Where the head cannot hold them beside the title and the state,
            they go to a line of their own, so each one stays in the strip. */}
        <span className="openspec-runstrip__actions">
          {/* OCW-FR-YRZK: a run at rest on plan_update or plan_findings selects
              its plan thread in the discussion column. */}
          {planThread && run.openspec && (
            <button
              type="button"
              className="btn btn--sm btn--ghost"
              onClick={() => {
                logInfo(["frontend"], "the run strip selected the plan thread", { runId: run.id });
                requestChangeDiscussion(
                  discussionTargetKey({ kind: "change", streamId: run.streamId, change: run.openspec!.change }),
                  planThread,
                );
              }}
            >
              Show thread
            </button>
          )}
          {/* OCW-FR-ZPDC: a plan rest offers Continue. */}
          {plan && (
            <button
              type="button"
              className="btn btn--sm btn--primary"
              // A disabled button drops the keyboard focus to the page, so the
              // act in flight is marked and guarded instead.
              aria-disabled={continuing.busy ? "true" : undefined}
              onClick={() => void continuing.go()}
            >
              Continue
            </button>
          )}
          <button
            type="button"
            className="btn btn--sm btn--ghost"
            onClick={() => openGraduationRunRoute(run.id)}
          >
            Open run
          </button>
          {/* OCW-FR-YTGJ: a terminal run stays until the author closes it. */}
          {terminal && (
            <button type="button" className="btn btn--sm btn--ghost" onClick={onClose}>
              Close
            </button>
          )}
        </span>
      </div>
      {/* OCW-FR-ZPDC / GRU-FR-BPNJ: a refused Continue, in words, in the strip. */}
      {plan && continuing.refusal && (
        <p
          className="openspec-runstrip__refusal t-ui-sm"
          role="alert"
          data-testid="openspec-run-strip-refusal"
        >
          {continuing.refusal}
        </p>
      )}
      {hasReadableProgress(run.observability) ? (
        <RunProgress
          stages={stageDescriptorsFor(run)}
          currentStage={stageOf(run) ?? "queued"}
          condition={conditionOf(run)}
          conditionSentence={sentence}
          conditionNote={failed}
          iterationLabel={iterationLabel(run)}
          history={stageHistory(run)}
          showLoops={false}
          outcome={outcomeStatement(run)}
          label={`Progress of ${title}`}
          detail={
            checks || next ? (
              <div className="openspec-runstrip__detail">
                {/* OCW-FR-NWRS: the next task is cut with an ellipsis, and its
                    whole text is its tooltip and its accessible name. */}
                {next && (
                  <p
                    className="openspec-runstrip__next t-meta"
                    title={next}
                    aria-label={`Next task: ${next}`}
                    data-testid="openspec-run-strip-next"
                  >
                    next: {next}
                  </p>
                )}
                {checks && (
                  <p className="openspec-runstrip__checks t-meta" data-testid="openspec-run-strip-checks">
                    {checks}
                  </p>
                )}
              </div>
            ) : undefined
          }
        />
      ) : (
        <p className="t-meta openspec-runstrip__sentence">
          {[sentence, failed].filter(Boolean).join(" · ")}
        </p>
      )}
    </section>
  );
}

/**
 * OCW-FR-ZPDC: Continue of a plan rest, through `continue_graduation_run`.
 * A refusal is kept for the run it was about, and goes when the run moves on.
 */
function useContinue(run: GraduationRun) {
  const [busy, setBusy] = useState(false);
  // The guard of the act in flight. A ref, so a second press before the
  // next render is refused too.
  const inFlight = useRef(false);
  const [refusal, setRefusal] = useState<string | null>(null);
  useEffect(() => {
    setRefusal(null);
  }, [run.id, run.state, run.rest?.reason]);
  const go = async () => {
    if (inFlight.current) return;
    inFlight.current = true;
    setBusy(true);
    setRefusal(null);
    logInfo(["frontend"], "the run strip continues a plan rest", {
      runId: run.id,
      reason: run.rest?.reason ?? null,
    });
    try {
      await api.continueGraduationRun(run.id);
      void rereadChangeRunQueue();
    } catch (error) {
      const raw = error instanceof Error ? error.message : refusalString(error);
      logWarn(["frontend"], "the run strip could not continue a plan rest", {
        runId: run.id,
        code: raw.split(":")[0].trim().slice(0, 60),
      });
      // OFP-FR-QLQJ / GRU-FR-BPNJ: an OpenSpec failure is read from the
      // object, so its cause and its CLI state stay apart. The strip has no
      // room for actions, so it states the short form (OFN-FR-QZEC).
      const failure = noticeFailureOf(error);
      const sentence = failure ? shortForm(failure, run.openspec?.change ?? null) : null;
      if (failure && sentence) {
        logFailureNotice("change_run_strip_continue", failure);
        setRefusal(`${sentence} The run did not continue.`);
      } else {
        setRefusal(planContinueMessage(raw));
      }
    } finally {
      inFlight.current = false;
      setBusy(false);
    }
  };
  return { busy, refusal, go };
}
