/**
 * One stream's row in the selector's listing (WSS-FR-SOAS).
 *
 * Everything the row says about a stream comes from what the backend reported:
 * how far ahead and behind its base it stands, what its merge run link and its
 * update record hold, and which of its actions those permit (WSS-FR-JBYF).
 *
 * A row offers **Merge stream**, **Create a PR**, **Update stream** and
 * **Delete stream** in that order, centered in the row (WSS-FR-TKMB,
 * WSS-FR-KHGP). Merge takes the stream into
 * its base branch; Update brings the base branch into the stream. Each is
 * enabled only where it has something to do and nothing is already reconciling
 * this stream (WSS-FR-XRHT).
 */

import type { StreamMergePublication, WorkStreamSummary } from "../../types";
import { canUpdateStream, mergeRunBlocksStream } from "../../types";
import { updateStateSentence } from "../StreamUpdateResolution";
import { DeleteConfirm } from "./DeleteConfirm";
import { MergeConfirm } from "./MergeConfirm";
import { MergeStatus } from "./MergeStatus";
import {
  namedPaths,
  type MergeOutcome,
  type RowSurface,
  type RunningMerge,
  type RunningUpdate,
} from "./rows";
import { UpdateWindow, type UpdateDirtyRefusal } from "./UpdateWindow";

export interface StreamRowProps {
  summary: WorkStreamSummary;
  surface: RowSurface;
  busy: boolean;
  error: string | null;
  /** WSS-FR-PSXK: the name of the draft the run holding the stream works on. */
  busyDraftName: string | null;
  /** WSS-FR-HGWL: the merge call this surface is waiting on, if one is out. */
  merge: RunningMerge | null;
  /** WSS-FR-KMHD / WSS-FR-PLVE: what the last settled merge call answered. */
  mergeOutcome: MergeOutcome | null;
  /** WSS-FR-BDMU: the update running in this stream, if one is. */
  update: RunningUpdate | null;
  /** WSS-FR-OMAP: the uncommitted paths a merge of this stream was refused for. */
  dirty: string[] | null;
  /** WSS-FR-QSAF: which worktree an update of this stream was refused for. */
  updateDirty: UpdateDirtyRefusal | null;
  /** WSS-FR-CJYE: set where the update was refused for a revision that moved. */
  updateStale: string | null;
  onActivate: () => void;
  onOpenSurface: (next: RowSurface) => void;
  onRefused: (text: string) => void;
  onSettled: () => void;
  onStartMerge: (
    summary: WorkStreamSummary,
    publication: StreamMergePublication,
  ) => void;
  onStartMergeWithMessage: (summary: WorkStreamSummary) => void;
  /** WSS-FR-WPGR: start the update with the strategy the author chose. */
  onStartUpdate: (
    summary: WorkStreamSummary,
    strategy: "merge_source" | "rebase_source",
  ) => void;
  onCancelUpdate: (streamId: string) => void;
  /** WSS-FR-AWRS: open the stream's merge run in the Runs panel. */
  onOpenRun: (runId: string) => void;
  /** WSS-FR-GTQL: open the resolution window against the update record. */
  onResolveUpdate: () => void;
  onOpenChanges: () => void;
  /** WSS-FR-KHGP: open the Create a PR window for this stream. */
  onCreatePullRequest: (summary: WorkStreamSummary) => void;
}

export function StreamRow({
  summary,
  surface,
  busy,
  error,
  busyDraftName,
  merge,
  mergeOutcome,
  update,
  dirty,
  updateDirty,
  updateStale,
  onActivate,
  onOpenSurface,
  onRefused,
  onSettled,
  onStartMerge,
  onStartMergeWithMessage,
  onStartUpdate,
  onCancelUpdate,
  onOpenRun,
  onResolveUpdate,
  onOpenChanges,
  onCreatePullRequest,
}: StreamRowProps) {
  const { stream, aheadOfBase, behindBase, queuedRunCount } = summary;
  const held = Boolean(stream.busyRunId);
  const missing = stream.isMissing;
  // WSS-FR-PSXK / WSS-FR-OQYG: both disable the row's acts, and each says why
  // in words rather than by a colour or an absent control.
  const blocked = held || missing;
  // WSS-FR-OFCU / WSS-FR-FVKO: what the row renders about a merge run and about
  // an update comes from the listing, so work another window started, or that
  // settled while this dropdown was closed, renders here too.
  const updateRecord = summary.update ?? null;
  const merging = Boolean(merge);
  const updating = Boolean(update) || updateRecord?.state === "running";
  // WSS-FR-XRHT: one reconciliation of a stream is startable at a time. A merge
  // run in any state but `completed` and `failed` holds that place.
  const mergeRunHolds = mergeRunBlocksStream(summary.mergeRun);
  const running = merging || updating || mergeRunHolds;
  // WSS-FR-GTQL: and so does an update that stopped.
  const updateNeedsAuthor =
    !running &&
    (updateRecord?.state === "escalated" ||
      updateRecord?.state === "conflicted" ||
      updateRecord?.state === "cancelled" ||
      updateRecord?.state === "failed");
  // WSS-FR-HZVQ: Update is enabled only where the stream stands behind its base
  // and no run is queued on it or holds it.
  const updatable = canUpdateStream(summary) && !running;
  // WSS-FR-YPDA: and Merge only where the stream stands ahead of its base.
  const mergeable = aheadOfBase > 0 && !running;
  // WSS-FR-EPCH: Create a PR needs a commit the base branch lacks, and no run,
  // merge call or update holding the stream, and a stream that is not missing.
  // A busy, missing or updating row keeps the button, disabled, beside its status
  // (WSS-FR-PSXK, WSS-FR-OQYG, WSS-FR-BDMU).
  const proposable = aheadOfBase > 0 && !running && !blocked;
  const createPrReason = proposable
    ? undefined
    : createPullRequestDisabledReason(summary, {
        missing,
        held,
        merging,
        updating,
      });
  const createPrButton = (
    <button
      type="button"
      className="btn btn--ghost btn--sm stream-select__act"
      disabled={!proposable}
      aria-disabled={!proposable}
      aria-describedby={
        proposable ? undefined : `stream-create-pr-reason-${stream.id}`
      }
      data-testid={`stream-create-pr-${stream.id}`}
      title={createPrReason}
      onClick={() => onCreatePullRequest(summary)}
    >
      Create a PR
    </button>
  );
  // WSS-FR-EPCH: where the button stands, its reason stands in words beside it.
  const createPrReasonLine = !proposable && (
    <p
      id={`stream-create-pr-reason-${stream.id}`}
      className="stream-select__meta t-muted"
      data-testid={`stream-create-pr-reason-${stream.id}`}
    >
      {createPrReason}
    </p>
  );
  const idleActions =
    !updating && !updateNeedsAuthor && !blocked && surface.kind === "none";
  // WSS-FR-NRCQ: the row acts on no merge run. Its one control for a merge is
  // Open in Runs…, which the status renders.

  return (
    <div className="stream-select__row" data-testid={`stream-row-${stream.id}`}>
      <button
        type="button"
        role="menuitem"
        className="stream-select__open"
        disabled={blocked || busy || running}
        aria-label={`Open the work stream ${stream.name}`}
        onClick={onActivate}
      >
        <span className="stream-select__name">{stream.name}</span>
        <span className="stream-select__branch" title={stream.branch}>
          {stream.branch}
        </span>
        {/* WSS-FR-SOAS: ahead, behind and queued, all from the listing. */}
        <span className="stream-select__meta t-muted">
          {aheadOfBase} ahead of {stream.baseBranch} · {behindBase} behind ·{" "}
          {queuedRunCount} queued
        </span>
      </button>

      {/* WSS-FR-PSXK: the busy line replaces the idle actions. Only Create a PR
          stays, disabled, so the row says in words why it is refused. */}
      {held && (
        <p className="badge badge--warn" role="status">
          Busy —{" "}
          {summary.mergeRun && summary.mergeRun.runId === stream.busyRunId
            ? summary.mergeRun.name
            : busyDraftName
              ? `“${busyDraftName}”`
              : "a run"}{" "}
          is working in it
        </p>
      )}
      {missing && (
        <p className="badge badge--warn" role="status">
          Missing — its working copy is gone. Delete it and make it again.
        </p>
      )}
      {blocked && !updating && (
        <>
          <div className="stream-select__row-actions">{createPrButton}</div>
          {createPrReasonLine}
        </>
      )}

      {/* WSS-FR-HGWL / WSS-FR-KMHD / WSS-FR-PLVE / WSS-FR-OFCU: the merge call,
          its answer, and the stream's merge run. None of them acts on a run. */}
      <MergeStatus
        summary={summary}
        call={merge}
        outcome={mergeOutcome}
        onOpenRun={onOpenRun}
      />

      {/* WSS-FR-BDMU: an updating stream says so, names the strategy, says which
          turn it is on, and offers the one action that stops it. */}
      {updating && (
        <>
          <p
            className="badge badge--accent"
            role="status"
            data-testid={`stream-update-running-${stream.id}`}
          >
            Updating{" "}
            {strategyWord(update?.progress?.strategy ?? updateRecord?.strategy)}
            {update?.progress
              ? ` — turn ${update.progress.turn} of ${update.progress.turnsMax}`
              : " — reconciling with the source branch"}
          </p>
          {update?.progress && update.progress.reconcilingPaths.length > 0 && (
            <p
              className="stream-select__meta t-muted"
              title={update.progress.reconcilingPaths.join(", ")}
            >
              {update.progress.reconcilingPaths.join(", ")}
            </p>
          )}
          <div className="stream-select__row-actions">
            {createPrButton}
            <button
              type="button"
              className="btn btn--ghost btn--sm"
              data-testid={`stream-update-cancel-${stream.id}`}
              onClick={() => onCancelUpdate(stream.id)}
            >
              Cancel update
            </button>
          </div>
          {createPrReasonLine}
        </>
      )}

      {/* WSS-FR-GTQL: an update that stopped. Its actions reach
          the update record's own operations alone (WSS-FR-PMYA). */}
      {updateNeedsAuthor && updateRecord && (
        <>
          <p
            className={
              updateRecord.state === "escalated"
                ? "badge badge--warn"
                : "badge badge--danger"
            }
            role="status"
            data-testid={`stream-update-state-${stream.id}`}
          >
            {updateRecord.state === "escalated" ? "Asks you" : "Update stopped"}
          </p>
          <p className="stream-select__meta t-muted">
            {updateStateSentence(updateRecord)}
          </p>
          {updateRecord.conflicts.length > 0 && (
            <p
              className="stream-select__meta t-muted"
              title={updateRecord.conflicts.map((c) => c.path).join(", ")}
            >
              {namedPaths(updateRecord.conflicts.map((c) => c.path))}
            </p>
          )}
          <div className="stream-select__row-actions">
            <button
              type="button"
              className="btn btn--ghost btn--sm"
              data-testid={`stream-update-open-${stream.id}`}
              onClick={onResolveUpdate}
            >
              {updateRecord.state === "escalated" ? "Answer…" : "Review update…"}
            </button>
          </div>
        </>
      )}

      {/* WSS-FR-TKMB / WSS-FR-KHGP: Merge, then Create a PR, then Update, then
          Delete. A control the stream has
          no work for stays in place and disabled, so the actions of two rows
          stand in one column. */}
      {idleActions && (
        <div className="stream-select__row-actions stream-select__row-actions--idle">
          <button
            type="button"
            className="btn btn--ghost btn--sm stream-select__act"
            disabled={!mergeable}
            data-testid={`stream-merge-${stream.id}`}
            title={mergeDisabledReason(summary, merging, mergeable)}
            onClick={() => onOpenSurface({ kind: "merge", streamId: stream.id })}
          >
            Merge stream
          </button>
          {createPrButton}
          <button
            type="button"
            className="btn btn--ghost btn--sm stream-select__act"
            disabled={!updatable}
            data-testid={`stream-update-${stream.id}`}
            title={updateDisabledReason(summary, running)}
            onClick={() => onOpenSurface({ kind: "update", streamId: stream.id })}
          >
            Update stream
          </button>
          <button
            type="button"
            className="btn btn--ghost btn--sm stream-select__act"
            // WSS-FR-HGWL / WSS-FR-XRHT: a stream whose merge call is out, or
            // whose merge run holds it, is not one to delete.
            disabled={merging || mergeRunHolds}
            data-testid={`stream-delete-${stream.id}`}
            onClick={() => onOpenSurface({ kind: "delete", streamId: stream.id })}
          >
            Delete stream
          </button>
        </div>
      )}

      {idleActions && createPrReasonLine}

      {!merge && surface.kind === "merge" && surface.streamId === stream.id && (
        <MergeConfirm
          summary={summary}
          dirty={dirty}
          onCancel={() => onOpenSurface({ kind: "none" })}
          onStart={onStartMerge}
          onStartWithMessage={onStartMergeWithMessage}
          onOpenChanges={onOpenChanges}
        />
      )}
      {!update && surface.kind === "update" && surface.streamId === stream.id && (
        <UpdateWindow
          summary={summary}
          dirty={updateDirty}
          stale={updateStale}
          onCancel={() => onOpenSurface({ kind: "none" })}
          onStart={onStartUpdate}
          onOpenChanges={onOpenChanges}
        />
      )}
      {surface.kind === "delete" && surface.streamId === stream.id && (
        <DeleteConfirm
          summary={summary}
          onCancel={() => onOpenSurface({ kind: "none" })}
          onRefused={onRefused}
          onSettled={onSettled}
          onMergeInstead={() =>
            onOpenSurface({ kind: "merge", streamId: stream.id })
          }
        />
      )}

      {error && (
        <p className="stream-select__error" role="alert">
          {error}
        </p>
      )}
    </div>
  );
}

/** WSS-FR-BDMU: which direction the running update reconciles in, in words. */
function strategyWord(strategy: string | undefined): string {
  return strategy === "rebase_source"
    ? "by rebase"
    : "by merging the source";
}

/**
 * WSS-FR-HZVQ: why **Update stream** is disabled, in words rather than by a state the
 * author has to infer from a greyed control.
 */
function updateDisabledReason(
  summary: WorkStreamSummary,
  running: boolean,
): string | undefined {
  if (running) return "This stream is already merging or updating.";
  if (summary.stream.busyRunId) return "A run is working in this stream.";
  if (summary.queuedRunCount > 0) return "Runs are queued on this stream.";
  if (summary.behindBase === 0)
    return `This stream is up to date with ${summary.stream.baseBranch}.`;
  return undefined;
}

/**
 * WSS-FR-XRHT / WSS-FR-YPDA: why **Merge stream** is disabled, in words.
 *
 * A merge call that is out and a merge run that holds the stream each say so,
 * and a stream that stands ahead of its base by nothing says that.
 */
function mergeDisabledReason(
  summary: WorkStreamSummary,
  calling: boolean,
  mergeable: boolean,
): string | undefined {
  if (mergeable) return undefined;
  if (calling) return `Merging ${summary.stream.name}.`;
  if (mergeRunBlocksStream(summary.mergeRun)) {
    return `${summary.mergeRun!.name} holds this stream's merge. Open it in Runs.`;
  }
  if (summary.stream.busyRunId) return "A run is working in this stream.";
  return "This stream holds nothing its base branch does not.";
}

/**
 * WSS-FR-EPCH: why **Create a PR** is disabled, in words: the stream is missing,
 * a run, a merge call, an update or a merge run holds it, or it holds no commit
 * its base branch lacks (it was merged or never advanced).
 */
function createPullRequestDisabledReason(
  summary: WorkStreamSummary,
  state: {
    missing: boolean;
    held: boolean;
    merging: boolean;
    updating: boolean;
  },
): string {
  if (state.missing) return `${summary.stream.name} is missing.`;
  if (state.held) return "A run is working in this stream.";
  if (state.merging) return `Merging ${summary.stream.name}.`;
  if (state.updating) return `Updating ${summary.stream.name}.`;
  if (mergeRunBlocksStream(summary.mergeRun)) {
    return `${summary.mergeRun!.name} holds this stream. Open it in Runs.`;
  }
  return `Nothing to propose: this stream holds no commit that ${summary.stream.baseBranch} lacks.`;
}
