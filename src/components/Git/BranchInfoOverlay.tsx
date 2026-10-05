import { useEffect } from "react";
import * as api from "../../api";
import { logDebug, logWarn } from "../../logging";
import type { BranchInformation, CommitSummary } from "../../types";
import { Overlay } from "./Overlay";
import { parseRejection, rejectionMessage } from "./errors";
import { formatCommitInstant } from "./format";
import { RegionNote } from "./parts";
import { useLoad } from "./useLoad";

function CommitEntry({ commit }: { commit: CommitSummary }) {
  return (
    <li className="git-info__commit" data-testid="git-info-commit">
      <div className="git-info__commit-head">
        <span className="t-hash">{commit.shortId}</span>
        <span className="git-info__commit-who">
          {commit.authorName} &lt;{commit.authorEmail}&gt;
        </span>
        <span className="t-meta">{formatCommitInstant(commit.authoredAt)}</span>
      </div>
      <div className="git-info__message">{commit.message || commit.subject}</div>
    </li>
  );
}

function Details({ info }: { info: BranchInformation }) {
  return (
    <>
      <dl className="git-info__facts">
        <dt>Name</dt>
        <dd data-testid="git-info-name">{info.name}</dd>
        <dt>Type</dt>
        <dd>{info.kind === "local" ? "Local branch" : "Remote branch"}</dd>
        <dt>Upstream</dt>
        <dd>{info.upstream ?? "None"}</dd>
        <dt>Checked out</dt>
        <dd>
          {info.worktree
            ? `${info.isCurrent ? "Current branch. " : ""}Worktree ${info.worktree.name} at ${info.worktree.path}${
                info.worktree.isPrimary ? " (primary worktree)" : ""
              }`
            : info.isCurrent
              ? "Current branch"
              : "Not checked out in any worktree"}
        </dd>
        <dt>Work stream</dt>
        <dd>
          {info.stream ? `Belongs to work stream ${info.stream.streamName}` : "None"}
        </dd>
        <dt>Tip commit</dt>
        <dd data-testid="git-info-tip">
          <span className="t-hash">{info.tip.shortId}</span> {info.tip.subject}
        </dd>
      </dl>
      <div className="git__section-head">Commits</div>
      {info.commits.length === 0 ? (
        <RegionNote kind="empty">This branch holds no commits.</RegionNote>
      ) : (
        <ol className="git-info__commits" aria-label="Commits of the branch">
          {info.commits.map((c) => (
            <CommitEntry key={c.id} commit={c} />
          ))}
        </ol>
      )}
    </>
  );
}

/**
 * GIT-FR-NUCX: what one branch is and holds, read through
 * `get branch information`. It shows loading and failure in words and closes on
 * Escape, the close control and the backdrop (GIT-FR-VCDG).
 */
export function BranchInfoOverlay({
  name,
  kind,
  onClose,
  restoreFocus,
}: {
  name: string;
  kind: "local" | "remote";
  onClose: () => void;
  restoreFocus: () => HTMLElement | null;
}) {
  const { state, run } = useLoad<BranchInformation>();

  const load = () => {
    logDebug(["frontend", "backend"], "branch information read started", { kind });
    void run(async () => {
      try {
        return await api.getBranchInformation(name, kind);
      } catch (e) {
        logWarn(["frontend", "backend"], "branch information read failed", {
          kind,
          code: parseRejection(e).code,
        });
        throw e;
      }
    });
  };
  useEffect(load, [name, kind]); // eslint-disable-line react-hooks/exhaustive-deps

  return (
    <Overlay
      title={`Branch ${name}`}
      titleId="git-branch-info-title"
      testId="git-branch-info"
      onClose={onClose}
      restoreFocus={restoreFocus}
    >
      {state.status === "loading" && (
        <RegionNote kind="loading">Loading branch information…</RegionNote>
      )}
      {state.status === "error" && (
        <RegionNote kind="error" onRetry={load}>
          {rejectionMessage(state.error, { branch: name })}
        </RegionNote>
      )}
      {state.status === "ready" && state.data && <Details info={state.data} />}
    </Overlay>
  );
}
