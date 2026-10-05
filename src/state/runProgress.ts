/**
 * The pure rules the run-progress visualization renders from
 * (`../../specifications/ui/RPV-run-progress.md`).
 *
 * It knows nothing about what any stage means. The ordered stages, the current
 * one, its condition, and the moves the work has made are all passed in by the
 * host surface, so the same derivation serves a graduation's four stages and
 * any other host's (RPV-FR-03). Nothing here fetches, subscribes, or retains
 * anything (RPV-FR-18).
 */

/** RPV-FR-04: the six conditions a current stage may be in. */
export type StageCondition =
  | "active"
  | "waiting"
  | "paused"
  | "blocked"
  | "stopped"
  | "complete";

/** RPV-FR-02: one configured stage. */
export interface ProgressStage {
  /** Stable within one configuration; what `currentStage` names. */
  id: string;
  /** The short word beneath the mark. */
  label: string;
  /**
   * The sentence assistive technology reads where the label alone is not one.
   * Omitted where the label says it.
   */
  description?: string;
  /**
   * RPV-FR-JSQW / RPV-FR-RTQB: whether the host lets this stage be activated.
   *
   * The host's data rather than this component's rule. No stage id, position,
   * or condition makes a stage activatable here.
   */
  activatable?: boolean;
  /** RPV-FR-MAIP: the sentence saying why an inactivatable entry does nothing. */
  disabledReason?: string | null;
}

/** RPV-FR-10: one move the work has made, in the host's own stage ids. */
export interface ProgressTransition {
  from?: string | null;
  to: string;
  iteration?: number | null;
}

/** RPV-FR-05: what one stage renders as. */
export type StageStatus = "complete" | "current" | "not_started";

export interface StageView extends ProgressStage {
  status: StageStatus;
  /** Present only on the current stage. */
  condition?: StageCondition;
  index: number;
}

/** RPV-FR-10: a move to a stage earlier in the configured order. */
export interface LoopEdge {
  from: ProgressStage;
  to: ProgressStage;
  iteration?: number | null;
}

export interface ProgressView {
  stages: StageView[];
  /**
   * RPV-FR-11: the current stage's position in the configured order, and
   * nothing else — never accumulated across iterations, never past the end.
   */
  reached: number;
  total: number;
  loops: LoopEdge[];
  /** RPV-FR-07: the one condition that may animate. */
  animates: boolean;
  /** RPV-FR-12: whether an outcome statement belongs on screen. */
  showsOutcome: boolean;
}

/** RPV-FR-07: only `active` is work in progress. */
export function animates(condition: StageCondition): boolean {
  return condition === "active";
}

/**
 * RPV-FR-08: the word a condition reads as when the host supplied no sentence.
 *
 * A condition is never rendered without a word — that is what keeps the meaning
 * off colour, shape, and motion alone.
 */
export function conditionWord(condition: StageCondition): string {
  switch (condition) {
    case "active":
      return "Working";
    case "waiting":
      return "Waiting";
    case "paused":
      return "Paused";
    case "blocked":
      return "Blocked";
    case "stopped":
      return "Stopped";
    case "complete":
      return "Done";
  }
}

/**
 * RPV-FR-05 / RPV-FR-06 / RPV-FR-11: the whole of what the component draws.
 *
 * Completion is **positional**: everything before the current stage is
 * complete, the current one carries its condition, everything after it has not
 * started. That one rule is what makes `stopped` safe — a run that ended
 * without succeeding completes nothing after where it stopped — and what makes
 * a loop harmless, because a stage reached twice renders at its own position
 * both times rather than adding to an extent.
 */
export function progressView(
  stages: ProgressStage[],
  currentStage: string | null | undefined,
  condition: StageCondition,
  history: ProgressTransition[] = [],
): ProgressView {
  const index = stages.findIndex((stage) => stage.id === currentStage);
  // RPV-FR-04: a current stage the configuration does not hold renders every
  // stage as not started rather than guessing which was meant.
  const current = index >= 0 ? index : null;
  // RPV-FR-06: every stage is complete in exactly one arrangement — `complete`
  // on the last configured stage.
  const last = stages.length - 1;
  const allComplete = current === last && condition === "complete";

  const view: StageView[] = stages.map((stage, at) => {
    if (current === null) {
      return { ...stage, index: at, status: "not_started" as const };
    }
    // A stage whose own condition is `complete` is complete, wherever it sits:
    // "the current one renders in its condition" (RPV-FR-05) and its condition
    // is that it finished. Only the last stage carrying it completes the whole,
    // so there is no arrangement in which the work reads as wholly done before
    // its last stage says so (RPV-FR-06).
    if (at < current || (condition === "complete" && at === current)) {
      return { ...stage, index: at, status: "complete" as const };
    }
    if (at === current) {
      return { ...stage, index: at, status: "current" as const, condition };
    }
    return { ...stage, index: at, status: "not_started" as const };
  });

  const position = new Map(stages.map((stage, at) => [stage.id, at]));
  const loops: LoopEdge[] = [];
  for (const move of history) {
    if (!move.from) continue;
    const from = position.get(move.from);
    const to = position.get(move.to);
    if (from === undefined || to === undefined || to >= from) continue;
    loops.push({
      from: stages[from],
      to: stages[to],
      iteration: move.iteration ?? null,
    });
  }

  return {
    stages: view,
    // The extent is the current stage's own position, bounded by the list.
    reached: current === null ? 0 : allComplete ? stages.length : current,
    total: stages.length,
    loops,
    animates: current !== null && animates(condition),
    showsOutcome:
      current !== null && (condition === "stopped" || condition === "complete"),
  };
}
