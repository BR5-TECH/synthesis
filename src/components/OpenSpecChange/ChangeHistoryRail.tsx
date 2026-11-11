/**
 * The History rail of a change on a stream
 * (`../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-RKCC,
 * OCW-FR-QCSL), on the terms of the draft History rail
 * (`../NewArtifactWorkspace/DraftHistoryRail.tsx`).
 *
 * It is a way of reading what the change has been, and nothing else: no
 * control here changes anything. The current change heads the rail, and the
 * versions follow it, newest first.
 */
import { formatRelative } from "../ProjectPicker";
import type { ChangeVersion } from "../../types";
import type { ChangeHistory } from "./useChangeHistory";
import { changedFileCount, shortCommit } from "./versionFiles";

function filesLabel(count: number): string {
  return count === 1 ? "1 file" : `${count} files`;
}

export function ChangeHistoryRail({ history }: { history: ChangeHistory }) {
  const { list, error, reading, refresh, shown, show, back } = history;
  const viewing: ChangeVersion | null = shown?.version ?? null;
  return (
    <div className="draft-rail openspec-history" aria-label="History" data-testid="openspec-history">
      <div className="draft-rail__header t-ui-xs">
        <span className="draft-rail__title">History</span>
        <button
          type="button"
          className="btn btn--ghost btn--sm"
          disabled={reading}
          onClick={refresh}
        >
          Refresh history
        </button>
      </div>
      <p className="draft-rail__legend openspec-history__legend t-ui-xs">
        Each version is a commit of the stream branch that changed a file of this change.
      </p>
      {error !== null && (
        <div className="draft-rail__error t-ui-xs" role="alert">
          <p>{error}</p>
          <button type="button" className="btn btn--sm" onClick={refresh}>
            Try again
          </button>
        </div>
      )}
      <button
        type="button"
        className={
          viewing === null
            ? "draft-version draft-version--live draft-version--reading"
            : "draft-version draft-version--live"
        }
        aria-current={viewing === null ? "true" : undefined}
        onClick={back}
      >
        <span className="draft-version__marker" aria-hidden="true">
          {viewing === null ? "▸" : " "}
        </span>
        <span className="draft-version__body">
          <span className="draft-version__source">Current change</span>
        </span>
      </button>
      {list !== null && list.versions.length === 0 && error === null && (
        <p className="draft-rail__legend t-ui-xs" role="status">
          The stream branch holds no version of this change yet.
        </p>
      )}
      {list !== null && list.versions.length > 0 && (
        <div className="draft-rail__divider t-ui-xs" aria-hidden="true">
          versions
        </div>
      )}
      <div className="draft-rail__versions" data-testid="openspec-history-versions">
        {/* OCW-FR-RKCC: newest first, as the list arrives. */}
        {(list?.versions ?? []).map((version) => {
          const current = viewing?.commitId === version.commitId;
          return (
            <button
              key={version.commitId}
              type="button"
              className={current ? "draft-version draft-version--reading" : "draft-version"}
              aria-current={current ? "true" : undefined}
              data-testid="openspec-version"
              onClick={() => show(version)}
            >
              <span className="draft-version__marker" aria-hidden="true">
                {current ? "▸" : " "}
              </span>
              <span className="draft-version__body">
                <span className="draft-version__source">{version.summary}</span>
                <span className="draft-version__standing">
                  {version.authorName} · {filesLabel(changedFileCount(version))}
                </span>
                <span className="draft-version__at" title={version.committedAt}>
                  {formatRelative(version.committedAt)} · {shortCommit(version.commitId)}
                </span>
              </span>
            </button>
          );
        })}
        {/* OCW-FR-RKCC: a truncated list says so. */}
        {list?.truncated && (
          <p className="draft-rail__legend t-ui-xs" role="note" data-testid="openspec-history-truncated">
            Only the {list.versions.length} newest versions are shown.
          </p>
        )}
      </div>
    </div>
  );
}
