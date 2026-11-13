/**
 * What a merge is, before one is asked for
 * (`../../../specifications/ui/WSS-work-stream-selector.md` WSS-FR-YPDA,
 * WSS-FR-VMNV, WSS-FR-OMAP).
 *
 * Merge takes the stream into its base branch. The confirmation says what it
 * would land, says that Git merges first, says where a merge Git cannot settle
 * goes, and takes the choice between leaving the result uncommitted and
 * committing it.
 */

import { useEffect, useState } from "react";

import * as api from "../../api";
import { logWarn } from "../../logging";
import type { StreamMergePublication, WorkStreamSummary } from "../../types";
import { useInlineWriteGuard } from "../MainBranchWrite";

/**
 * WSS-FR-VMNV: whether OpenSpec mode is active, read when the confirmation
 * opens. A read that fails counts as not active, so the confirmation says
 * nothing about an archive it cannot know of.
 */
function useOpenSpecActive(enabled: boolean): boolean {
  const [active, setActive] = useState(false);
  useEffect(() => {
    if (!enabled) return;
    let live = true;
    api
      .getOpenspecStatus()
      .then((status) => {
        if (live) setActive(status?.active === true);
      })
      .catch((e) => {
        logWarn(["frontend"], "merge confirmation could not read the openspec status", {
          reason: e instanceof Error ? e.message : String(e),
        });
      });
    return () => {
      live = false;
    };
  }, [enabled]);
  return active;
}

export function MergeConfirm({
  summary,
  dirty,
  onCancel,
  onStart,
  onStartWithMessage,
  onOpenChanges,
  onCreatePullRequest,
}: {
  summary: WorkStreamSummary;
  /** WSS-FR-OMAP: the complete path set a dirty side was refused with. */
  dirty: string[] | null;
  onCancel: () => void;
  onStart: (
    summary: WorkStreamSummary,
    publication: StreamMergePublication,
  ) => void;
  /** WSS-FR-TQBN: the route that takes a message before it starts. */
  onStartWithMessage: (summary: WorkStreamSummary) => void;
  onOpenChanges: () => void;
  /** WSS-FR-DBGA: a protected base branch points to **Create a PR** of the same row. */
  onCreatePullRequest?: () => void;
}) {
  const { stream, aheadOfBase } = summary;
  // WSS-FR-DBGA: the guard of the base branch, read when the confirmation opens.
  const writeGuard = useInlineWriteGuard({
    write: "merge_stream",
    branch: stream.baseBranch,
    enabled: !dirty,
    onCancel,
    protectedAction: onCreatePullRequest
      ? { label: "Create a PR", run: onCreatePullRequest }
      : undefined,
  });
  const openspec = useOpenSpecActive(!dirty);

  if (dirty) {
    return (
      <div className="stream-select__confirm" role="group">
        <p role="alert">The merge was refused: these paths are not committed.</p>
        <ul className="stream-select__paths">
          {dirty.map((path) => (
            <li key={path}>{path}</li>
          ))}
        </ul>
        <div className="modal__actions">
          <button className="btn btn--ghost btn--sm" onClick={onCancel}>
            Close
          </button>
          <button className="btn btn--default btn--sm" onClick={onOpenChanges}>
            Open Changes
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="stream-select__confirm" role="group">
      <p>
        Merge {aheadOfBase} commit{aheadOfBase === 1 ? "" : "s"} from{" "}
        {stream.name} into {stream.baseBranch}.
      </p>
      {/* WSS-FR-VMNV: said plainly, before the act rather than after it. */}
      <p className="t-muted" data-testid="stream-merge-explainer">
        Git merges first.
        {openspec &&
          " Before that, the OpenSpec changes this stream built are archived on it."}{" "}
        A merge Git settles completes at once and makes no run. A merge Git
        cannot settle is handed to a merge run named “Merge {stream.name}” in
        the Runs panel, where an agent reconciles it and you control it. The
        base branch changes only after a review of the reconciled result has
        judged it ready.
      </p>
      {writeGuard.section}
      {/* WSS-FR-TQBN: neither action waits for the merge. The confirmation
          closes as the merge starts, and the row carries it from there. */}
      <div className="modal__actions">
        <button className="btn btn--ghost btn--sm" onClick={onCancel}>
          Cancel
        </button>
        <button
          className="btn btn--default btn--sm"
          disabled={writeGuard.blocked}
          aria-describedby={writeGuard.describedBy}
          onClick={() => onStart(summary, { kind: "uncommitted" })}
        >
          Leave uncommitted
        </button>
        <button
          className="btn btn--primary btn--sm"
          disabled={writeGuard.blocked}
          aria-describedby={writeGuard.describedBy}
          onClick={() => onStartWithMessage(summary)}
        >
          Commit…
        </button>
      </div>
    </div>
  );
}
