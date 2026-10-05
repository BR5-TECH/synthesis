/**
 * What a stream's row says about a merge
 * (`../../../specifications/ui/WSS-work-stream-selector.md` WSS-FR-HGWL,
 * WSS-FR-OFCU, WSS-FR-RJTN, WSS-FR-GBWE, WSS-FR-KMHD, WSS-FR-PLVE,
 * WSS-FR-AWRS, WSS-FR-DTYB).
 *
 * Three things stand here, and each comes from a different place:
 *
 * - the merge **call** this surface is waiting on, held for the length of the
 *   call alone;
 * - the **answer** of a call that settled, kept until the dropdown closes;
 * - the stream's **merge run**, read from the listing, which is the only
 *   durable status a merge has. It is the same in every window and after a
 *   relaunch, because it is rendered from the listing and from nothing this
 *   surface attempted.
 *
 * The row acts on no merge run. The one control it offers is **Open in
 * Runs…**, where a merge run is read, answered, continued and discarded
 * (WSS-FR-NRCQ).
 */

import type { StreamMergeRunLink, WorkStreamSummary } from "../../types";
import { shortCommit } from "../../state/graduation";
import { namedPaths, type MergeOutcome, type RunningMerge } from "./rows";

/** WSS-FR-DTYB: a merge run's state, in one word set per state. */
export function mergeRunWord(state: StreamMergeRunLink["state"]): string {
  switch (state) {
    case "queued":
      return "waiting";
    case "working":
      return "reconciling";
    case "reviewing":
      return "being reviewed";
    case "blocked":
      return "blocked";
    case "interrupted":
      return "interrupted";
    case "awaiting_author":
      return "waiting on the author";
    case "failed":
      return "failed";
    case "completed":
      return "merged";
    case "discarded":
      return "discarded";
  }
}

export interface MergeStatusProps {
  summary: WorkStreamSummary;
  /** WSS-FR-HGWL: the merge call this surface is waiting on, if one is. */
  call: RunningMerge | null;
  /** WSS-FR-KMHD / WSS-FR-PLVE: what the last settled call answered. */
  outcome: MergeOutcome | null;
  /** WSS-FR-AWRS: open a merge run in the Runs panel. */
  onOpenRun: (runId: string) => void;
}

export function MergeStatus({ summary, call, outcome, onOpenRun }: MergeStatusProps) {
  const { stream } = summary;
  const run = summary.mergeRun ?? null;
  return (
    <>
      {/* WSS-FR-HGWL: the call is out, so the row says so, and carries the busy
          status for a reader who cannot see it. The row offers no cancel. */}
      {call && (
        <p
          className="badge badge--accent"
          role="status"
          aria-busy="true"
          data-testid={`stream-merging-${stream.id}`}
        >
          Merging {call.streamName}
        </p>
      )}

      {/* WSS-FR-KMHD / WSS-FR-PLVE: the answer to a call that settled. */}
      {!call && outcome && <OutcomeLine summary={summary} outcome={outcome} />}

      {/* WSS-FR-OFCU / WSS-FR-DTYB: the merge run, from the listing alone. */}
      {run && (
        <>
          <p
            className={
              run.state === "failed"
                ? "badge badge--danger"
                : run.state === "awaiting_author"
                  ? "badge badge--warn"
                  : "badge badge--accent"
            }
            role="status"
            data-testid={`stream-merge-run-${stream.id}`}
          >
            {run.name}: {mergeRunWord(run.state)}
          </p>
          {run.state === "awaiting_author" && (
            <p className="stream-select__meta t-muted">
              {run.name} waits on you. The answer or the decision is made in Runs.
            </p>
          )}
          {run.state === "failed" && (
            <p className="stream-select__meta t-muted">
              {run.name} failed. Neither branch was written. The cause is in
              Runs.
            </p>
          )}
          <div className="stream-select__row-actions">
            <button
              type="button"
              className="btn btn--ghost btn--sm"
              data-testid={`stream-merge-open-runs-${stream.id}`}
              onClick={() => onOpenRun(run.runId)}
            >
              Open in Runs…
            </button>
          </div>
        </>
      )}
    </>
  );
}

/** One settled call, as a sentence the row carries. */
function OutcomeLine({
  summary,
  outcome,
}: {
  summary: WorkStreamSummary;
  outcome: MergeOutcome;
}) {
  const { stream } = summary;
  const testId = `stream-merge-result-${stream.id}`;
  switch (outcome.kind) {
    case "nothing_to_merge":
      // WSS-FR-KMHD: nothing landed, and nothing was written.
      return (
        <p className="stream-select__meta" role="status" data-testid={testId}>
          {stream.name} holds nothing {stream.baseBranch} does not. Nothing was
          written.
        </p>
      );
    case "merged": {
      const count = outcome.mergedPaths.length;
      return (
        <p className="stream-select__meta" role="status" data-testid={testId}>
          Merged {stream.name}: {count} {count === 1 ? "path" : "paths"} into{" "}
          {stream.baseBranch},{" "}
          {outcome.commit ? (
            <>
              committed as{" "}
              <code title={outcome.commit} aria-label={`Commit ${outcome.commit}`}>
                {shortCommit(outcome.commit)}
              </code>
            </>
          ) : (
            "left uncommitted in the base worktree"
          )}
          . No run was made.
        </p>
      );
    }
    case "conflicted": {
      // WSS-FR-PLVE: Git could not settle the merge. The status of the run the
      // conflict was handed to is the listing's, shown beside this.
      const count = outcome.conflictedPaths.length;
      return (
        <>
          <p className="stream-select__meta" role="status" data-testid={testId}>
            Git could not settle {count} {count === 1 ? "path" : "paths"}.
            Neither branch was written.
          </p>
          {count > 0 && (
            <p
              className="stream-select__meta t-muted"
              title={outcome.conflictedPaths.join(", ")}
              aria-label={`Paths Git could not settle: ${outcome.conflictedPaths.join(", ")}`}
            >
              {namedPaths(outcome.conflictedPaths)}
            </p>
          )}
        </>
      );
    }
  }
}
