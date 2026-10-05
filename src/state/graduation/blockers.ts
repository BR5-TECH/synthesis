/**
 * What blocks a run, and what the author does about it
 * (`../../../specifications/ui/GRU-graduation-runs.md` GRU-FR-LBPR).
 */

import type { GraduationBlocker, GraduationRun } from "../../types/graduation";
import { MERGE_BLOCKER_SENTENCES, MERGE_BLOCKER_TAIL } from "./merge";

/**
 * GRU-FR-LBPR: what each blocker code means, in the author's terms.
 *
 * The backend's own message is an internal error string. It names an absolute
 * path and a Rust error shape, so it belongs beside the sentence as detail
 * rather than standing in for one.
 */
const SENTENCES: Record<string, string> = {
  stream_missing: "The work stream's working copy is not there.",
  base_commit_failed: "The run could not measure a starting point in the stream.",
  task_invalid: "The run could not compose a task for the agent.",
  review_checkout_failed: "The review could not be given a checkout to stand in.",
  review_verdict_invalid: "The review did not answer with a verdict this application can read.",
  commit_failed: "The work could not be committed onto the stream.",
  // GRU-FR-DBUS: the three ways a merge run's apply is blocked.
  ...MERGE_BLOCKER_SENTENCES,
};

/**
 * GRU-FR-LBPR: what this blocker means, in the author's terms.
 *
 * A code this build does not know falls back to the backend's own message,
 * which is the only account of it there is.
 */
export function blockerSentence(blocker: GraduationBlocker): string {
  return SENTENCES[blocker.code] ?? blocker.message;
}

/** Whether this run rests on a blocker, in either state that carries one. */
function blockerOf(run: GraduationRun): GraduationBlocker | null {
  if (run.state !== "blocked" && run.state !== "awaiting_author") return null;
  return run.blocker ?? null;
}

/**
 * GRU-FR-LBPR / GRU-FR-FZCN: what is blocking the run, how often it has blocked
 * on the same thing, that nothing was committed, and the act that clears it.
 */
export function blockerStatement(run: GraduationRun): string | null {
  const blocker = blockerOf(run);
  if (!blocker) return null;
  const sentence = blockerSentence(blocker);
  const attempt = blockerAttemptStatement(blocker);
  // GRU-FR-DBUS: an apply blocker says that nothing was written to either
  // branch, that no new pass is spent, and that Continue retries the apply.
  if (blocker.code in MERGE_BLOCKER_SENTENCES) {
    return [sentence, attempt, MERGE_BLOCKER_TAIL].filter(Boolean).join(" ");
  }
  const clears =
    run.state === "awaiting_author"
      ? "Continue the run to try again, or discard it."
      : blocker.clearsBy;
  return [sentence, attempt, "Nothing was committed.", clears]
    .filter(Boolean)
    .join(" ");
}

/**
 * GRU-FR-LBPR: how many times the run blocked on the same condition.
 *
 * A record written before the blocker carried an attempt reads zero, and says
 * nothing rather than claiming a first attempt it cannot know about.
 */
export function blockerAttemptStatement(blocker: GraduationBlocker): string | null {
  if (!blocker.attempt) return null;
  return blocker.attempt === 1
    ? "This is the first time it stopped here."
    : `It stopped here ${blocker.attempt} times in a row.`;
}

/**
 * The backend's own account of the failure, for the secondary line.
 *
 * Kept apart from the sentence above so an internal path never reads as the
 * thing the author is being asked to act on.
 */
export function blockerDetail(run: GraduationRun): string | null {
  return blockerOf(run)?.message ?? null;
}

/** The typed code the blocker carries, for a surface that renders one. */
export function blockerCode(run: GraduationRun): string | null {
  return blockerOf(run)?.code ?? null;
}

/**
 * GRU-FR-WJHV: the paths a run's work touched that the repository's ignore
 * rules hide.
 *
 * Authored work an ignore rule hides is committed by nothing, so it is said
 * rather than left silent.
 */
export function hiddenPathStatement(hidden: string[]): string | null {
  if (hidden.length === 0) return null;
  return hidden.length === 1
    ? "One path this run wrote is hidden by the repository's ignore rules and was not committed."
    : `${hidden.length} paths this run wrote are hidden by the repository's ignore rules and were not committed.`;
}
