import { useEffect, useRef, useState } from "react";
import * as api from "../../api";
import { logDebug, logInfo, logWarn } from "../../logging";
import type { BranchDeletionPlan, RemoteDeletionState } from "../../types";
import { Overlay } from "./Overlay";
import {
  GIT_ERRORS,
  parseRejection,
  rejectionMessage,
  rejectionPaths,
} from "./errors";
import { summarizePaths } from "./format";
import { withGithubToken } from "./githubToken";

/** What a finished deletion reports back to the Branches section (GIT-FR-UDKY). */
export interface DeletionResult {
  branch: string;
  removedWorktreePath?: string;
  /** Set when the branch was removed with its work stream. */
  streamName?: string;
  remote: {
    state: RemoteDeletionState;
    branch?: string;
    /** The typed cause of a failed remote deletion, in words. */
    error?: string;
  };
}

function PathWarning({ paths }: { paths: string[] }) {
  const { listed, remaining } = summarizePaths(paths);
  return (
    <div
      className="git-delete__warning"
      role="alert"
      data-testid="git-delete-dirty-warning"
    >
      <strong>
        Warning: confirming discards {paths.length} uncommitted{" "}
        {paths.length === 1 ? "path" : "paths"}.
      </strong>
      <ul className="git-delete__paths">
        {listed.map((p) => (
          <li key={p}>{p}</li>
        ))}
      </ul>
      {remaining > 0 && (
        <div className="t-meta">and {remaining} more not listed</div>
      )}
    </div>
  );
}

/**
 * GIT-FR-QYWP, GIT-FR-GAMV, GIT-FR-SIJB, GIT-FR-CTHN, GIT-FR-XOLE: the
 * confirmation of a branch deletion.
 *
 * Nothing is removed before **Delete** is confirmed. Cancel, Escape and the
 * backdrop close it and change nothing. A branch of a work stream is removed
 * through the work stream operations and nothing else, with no remote choice.
 */
export function BranchDeleteDialog({
  plan,
  onClose,
  onDeleted,
  onRequestGithubToken,
  restoreFocus,
}: {
  plan: BranchDeletionPlan;
  onClose: () => void;
  onDeleted: (result: DeletionResult) => void;
  onRequestGithubToken?: () => Promise<boolean>;
  restoreFocus: () => HTMLElement | null;
}) {
  const stream = plan.stream;
  const [paths, setPaths] = useState<string[]>(plan.uncommittedPaths ?? []);
  const [pathsLoading, setPathsLoading] = useState(stream !== undefined);
  const [deleteRemote, setDeleteRemote] = useState(false);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // GIT-FR-XOLE: the commits the stream holds that its base branch does not.
  // A warning, not a refusal: the author may delete them anyway, which passes
  // `force`. A `stream_unmerged` refusal raises it when the count grew after
  // the plan was read.
  const [unmerged, setUnmerged] = useState(stream?.aheadOfBase ?? 0);
  const cancelRef = useRef<HTMLButtonElement | null>(null);

  useEffect(() => {
    logDebug(["frontend"], "branch deletion dialog shown", {
      stream: stream !== undefined,
      paths: plan.uncommittedPaths?.length ?? 0,
      unmerged: stream?.aheadOfBase ?? 0,
    });
    // Logged once, when the dialog opens.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // GIT-FR-XOLE: the discarded paths of a work stream are named from the work
  // stream's own read. The plan's set stands in when that read fails.
  useEffect(() => {
    if (!stream) return;
    let alive = true;
    api
      .getWorkStreamUncommittedPaths(stream.streamId)
      .then((read) => {
        if (alive && Array.isArray(read)) setPaths(read);
      })
      .catch((e) => {
        logWarn(["frontend", "backend"], "work stream path read failed", {
          code: parseRejection(e).code,
        });
      })
      .finally(() => {
        if (alive) setPathsLoading(false);
      });
    return () => {
      alive = false;
    };
  }, [stream]);

  const confirm = async () => {
    const discard = paths.length > 0;
    setRunning(true);
    setError(null);
    const force = stream !== undefined && unmerged > 0;
    logInfo(["frontend", "backend"], "branch deletion confirmed", {
      stream: stream !== undefined,
      deleteRemote: stream ? false : deleteRemote,
      discard,
      force,
    });
    try {
      if (stream) {
        await api.deleteWorkStream(stream.streamId, force, discard);
        onDeleted({
          branch: plan.branch,
          removedWorktreePath: plan.worktree?.path,
          streamName: stream.streamName,
          remote: { state: "not_requested" },
        });
        return;
      }
      const outcome = await withGithubToken(
        () => api.deleteBranch(plan.branch, deleteRemote, discard),
        onRequestGithubToken,
      );
      const remote = outcome.remote ?? { requested: false, state: "not_requested" };
      onDeleted({
        branch: outcome.branch ?? plan.branch,
        removedWorktreePath: outcome.removedWorktreePath,
        remote: {
          state: remote.state,
          branch: remote.branch,
          error: remote.error ? rejectionMessage(remote.error) : undefined,
        },
      });
    } catch (e) {
      const rejection = parseRejection(e);
      const dirty =
        rejection.code === (stream ? GIT_ERRORS.streamDirty : GIT_ERRORS.worktreeDirty);
      logWarn(["frontend", "backend"], "branch deletion refused", {
        code: rejection.code,
      });
      const fresh = dirty ? rejectionPaths(rejection) : [];
      const ahead = Number(rejection.detail);
      if (
        stream &&
        rejection.code === GIT_ERRORS.streamUnmerged &&
        Number.isInteger(ahead) &&
        ahead > 0
      ) {
        // GIT-FR-XOLE: the stream gained commits after the plan was read. The
        // warning shows the new count, and a second confirmation forces it.
        setUnmerged(ahead);
      } else if (dirty && fresh.length > 0) {
        // GIT-FR-SIJB: the worktree changed after the plan was read. The same
        // warning shows with the new paths, and a second confirmation is needed.
        setPaths(fresh);
      } else {
        setError(rejectionMessage(e, { branch: plan.branch }));
      }
    } finally {
      setRunning(false);
    }
  };

  const worktreePath = plan.worktree?.path;
  const discarding = paths.length > 0;
  const forcing = stream !== undefined && unmerged > 0;

  return (
    <Overlay
      title={`Delete branch ${plan.branch}`}
      titleId="git-branch-delete-title"
      testId="git-branch-delete"
      onClose={onClose}
      dismissible={!running}
      initialFocus={() => cancelRef.current}
      restoreFocus={restoreFocus}
      actions={
        <>
          <button
            type="button"
            ref={cancelRef}
            className="btn btn--default btn--sm"
            disabled={running}
            onClick={onClose}
          >
            Cancel
          </button>
          <button
            type="button"
            className="btn btn--danger btn--sm"
            disabled={running || pathsLoading}
            aria-busy={running || undefined}
            onClick={() => void confirm()}
          >
            {running
              ? "Deleting…"
              : forcing
                ? discarding
                  ? "Delete anyway and discard changes"
                  : "Delete anyway"
                : discarding
                  ? "Delete and discard changes"
                  : stream
                    ? "Delete work stream"
                    : "Delete branch"}
          </button>
        </>
      }
    >
      {stream && (
        <div
          className="git-delete__warning"
          role="note"
          data-testid="git-delete-stream-warning"
        >
          <strong>
            Branch {plan.branch} belongs to the work stream {stream.streamName}.
          </strong>{" "}
          Deleting it deletes the work stream, its worktree and its branch.
        </div>
      )}
      {forcing && (
        <div
          className="git-delete__warning"
          role="alert"
          data-testid="git-delete-unmerged-warning"
        >
          <strong>
            Warning: this work stream holds {unmerged}{" "}
            {unmerged === 1 ? "commit" : "commits"} that its base branch does
            not hold.
          </strong>{" "}
          Deleting it anyway loses {unmerged === 1 ? "it" : "them"}. To keep{" "}
          {unmerged === 1 ? "it" : "them"}, merge the stream from the Streams
          window first.
        </div>
      )}
      <p className="git-delete__effect" data-testid="git-delete-effect">
        {stream
          ? `This removes the work stream ${stream.streamName}, its branch ${plan.branch}`
          : `This removes the local branch ${plan.branch}`}
        {worktreePath ? (
          <>
            {" and its worktree "}
            <code>{worktreePath}</code>
            {"."}
          </>
        ) : (
          <>{". It has no linked worktree."}</>
        )}{" "}
        Nothing is removed until you confirm.
      </p>
      {pathsLoading && (
        <div className="git__state" role="status">
          Reading uncommitted paths…
        </div>
      )}
      {discarding && <PathWarning paths={paths} />}
      {plan.remoteBranch && !stream && (
        <label className="git-delete__remote">
          <input
            type="checkbox"
            checked={deleteRemote}
            disabled={running}
            onChange={(e) => setDeleteRemote(e.target.checked)}
          />
          <span>Also delete the remote branch {plan.remoteBranch}</span>
        </label>
      )}
      {error && (
        <div
          className="git__state git__state--error"
          role="alert"
          data-testid="git-delete-error"
        >
          {error}
        </div>
      )}
    </Overlay>
  );
}
