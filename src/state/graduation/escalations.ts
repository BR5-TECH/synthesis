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
import { totalTimeLimitRestOf } from "./rest";
import type { NotificationLevel } from "../toasts";

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

/**
 * GRU-FR-BLSS: the level a run's raise carries. A run that waits on the author
 * or whose publication stopped is Warn. A run that failed, or that stopped on a
 * failure reason, is Error. A completed or discarded run is Info.
 */
export function raiseLevel(run: GraduationRun): NotificationLevel {
  switch (run.state) {
    case "awaiting_author":
    case "blocked":
      return "Warn";
    case "failed":
    case "interrupted":
      return "Error";
    default:
      return "Info";
  }
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

/**
 * NTF-FR-BDNY / GXD-FR-QUCP: the cause a sandbox launch failure names, read
 * from the run's detail, or `null` where the detail is not a sandbox failure.
 */
export function sandboxFailureCause(run: GraduationRun): string | null {
  const interruption = run.interruption;
  if (!interruption || interruption.reason !== "launch_failed") return null;
  const match = /^The Microsandbox VM could not start: (.+?)\.(?: |$)/.exec(interruption.detail ?? "");
  return match ? match[1] : null;
}

/** GRU-FR-FJZD / NTF-FR-BDNY: the stop sentence of an interrupted run. */
function interruptedStatement(run: GraduationRun, title: string): string {
  if (!run.interruption) return `“${title}” stopped.`;
  const sandbox = sandboxFailureCause(run);
  if (sandbox) return `“${title}” stopped: the Microsandbox VM could not start — ${sandbox}.`;
  return `“${title}” stopped: ${interruptionCause(run.interruption.reason)}.`;
}

/**
 * GRU-FR-BLSS / GRU-FR-WKTL: the line of a run that waits on the author. A run
 * at rest on the total time limit says that limit, not a spent pass budget.
 */
function awaitingStatement(run: GraduationRun, title: string): string {
  if (run.escalation) return `“${title}” is waiting for your answer.`;
  if (totalTimeLimitRestOf(run)) return `“${title}” stopped at its total time limit.`;
  return `“${title}” is waiting for your decision.`;
}

/** GRU-FR-BLSS: the one line the notification carries. */
export function raiseStatement(run: GraduationRun): string {
  const title = runTitle(run);
  if (run.merge) {
    switch (run.state) {
      case "awaiting_author":
        return awaitingStatement(run, title);
      case "blocked":
        return `“${title}” is blocked.`;
      case "completed":
        return `“${title}” is merged into ${run.merge.baseBranch}.`;
      case "discarded":
        return `“${title}” was discarded.`;
      case "failed":
        return `“${title}” failed.`;
      // GRU-FR-CXLB / NTF-FR-BDNY: a merge run stops on the terms every run has.
      case "interrupted":
        return interruptedStatement(run, title);
      default:
        return `“${title}” changed.`;
    }
  }
  switch (run.state) {
    case "awaiting_author":
      return awaitingStatement(run, title);
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
      return interruptedStatement(run, title);
    default:
      return `“${title}” changed.`;
  }
}

/** The notification address a run's raise carries. */
export function runAddress(run: GraduationRun): string {
  return `run/${run.id}`;
}
