/**
 * The run strip of a change workspace on a stream
 * (`../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-JHAB,
 * OCW-FR-YTGJ, OCW-FR-BTSI, OCW-FR-FAEL, OCW-FR-LYIX, OCW-FR-NWRS).
 *
 * The newest apply run or rework run of the change on this stream: its title,
 * its state in words, and its stage row through `RPV-run-progress.md`, with the
 * words of its phase (GOB-FR-CAXV). It takes the full width of the document
 * column, so the discussion column gives it no width.
 */
import {
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
import type { ApplyProgress, GraduationRun } from "../../types";
import { RunProgress } from "../RunProgress";

/** OCW-FR-JHAB: the state of the run in words, with its pass while it works. */
function stateWords(run: GraduationRun): string {
  const state = stateLabelOf(run);
  return run.state === "working" || run.state === "reviewing"
    ? `${state}, pass ${passOf(run)}`
    : state;
}

export function ChangeRunStrip({
  run,
  progress,
  onClose,
}: {
  run: GraduationRun;
  /** OCW-FR-BTSI: the last answer of `"read apply progress"`. */
  progress: ApplyProgress | null;
  onClose: () => void;
}) {
  const title = runTitle(run);
  const phase = changeRunPhase(run);
  // OCW-FR-BTSI: the Tasks stage of an apply run states the task counts, and
  // the next open task stands under it (OCW-FR-NWRS). The stage row keeps the
  // counts whole at every width (RPV-FR-16), and the next task alone is cut.
  const tasks = phase === "apply" && run.state === "working" && progress ? progress : null;
  const sentence = tasks ? taskCounts(tasks) : conditionSentence(run);
  // OCW-FR-FAEL: the check state of an apply run. A rework run states nothing.
  const checks = checkStateOf(run);
  const terminal = isTerminalRun(run.state);
  const next = tasks?.nextTask?.trim() || null;
  const state = stateWords(run);
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
        <span className="spacer" />
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
      </div>
      {hasReadableProgress(run.observability) ? (
        <RunProgress
          stages={stageDescriptorsFor(run)}
          currentStage={stageOf(run) ?? "queued"}
          condition={conditionOf(run)}
          conditionSentence={sentence}
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
        <p className="t-meta openspec-runstrip__sentence">{sentence}</p>
      )}
    </section>
  );
}
