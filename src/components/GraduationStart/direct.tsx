/**
 * **Work directly**: the worktree the run will write in
 * (`../../../specifications/ui/GSD-graduation-start-dialog.md` GSD-FR-FQQP,
 * GSD-FR-USOH).
 *
 * It names the worktree and the branch the preflight reports, says what the run
 * does there, and gives the reason in words when the choice cannot be
 * confirmed. It asks no standing-work choice and no commit message.
 */

import type { DirectGraduationPreflight } from "../../types";

/** Why **Work directly** cannot be confirmed, or `null` when it can be. */
export function directBlocker(
  preflight: DirectGraduationPreflight | null,
): string | null {
  if (!preflight) return null;
  if (preflight.isDetached || !preflight.branch) {
    return "This worktree is not on a branch, so a run has no branch to work on. Check a branch out first.";
  }
  if (preflight.dirtyPaths.length > 0) {
    return "This worktree holds uncommitted work. Commit or discard it, then graduate again.";
  }
  return null;
}

export interface DirectChoiceProps {
  /** `null` while the preflight is being read. */
  preflight: DirectGraduationPreflight | null;
  /** The refusal the preflight itself returned. */
  unreadable: string | null;
}

export function DirectChoice({ preflight, unreadable }: DirectChoiceProps) {
  if (unreadable) {
    return (
      <p
        className="graduation-start__notice"
        role="alert"
        data-testid="graduation-direct-unreadable"
      >
        {unreadable}
      </p>
    );
  }
  if (!preflight) {
    return <p className="t-ui-sm">Reading the active worktree…</p>;
  }
  const blocker = directBlocker(preflight);
  return (
    <div data-testid="graduation-direct">
      {preflight.branch && (
        <p className="t-ui-sm" data-testid="graduation-direct-target">
          The run works in <strong>{preflight.worktreeName}</strong>, on branch{" "}
          <strong>{preflight.branch}</strong>, and commits there. Edits you make
          in that worktree while it works are part of the commit.
        </p>
      )}
      <p className="t-ui-xs graduation-start__note" data-testid="graduation-direct-path">
        {preflight.worktreePath}
      </p>
      {preflight.stream && (
        <p className="t-ui-xs graduation-start__note" data-testid="graduation-direct-stream">
          This worktree is the working copy of the work stream “
          {preflight.stream.streamName}”. The run joins that stream’s queue.
        </p>
      )}
      {blocker && (
        <p
          className="graduation-start__notice"
          role="alert"
          data-testid="graduation-direct-blocker"
        >
          {blocker}
        </p>
      )}
      {preflight.dirtyPaths.length > 0 && (
        <ul className="graduation-start__paths" data-testid="graduation-direct-dirty">
          {preflight.dirtyPaths.map((path) => (
            <li key={path} className="t-meta">
              {path}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
