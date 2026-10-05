/**
 * The questions a run stopped to ask, and the answers the author gives
 * (`../../../specifications/ui/GEA-graduation-escalation-answering.md`).
 */

import { runTitle } from "./merge";
import type {
  EscalationAnswerInput,
  GraduationEscalation,
  GraduationRun,
} from "../../types/graduation";
import { interruptionCause, isFailureStop } from "./stages";

/** The questions this run is waiting on, or none. */
export function questionsOf(run: GraduationRun) {
  return run.escalation?.questions ?? [];
}

/** GEA: how many of the questions the author has answered so far. */
export function answeredCount(
  answers: Record<number, string>,
  escalation: GraduationEscalation | null | undefined,
): number {
  if (!escalation) return 0;
  return escalation.questions.filter((q) => (answers[q.position] ?? "").trim() !== "")
    .length;
}

/**
 * GXD-FR-BJYT: the answers, covering every recorded question and nothing else.
 *
 * A set that does not cover them all is not sent: the backend records nothing
 * from one, and composing it here would only produce a refusal.
 */
export function composeAnswers(
  escalation: GraduationEscalation,
  answers: Record<number, string>,
  summaries: Record<number, string>,
): EscalationAnswerInput[] | null {
  const composed: EscalationAnswerInput[] = [];
  for (const question of escalation.questions) {
    const answer = (answers[question.position] ?? "").trim();
    if (answer === "") return null;
    composed.push({
      position: question.position,
      answer,
      summary: (summaries[question.position] ?? answer).slice(0, 120),
    });
  }
  return composed;
}

/**
 * GRU-FR-BLSS: whether this run raises for the author's attention. An
 * interrupted run raises when it stopped on a failure, and not when the author
 * or the application stopped it on purpose.
 */
export function raisesForRun(run: GraduationRun): boolean {
  if (run.state === "interrupted") {
    return run.interruption ? isFailureStop(run.interruption.reason) : false;
  }
  return (
    run.state === "awaiting_author" ||
    run.state === "blocked" ||
    run.state === "completed" ||
    run.state === "discarded" ||
    run.state === "failed"
  );
}

/** A stable key for the raise a run currently carries. */
export function raiseKey(run: GraduationRun): string {
  return `${run.id}:${run.state}`;
}

/**
 * GRU-FR-BLSS: the title the notification carries.
 *
 * A merge run's reads as a merge, and no wording of it calls it a graduation
 * of a draft (GRU-FR-ZKYU).
 */
export function raiseTitle(run: GraduationRun, stateWord: string): string {
  return run.merge ? `Merge · ${stateWord}` : `Graduation · ${stateWord}`;
}

/** GRU-FR-BLSS: the one line the notification carries. */
export function raiseStatement(run: GraduationRun): string {
  const title = runTitle(run);
  if (run.merge) {
    switch (run.state) {
      case "awaiting_author":
        return run.escalation
          ? `“${title}” is waiting for your answer.`
          : `“${title}” is waiting for your decision.`;
      case "blocked":
        return `“${title}” is blocked.`;
      case "completed":
        return `“${title}” is merged into ${run.merge.baseBranch}.`;
      case "discarded":
        return `“${title}” was discarded.`;
      case "failed":
        return `“${title}” failed.`;
      default:
        return `“${title}” changed.`;
    }
  }
  switch (run.state) {
    case "awaiting_author":
      return run.escalation
        ? `“${title}” is waiting for your answer.`
        : `“${title}” is waiting for your decision.`;
    case "blocked":
      return `“${title}” is blocked.`;
    case "completed":
      return `“${title}” is committed on ${run.directTarget ? run.directTarget.branch : run.streamName}.`;
    case "discarded":
      return `“${title}” was discarded.`;
    case "failed":
      return `“${title}” failed.`;
    // GRU-FR-FJZD: the run and the cause its headline states.
    case "interrupted":
      return run.interruption
        ? `“${title}” stopped: ${interruptionCause(run.interruption.reason)}.`
        : `“${title}” stopped.`;
    default:
      return `“${title}” changed.`;
  }
}

/** The notification address a run's raise carries. */
export function runAddress(run: GraduationRun): string {
  return `run/${run.id}`;
}
