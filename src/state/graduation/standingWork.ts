/**
 * What a run does with work standing in its stream
 * (`../../../specifications/ui/GSD-graduation-start-dialog.md` GSD-FR-TBQX,
 * GSD-FR-NWSC).
 *
 * The three positions are the same three wherever the question is asked — the
 * start dialog and the restart confirmation — so both surfaces read from one
 * list rather than each writing the words again.
 */

import type {
  GraduationCapacity,
  GraduationRun,
  StandingWork,
} from "../../types/graduation";
import type { WorkStreamSummary } from "../../types/streams";
import { splitTyped } from "./messages";
import { isTerminalRun } from "./runState";

/** GSD-FR-TBQX: `commit` is the resting position. */
export const DEFAULT_STANDING_WORK: StandingWork = "commit";

export interface StandingWorkPosition {
  value: StandingWork;
  /** What the position is, in the words the control renders. */
  summary: string;
  /** What the run does when its turn comes, which is what is being chosen. */
  description: string;
}

/**
 * GSD-FR-TBQX / GSD-FR-NWSC: the three positions, in the order they read.
 *
 * Each description says what happens at the dispatch rather than what stands in
 * the stream now, because the two moments are not the same one.
 */
export const STANDING_WORK_POSITIONS: StandingWorkPosition[] = [
  {
    value: "keep",
    summary: "Leave it uncommitted",
    description:
      "The run works on top of it, and it becomes part of what the run commits.",
  },
  {
    value: "commit",
    summary: "Commit it first",
    description:
      "It is committed under a message naming you, and the run starts from that commit.",
  },
  {
    value: "commit_and_push",
    summary: "Commit it first, then push the stream branch",
    description:
      "The same commit, then the branch goes to the project's remote. A push the remote refuses does not stop the run.",
  },
];

/**
 * GSD-FR-MZTB: whether this choice makes a commit of its own.
 *
 * The message belongs to that commit, so a choice that commits nothing is
 * asked for no message and sends none.
 */
export function standingWorkCommits(value: StandingWork): boolean {
  return value === "commit" || value === "commit_and_push";
}

/**
 * GSD-FR-WQPD: whether a run onto this stream will wait for it.
 *
 * A stream a run holds, and one with runs already queued on it, both take the
 * new run behind what is there. A stream that holds neither dispatches it at
 * once, so what stands in it now is what the run finds and there is nothing to
 * decide.
 */
export function isOccupied(stream: WorkStreamSummary | null | undefined): boolean {
  if (!stream) return false;
  return Boolean(stream.stream.busyRunId) || stream.queuedRunCount > 0;
}

/**
 * GSD-FR-LHQY: whether the project-wide limit of graduation runs is full.
 *
 * A limit that is `unlimited` is never full (GSD-FR-KDBU). A capacity that
 * could not be read is not full either (GSD-FR-NOID): the dialog states what it
 * knows and no more.
 */
export function isProjectLimitFull(
  capacity: GraduationCapacity | null | undefined,
): boolean {
  if (!capacity || capacity.limit === "unlimited") return false;
  return capacity.inUse >= capacity.limit;
}

/**
 * GSD-FR-HVDN: the commit message an existing stream's field is prefilled with.
 *
 * It is the captured draft name of the newest non-terminal run assigned to the
 * stream. `runs` is the project's run order, earliest first, so the newest run
 * is the last match. A stream with no such run has no default, and an empty
 * field commits under the run's own name.
 */
export function defaultCommitMessage(
  runs: readonly GraduationRun[],
  streamId: string,
): string {
  if (!streamId) return "";
  for (let i = runs.length - 1; i >= 0; i -= 1) {
    const run = runs[i];
    // A merge run has no draft name to offer (GRU-FR-ZKYU).
    if (run.streamId === streamId && !run.merge && !isTerminalRun(run.state)) {
      return run.input.draftName;
    }
  }
  return "";
}

/** The words one position reads as, or none for a value this build has not. */
export function standingWorkPosition(
  value: StandingWork,
): StandingWorkPosition | null {
  return STANDING_WORK_POSITIONS.find((position) => position.value === value) ?? null;
}

/**
 * GRU-FR-TFEZ: what the run region says about a push the remote did not take.
 *
 * A run that asked for no push, one whose push landed, and one the dispatch has
 * not reached all read the same here: there is nothing to say. The sentence
 * states that the run continued, because a report the author cannot act on
 * must not read as a condition they have to clear.
 */
export function pushRefusalStatement(run: GraduationRun): string | null {
  const outcome = run.standingWorkOutcome;
  if (!outcome || outcome.pushed !== false) return null;
  const reason = pushRefusalReason(outcome.pushFailure?.code);
  return reason
    ? `The stream branch was not pushed: ${reason}. The run was not stopped by it.`
    : "The stream branch was not pushed. The run was not stopped by it.";
}

/**
 * The typed cause in words.
 *
 * Every cause the push can report is named here. One this build does not know
 * is rendered as itself rather than dropped: a reason nobody can read is still
 * a reason the author is owed. The detail beside a typed code is dropped — it
 * is a path or a remote, and this is one line inside a run region.
 */
function pushRefusalReason(code: string | undefined): string {
  const [typed] = splitTyped((code ?? "").trim());
  switch (typed) {
    case "":
      return "";
    case "no remote configured":
      return "this project has no remote to push to";
    case "no branch is checked out":
      return "the stream's working copy has no branch checked out";
    case "invalid_token":
      return "the remote did not take the credential this project resolves";
    case "github_unreachable":
      return "the remote could not be reached";
    case "github_token_missing":
    case "github_token_selection_required":
      return "this project resolves no credential for the remote";
    case "github_host_mismatch":
      return "the credential this project names belongs to another host than the remote";
    case "unknown_token":
      return "the credential this project names is not there any more";
    case "keychain_unavailable":
      return "the credential store could not be opened";
    case "github_identity_unresolved":
      return "the remote would not say who the credential belongs to";
    case "push_unavailable":
      return "this machine could not make the push";
    case "push_did_not_finish":
      return "it did not finish in time, and the run went on without waiting";
    case "push_abandoned":
      return "the run stopped before the push finished";
    default:
      return typed;
  }
}
