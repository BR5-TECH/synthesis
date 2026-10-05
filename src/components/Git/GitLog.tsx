import type { KeyboardEvent } from "react";
import { Icon } from "../icons";
import type { CommitSummary } from "../../types";
import { ChangedFilesView } from "./ChangedFilesView";
import { rejectionMessage } from "./errors";
import { formatCommitDate, formatCommitInstant } from "./format";
import { RegionNote, SelectedMark } from "./parts";
import type { GitLogController } from "./useGitLog";

/** Move focus between the rows of a list with the arrow keys. */
export function moveRowFocus(e: KeyboardEvent<HTMLElement>, selector: string) {
  const keys = ["ArrowDown", "ArrowUp", "Home", "End"];
  if (!keys.includes(e.key)) return;
  const rows = Array.from(
    e.currentTarget.querySelectorAll<HTMLElement>(selector),
  );
  if (rows.length === 0) return;
  const at = rows.indexOf(document.activeElement as HTMLElement);
  let next = at;
  if (e.key === "ArrowDown") next = Math.min(rows.length - 1, at + 1);
  else if (e.key === "ArrowUp") next = Math.max(0, at - 1);
  else if (e.key === "Home") next = 0;
  else next = rows.length - 1;
  e.preventDefault();
  rows[next]?.focus();
}

function CommitRow({
  commit,
  context,
  selected,
  tabStop,
  onSelect,
}: {
  commit: CommitSummary;
  /** GIT-FR-MVBZ: the history's branch, shown on every row. */
  context: string;
  selected: boolean;
  tabStop: boolean;
  onSelect: () => void;
}) {
  const descId = `git-commit-desc-${commit.id}`;
  return (
    <div
      className="git__file git-commit"
      role="option"
      aria-selected={selected}
      aria-describedby={descId}
      tabIndex={tabStop ? 0 : -1}
      data-selected={selected || undefined}
      data-testid="git-commit-row"
      onClick={onSelect}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onSelect();
        }
      }}
    >
      <div className="git-commit__top">
        <span className="t-hash">{commit.shortId}</span>
        <span className="git-commit__subject">{commit.subject}</span>
      </div>
      <div className="t-meta git-commit__who">
        {commit.authorName} &lt;{commit.authorEmail}&gt;
      </div>
      <div className="git-commit__foot">
        <time
          className="t-meta"
          dateTime={new Date(commit.authoredAt * 1000).toISOString()}
          title={formatCommitInstant(commit.authoredAt)}
        >
          {formatCommitDate(commit.authoredAt)}
        </time>
        <span
          className="t-meta git-commit__branch"
          data-testid="git-commit-branch"
        >
          {context}
        </span>
        {commit.refs.map((ref) => (
          <span key={ref} className="badge git-commit__ref">
            {ref}
          </span>
        ))}
        {selected && <SelectedMark />}
      </div>
      <span id={descId} className="sr-only">
        {commit.message}
      </span>
    </div>
  );
}

/** GIT-FR-KPTE, GIT-FR-MVBZ, GIT-FR-DXNC: the commit rail. */
export function LogRail({ log }: { log: GitLogController }) {
  const { history } = log;
  const data = history.data;
  const commits = data?.commits ?? [];
  const head = commits.find((c) => c.id === data?.headId);
  const detachedId = data?.headId
    ? (head?.shortId ?? data.headId.slice(0, 7))
    : "";
  const branchContext =
    data?.isDetached || !data?.branch ? "Detached HEAD" : `on ${data.branch}`;
  const selectedIndex = commits.findIndex((c) => c.id === log.commitId);

  return (
    <div data-testid="git-log-rail">
      {data && (
        <div
          className="git__section-head git-log__context"
          data-testid="git-log-context"
        >
          <Icon.Branch size={12} />
          {data.isDetached || !data.branch ? (
            <span>Detached HEAD {detachedId}</span>
          ) : (
            <span className="git__file-name" title={data.branch}>
              {data.branch}
            </span>
          )}
        </div>
      )}
      {history.status === "loading" && (
        <RegionNote kind="loading" testId="git-log-loading">
          Loading commit history…
        </RegionNote>
      )}
      {history.status === "error" && (
        <RegionNote
          kind="error"
          onRetry={log.retryHistory}
          testId="git-log-error"
        >
          {rejectionMessage(history.error)}
        </RegionNote>
      )}
      {history.status === "ready" && commits.length === 0 && (
        <RegionNote kind="empty" testId="git-log-empty">
          This branch has no commits yet.
        </RegionNote>
      )}
      {commits.length > 0 && (
        <div
          role="listbox"
          aria-label="Commits"
          onKeyDown={(e) => moveRowFocus(e, '[role="option"]')}
        >
          {commits.map((c, i) => (
            <CommitRow
              key={c.id}
              commit={c}
              context={branchContext}
              selected={c.id === log.commitId}
              tabStop={i === (selectedIndex === -1 ? 0 : selectedIndex)}
              onSelect={() => log.selectCommit(c.id)}
            />
          ))}
        </div>
      )}
    </div>
  );
}

/**
 * GIT-FR-03, GIT-FR-DXNC, GIT-FR-JRYS, GIT-FR-TFAU: the files and the diff of
 * a commit, side by side (GIT-FR-WMVK).
 */
export function LogView({ log }: { log: GitLogController }) {
  const commit = log.history.data?.commits.find((c) => c.id === log.commitId);
  if (!commit) {
    return (
      <>
        <div className="git__right-head">
          <span className="t-eyebrow">COMMIT</span>
        </div>
        <RegionNote kind="empty" testId="git-log-no-commit">
          Select a commit to see the files it changed.
        </RegionNote>
      </>
    );
  }
  return (
    <>
      <div className="git__right-head">
        <span className="t-eyebrow">COMMIT</span>
        <span className="t-hash">{commit.shortId}</span>
        <span className="git__file-name" title={commit.subject}>
          {commit.subject}
        </span>
      </div>
      <ChangedFilesView
        files={log.files}
        filePath={log.filePath}
        onSelectFile={log.selectFile}
        onRetryFiles={log.retryFiles}
        diff={log.diff}
        onRetryDiff={log.retryDiff}
        diffOwner={commit.shortId}
        emptyFilesText="This commit changed no files."
        testIdPrefix="git-log"
      />
    </>
  );
}
