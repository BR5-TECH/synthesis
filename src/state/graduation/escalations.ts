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
import type { QuestionDraftAnswer } from "../questionAnswerDrafts";

/** The questions this run is waiting on, or none. */
export function questionsOf(run: GraduationRun) {
  return run.escalation?.questions ?? [];
}

/**
 * GEA-FR-VWQH: the whole ordered set of answers from the draft, or `null` while
 * it does not cover every recorded question. The backend records nothing from
 * a set that does not cover them all, so composing one would only be refused.
 *
 * GEA-FR-TSQO: a chosen proposed response sends its own answer and summary, and
 * the note where one was written. The own answer sends its text, its first 120
 * characters as the summary (GEA-FR-NXFQ), and no note (GEA-FR-PLTO).
 *
 * GEA-FR-DNBI: an option's position is its 1-based place in the recorded list.
 */
export function composeEscalationAnswers(
  escalation: GraduationEscalation,
  recall: (position: number) => QuestionDraftAnswer,
): EscalationAnswerInput[] | null {
  const composed: EscalationAnswerInput[] = [];
  for (const question of escalation.questions) {
    const draft = recall(question.position);
    if (question.options.length === 0 || draft.ownWords) {
      const answer = draft.typed.trim();
      if (answer === "") return null;
      // Counted in code points, the unit every escalation limit is counted in
      // (ESU-FR-11), so a cut never splits a character in two.
      const summary = Array.from(answer).slice(0, 120).join("");
      composed.push({ position: question.position, answer, summary });
      continue;
    }
    const option = draft.selected === null ? undefined : question.options[draft.selected - 1];
    // A selection naming an option the re-read run no longer records is none.
    if (!option) return null;
    const note = draft.note.trim();
    composed.push({
      position: question.position,
      answer: option.answer,
      summary: option.summary,
      ...(note === "" ? {} : { note }),
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
