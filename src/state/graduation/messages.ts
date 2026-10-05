/**
 * The words the graduation surfaces render
 * (`../../../specifications/ui/GRU-graduation-runs.md` GRU-FR-MRPE).
 */

import type { GraduationRun, ReviewSeverity } from "../../types/graduation";
import { MERGE_BRANCH_MOVED_STATEMENT } from "./merge";

/**
 * GRU-FR-XQVG: the one line a typed refusal is rendered as, against the run it
 * is about.
 *
 * A code the surface does not know is rendered as itself rather than dropped:
 * a refusal nobody can read is still a refusal the author must see.
 */
export function graduationErrorMessage(error: string): string {
  const [code, detail] = splitTyped(error);
  switch (code) {
    case "no_project_open":
      return "No project is open.";
    case "draft_not_found":
      return "That draft is not there any more.";
    case "draft_not_single_file":
      return "That draft is not a single prompt file.";
    case "draft_locked_by_graduation":
      return "A run already holds this draft.";
    case "unknown_stream":
      return "That work stream is not there any more.";
    case "run_state_not_permitted":
      return detail
        ? `That is not something a run in “${detail}” can do.`
        : "That is not something this run can do now.";
    case "unknown_run":
      return "That run is not there any more.";
    case "queue_position_stale":
      return "The queue moved while you were looking at it. It has been re-read.";
    case "queue_position_out_of_range":
      return "That position is not one of the queue's.";
    case "run_archived":
      return "A run you have filed away is not one you are arranging.";
    case "run_not_revertable":
      return "This run committed nothing, so there is nothing to revert.";
    case "vendor_image_unconfigured":
      return "This project has no agent image configured for the active agent.";
    case "vendor_image_invalid":
      return "This project's agent image cannot carry a run.";
    case "vendor_execution_unsupported":
      return "The active agent cannot be executed in a container.";
    case "docker_backend_unverified":
      return "The machine's Docker backend has not been verified.";
    case "stream_busy":
      return "A run is working in that work stream.";
    case "stream_dirty":
      return detail
        ? `The work stream holds uncommitted work: ${detail}`
        : "The work stream holds uncommitted work.";
    case "base_dirty":
      return detail
        ? `The branch this stream merges into holds uncommitted work: ${detail}`
        : "The branch this stream merges into holds uncommitted work.";
    case "stream_missing":
      return "That work stream's working copy is not there.";
    case "base_not_checked_out":
      return "No worktree has that stream's base branch checked out.";
    case "stream_unmerged":
      return detail
        ? `That stream holds ${detail} commits its base branch does not.`
        : "That stream holds commits its base branch does not.";
    case "stream_has_runs":
      return "That stream still has runs that have not ended.";
    case "worktree_identity_changed":
      return "The checkout this was for is no longer the one you are in. Nothing was written.";
    case "direct_worktree_detached":
      return "The active worktree is not on a branch, so a run has no branch to work on.";
    case "direct_branch_changed":
      return "The active worktree is on a different branch from the one you confirmed. Nothing was started.";
    case "direct_worktree_dirty":
      return detail
        ? `The active worktree holds uncommitted work: ${detail}. Commit or discard it, then graduate again.`
        : "The active worktree holds uncommitted work. Commit or discard it, then graduate again.";
    case "direct_worktree_missing":
      return "The worktree this run works in is not there.";
    case "direct_target_changed":
      return "The worktree is no longer on the branch this run was started on, so nothing was committed.";
    case "not_a_git_repository":
      return "This project is not in a Git repository, so it has no worktree to work in.";
    case "merge_branch_moved":
      // GRU-FR-EMNV / GEA-FR-HBCF: a Continue or an answer refused because a
      // branch moved says so, and says that nothing was written.
      return MERGE_BRANCH_MOVED_STATEMENT;
    case "direct graduation active":
      return "A graduation run is working in the active worktree. Switching waits until the run ends.";
    default:
      return error;
  }
}

/**
 * A typed refusal is `code` or `code: detail`.
 *
 * An `Error` stringifies with an `Error:` prefix, so it is unwrapped first:
 * a refusal that arrived wrapped is the same refusal.
 */
export function splitTyped(error: string): [string, string | null] {
  const raw = error.startsWith("Error: ") ? error.slice(7) : error;
  const at = raw.indexOf(":");
  if (at === -1) return [raw.trim(), null];
  return [raw.slice(0, at).trim(), raw.slice(at + 1).trim() || null];
}

/**
 * The two checkouts a `worktree_identity_changed` refusal names.
 *
 * The refusal carries them so a surface can say which checkout the act was for
 * and which one the author is in — a message that named neither would leave
 * them to work it out.
 */
export function worktreeIdentityChanged(
  error: unknown,
): { expected: string; active: string } | null {
  const raw = error instanceof Error ? error.message : String(error ?? "");
  // The refusal is `code: detail`, and the detail is the payload. An `Error`
  // wrapper and a bare string carry the same one.
  const [, detail] = splitTyped(raw);
  if (!detail) return null;
  try {
    const parsed = JSON.parse(detail) as { expected?: string; active?: string };
    if (typeof parsed.expected === "string" && typeof parsed.active === "string") {
      return { expected: parsed.expected, active: parsed.active };
    }
  } catch {
    // Not a JSON payload: the refusal carries the code alone, and the caller
    // renders the sentence that names no path.
  }
  return null;
}

/** GRU-FR-QYEE: the typed failure that ended a run. */
export function failureLabel(run: GraduationRun): string | null {
  if (run.state !== "failed") return null;
  return run.failure ? graduationErrorMessage(run.failure.code) : "The run failed.";
}

/** GRU-FR-RRNN: the mark a changed path carries. */
export function entryMark(path: string, changed: string[]): "+" | "~" {
  return changed.includes(path) ? "~" : "+";
}

/** How a finding's severity reads. */
export function severityLabel(severity: ReviewSeverity): string {
  switch (severity) {
    case "critical":
      return "Critical";
    case "major":
      return "Major";
    case "minor":
      return "Minor";
  }
}

/** GRU-FR-YYXN: the heading one pass's row carries. */
export function passHeading(pass: number): string {
  return pass === 1 ? "First pass" : `Pass ${pass}`;
}
