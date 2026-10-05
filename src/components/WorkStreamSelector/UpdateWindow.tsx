/**
 * The stream update window
 * (`../../../specifications/ui/WSS-work-stream-selector.md` WSS-FR-NLXD,
 * WSS-FR-WPGR, WSS-FR-CJYE, WSS-FR-QSAF).
 *
 * Update brings the stream branch up to the branch the stream **records** as
 * its base, whichever branch the active worktree happens to hold. The window
 * names that branch, shows the revision the listing read, lists the commits the
 * stream is missing, and offers the two strategies.
 *
 * The displayed revision is what confirming submits. The backend refuses with
 * `stale_base_revision` where the branch has left it, so what the author judged
 * the commit list by is what the update runs on, and a branch that moved is
 * told rather than silently used (WSS-FR-CJYE).
 */

import type { StreamUpdateStrategy, WorkStreamSummary } from "../../types";
import { STREAM_ERRORS } from "../../types";
import { namedPaths } from "./rows";

/** WSS-FR-QSAF: which worktree the refusal is about, and the paths in it. */
export interface UpdateDirtyRefusal {
  /** `stream_dirty`, `base_dirty`, or `base_not_checked_out`. */
  code: string;
  paths: string[];
}

/** WSS-FR-NLXD: how many of the missing commits the window lists. */
const COMMITS_SHOWN = 8;

export function UpdateWindow({
  summary,
  dirty,
  stale,
  onCancel,
  onStart,
  onOpenChanges,
}: {
  summary: WorkStreamSummary;
  /** WSS-FR-QSAF: which worktree was refused, and the complete path set in it. */
  dirty: UpdateDirtyRefusal | null;
  /** WSS-FR-CJYE: set where the base branch moved since this window read it. */
  stale: string | null;
  onCancel: () => void;
  onStart: (summary: WorkStreamSummary, strategy: StreamUpdateStrategy) => void;
  onOpenChanges: () => void;
}) {
  const { stream, behindBase, baseTipRevision, missingCommits } = summary;
  const shortRevision = baseTipRevision.slice(0, 7);

  if (dirty) {
    return (
      <div
        className="stream-select__confirm"
        role="group"
        data-testid={`stream-update-window-${stream.id}`}
      >
        {/* WSS-FR-QSAF: the refusal names the worktree it is about. The two are
            settled in different checkouts, so an author told only "not
            committed" would not know which one to open. */}
        <p role="alert">{dirtySentence(dirty, stream.baseBranch, stream.name)}</p>
        {dirty.paths.length > 0 && (
          <ul className="stream-select__paths" data-testid="stream-update-dirty">
            {dirty.paths.map((path) => (
              <li key={path}>{path}</li>
            ))}
          </ul>
        )}
        <div className="modal__actions">
          <button className="btn btn--ghost btn--sm" onClick={onCancel}>
            Close
          </button>
          {dirty.code !== STREAM_ERRORS.baseNotCheckedOut && (
            <button className="btn btn--default btn--sm" onClick={onOpenChanges}>
              Open Changes
            </button>
          )}
        </div>
      </div>
    );
  }

  // WSS-FR-CJYE: the author reopens Update for a new revision. This surface
  // re-runs nothing on its own, because the commit list they read is stale too.
  if (stale) {
    return (
      <div
        className="stream-select__confirm"
        role="group"
        data-testid={`stream-update-window-${stream.id}`}
      >
        <p role="alert" data-testid="stream-update-stale">
          {stale}
        </p>
        <div className="modal__actions">
          <button className="btn btn--default btn--sm" onClick={onCancel}>
            Close
          </button>
        </div>
      </div>
    );
  }

  return (
    <div
      className="stream-select__confirm"
      role="group"
      data-testid={`stream-update-window-${stream.id}`}
    >
      <p>
        Update {stream.name} from {stream.baseBranch} at{" "}
        <span title={baseTipRevision} data-testid="stream-update-revision">
          {shortRevision}
        </span>
        .
      </p>
      <p className="t-muted">
        {behindBase} commit{behindBase === 1 ? "" : "s"} on {stream.baseBranch}{" "}
        {behindBase === 1 ? "is" : "are"} not in this stream. Only this stream
        changes; {stream.baseBranch} does not move.
      </p>
      <ul className="stream-select__paths" data-testid="stream-update-commits">
        {missingCommits.slice(0, COMMITS_SHOWN).map((commit) => (
          <li key={commit.revision} title={commit.revision}>
            <span className="stream-select__path">
              {commit.revision.slice(0, 7)}
            </span>{" "}
            <span className="t-muted">{commit.summary}</span>
          </li>
        ))}
        {missingCommits.length > COMMITS_SHOWN && (
          <li className="t-muted">
            and {missingCommits.length - COMMITS_SHOWN} more
          </li>
        )}
      </ul>
      <p className="t-muted">
        A path Git cannot settle is settled by one agent turn. An update that
        conflicts changes nothing until that turn has run.
      </p>
      {/* WSS-FR-WPGR: the two strategies stand one above the other, so neither
          reads as the default. */}
      <div className="modal__actions modal__actions--stacked">
        <button
          className="btn btn--default btn--sm"
          data-testid={`stream-update-merge-source-${stream.id}`}
          onClick={() => onStart(summary, "merge_source")}
        >
          Merge source into stream
        </button>
        <button
          className="btn btn--default btn--sm"
          data-testid={`stream-update-rebase-source-${stream.id}`}
          onClick={() => onStart(summary, "rebase_source")}
        >
          Rebase stream onto source
        </button>
        <button
          className="btn btn--ghost btn--sm"
          data-testid={`stream-update-cancel-window-${stream.id}`}
          onClick={onCancel}
        >
          Cancel
        </button>
      </div>
    </div>
  );
}

/** WSS-FR-QSAF: which worktree holds the work the update will not write over. */
function dirtySentence(
  dirty: UpdateDirtyRefusal,
  baseBranch: string,
  streamName: string,
): string {
  if (dirty.code === STREAM_ERRORS.baseNotCheckedOut) {
    return `The update was refused: no worktree holds ${baseBranch}. Check it out and try again.`;
  }
  const where =
    dirty.code === STREAM_ERRORS.baseDirty
      ? `the worktree holding ${baseBranch}`
      : `the working copy of ${streamName}`;
  return `The update was refused: these paths in ${where} are not committed.`;
}

export { namedPaths };
