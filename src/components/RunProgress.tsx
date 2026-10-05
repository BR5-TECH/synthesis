/**
 * The run-progress visualization
 * (`../../specifications/ui/RPV-run-progress.md`).
 *
 * A component rather than a surface (RPV-FR-01): it is opened by nothing,
 * occupies no zone of the window, and renders wherever a host places it. It
 * knows nothing about what any stage means — the ordered stages, the current
 * one, its condition, the iteration, and the moves the work has made are all
 * passed in, so the same component serves a graduation's four stages and any
 * other host's (RPV-FR-03).
 *
 * It invokes nothing, subscribes to nothing, and retains nothing between
 * renders (RPV-FR-18).
 */
import type { ReactNode } from "react";
import {
  conditionWord,
  progressView,
  type ProgressStage,
  type ProgressTransition,
  type StageCondition,
} from "../state/runProgress";

export interface RunProgressProps {
  /** RPV-FR-02: two or more stages, in the order they are rendered. */
  stages: ProgressStage[];
  /** RPV-FR-04: one of the configured ids, or null while none is current. */
  currentStage?: string | null;
  condition: StageCondition;
  /**
   * RPV-FR-08: what is being waited for, what stopped it, what blocks it. Where
   * the host supplies none, the condition's own word is read in its place — a
   * condition is never rendered without a word.
   */
  conditionSentence?: string | null;
  /** RPV-FR-09: rendered on the condition line; omitted entirely when absent. */
  iterationLabel?: string | null;
  /** RPV-FR-10: every move the work has made, in the host's own stage ids. */
  history?: ProgressTransition[];
  /**
   * RPV-FR-10: whether the component states the backward edges itself.
   *
   * A host that renders the same moves in its own detail region turns this off
   * and the moves are read once, where the host put them. The component keeps
   * reading the history either way — it is what a backward edge is decided
   * from — but it neither draws a list of its own nor announces one.
   */
  showLoops?: boolean;
  /** RPV-FR-12: one line for work that has ended. */
  outcome?: string | null;
  /** RPV-FR-13: rendered beneath everything, and interpreted not at all. */
  detail?: ReactNode;
  /** The accessible name of the whole region. */
  label: string;
  /**
   * RPV-FR-NEVJ: an activatable stage was activated.
   *
   * Reporting it is the whole of what activation does here. The component
   * opens nothing, changes no current stage, and holds no selection of its own.
   */
  onActivateStage?: (stageId: string) => void;
}

export function RunProgress({
  stages,
  currentStage,
  condition,
  conditionSentence,
  iterationLabel,
  history = [],
  showLoops = true,
  outcome,
  detail,
  label,
  onActivateStage,
}: RunProgressProps) {
  const view = progressView(stages, currentStage, condition, history);
  // RPV-FR-JSQW: a configuration in which no stage is activatable renders
  // exactly as it does without them, and adds no control and no focus stop.
  const interactive = stages.some((stage) => stage.activatable);
  const sentence = conditionSentence?.trim()
    ? conditionSentence.trim()
    : conditionWord(condition);

  // RPV-FR-15: one region naming, in order, the stages, which are complete,
  // which is current, its condition and sentence, the iteration, every loop,
  // and the outcome. Assembled here rather than left to a reader stitching
  // together separate nodes, and announced when the stage or condition changes.
  const spoken = [
    `${label}.`,
    ...view.stages.map((stage) => {
      const named = stage.description ?? stage.label;
      if (stage.status === "complete") return `${named}: done.`;
      if (stage.status === "current") return `${named}: ${sentence}.`;
      return `${named}: not started.`;
    }),
    iterationLabel ? `${iterationLabel}.` : null,
    ...(showLoops ? view.loops : []).map(
      (loop) =>
        `Went back from ${loop.from.label} to ${loop.to.label}` +
        (loop.iteration != null ? ` on iteration ${loop.iteration}.` : "."),
    ),
    view.showsOutcome && outcome ? outcome : null,
  ]
    .filter(Boolean)
    .join(" ");

  return (
    <div
      className="run-progress"
      data-testid="run-progress"
      data-animates={view.animates ? "true" : "false"}
      data-reached={view.reached}
      data-total={view.total}
    >
      {/* RPV-FR-15: the whole region as one announcement, so a change of stage
          or condition reaches a screen reader as a sentence rather than as a
          scatter of re-read nodes. The marks and labels below are hidden from
          it for the same reason — they say the same thing less well. */}
      <p className="sr-only" role="status" aria-live="polite">
        {spoken}
      </p>

      <ol
        className="run-progress__stages"
        // The marks and labels say what the region announcement above already
        // says, so they are hidden from assistive technology — except where a
        // stage is a control, which must be reachable and named (RPV-FR-LQUP).
        aria-hidden={interactive ? undefined : "true"}
      >
        {view.stages.map((stage) => {
          const conditionWords =
            stage.status === "complete"
              ? "Done"
              : stage.status === "current"
                ? sentence
                : "—";
          // RPV-FR-LQUP: the mark, the label, and the condition together are
          // the entry, so the whole of it is one control rather than three.
          const body = (
            <>
              <span className="run-progress__mark" />
              <span className="run-progress__label">{stage.label}</span>
              {/* RPV-FR-08: every condition in words on the stage it applies
                  to, never by a colour, a mark, or a motion alone. RPV-FR-16
                  keeps this one whole at the smallest width. */}
              <span className="run-progress__condition">{conditionWords}</span>
            </>
          );
          // RPV-FR-MAIP: a stage that may not be activated is a disabled
          // entry where the host said why, and plain text where it did not —
          // an entry is never a control with no account of what it does.
          const disabled = interactive && !stage.activatable;
          const control = stage.activatable || (interactive && stage.disabledReason);
          return (
            <li
              key={stage.id}
              className="run-progress__stage"
              data-stage={stage.id}
              data-status={stage.status}
              data-condition={stage.status === "current" ? condition : undefined}
            >
              {control ? (
                <button
                  type="button"
                  className="run-progress__entry"
                  data-testid={`run-progress-stage-${stage.id}`}
                  // RPV-FR-MAIP: a disabled entry keeps its focus stop, so the
                  // sentence saying why it does nothing is announced rather
                  // than skipped over.
                  aria-disabled={disabled ? "true" : undefined}
                  aria-label={
                    disabled
                      ? `${stage.label}. ${conditionWords}. ${stage.disabledReason}`
                      : `${stage.label}. ${conditionWords}.`
                  }
                  onClick={() => {
                    if (disabled) return;
                    onActivateStage?.(stage.id);
                  }}
                >
                  {body}
                </button>
              ) : (
                body
              )}
            </li>
          );
        })}
      </ol>

      {(iterationLabel || (view.showsOutcome && outcome)) && (
        <p className="run-progress__line t-meta" aria-hidden="true">
          {iterationLabel}
          {iterationLabel && view.showsOutcome && outcome ? " · " : ""}
          {view.showsOutcome ? outcome : null}
        </p>
      )}

      {/* RPV-FR-10: one indication per backward edge, naming both stages and
          the iteration it happened on. RPV-FR-11 is what keeps this from
          reading as progress: the extent above is the current stage's own
          position however many of these there are. */}
      {(showLoops ? view.loops : []).map((loop, at) => (
        <p
          // Two backward edges can share both stages and an iteration — a run
          // sent round twice within one turn — so the position in the history
          // is what makes the key unique.
          key={`${at}-${loop.from.id}-${loop.to.id}`}
          className="run-progress__loop t-meta"
          aria-hidden="true"
        >
          {/* RPV-FR-17: no icon of its own. What says a loop happened is the
              two stage names and the word between them. */}
          Went back: {loop.from.label} → {loop.to.label}
          {loop.iteration != null ? ` on iteration ${loop.iteration}` : ""}
        </p>
      ))}

      {/* RPV-FR-13: whatever the host put here, beneath everything this
          component draws, and interpreted not at all. */}
      {detail}
    </div>
  );
}
