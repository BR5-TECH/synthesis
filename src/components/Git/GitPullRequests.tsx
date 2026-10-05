import { Icon } from "../icons";
import type { PullRequestDetail, PullRequestSummary } from "../../types";
import { CommentMarkdown } from "../CommentMarkdown";
import { rejectionMessage } from "./errors";
import { formatIsoInstant } from "./format";
import { moveRowFocus } from "./GitLog";
import { RegionNote, SelectedMark } from "./parts";
import { TimelineItem } from "./Timeline";
import type { PullRequestsController } from "./usePullRequests";

function StateBadge({ pr }: { pr: Pick<PullRequestSummary, "state" | "isDraft"> }) {
  const cls =
    pr.state === "open"
      ? "badge badge--ok"
      : pr.state === "merged"
        ? "badge badge--accent"
        : "badge";
  return (
    <>
      <span className={cls}>{pr.state}</span>
      {pr.isDraft && <span className="badge badge--warn">draft</span>}
    </>
  );
}

/**
 * GIT-FR-LKRX: a failed read after an earlier success keeps the rows and says
 * they are stale. The error never reads as an empty result.
 */
function StaleError({
  error,
  onRetry,
  testId,
}: {
  error: unknown;
  onRetry: () => void;
  testId: string;
}) {
  return (
    <div
      className="git__state git__state--error"
      role="alert"
      data-testid={testId}
    >
      <span>{rejectionMessage(error)}</span>
      <button type="button" className="btn btn--default btn--sm" onClick={onRetry}>
        Retry
      </button>
    </div>
  );
}

/** GIT-FR-OBZW: the PRs rail below **Create PR for current branch**. */
export function PullRequestRail({ pr }: { pr: PullRequestsController }) {
  const { list } = pr;
  const rows = list.data ?? [];
  const selectedIndex = rows.findIndex((r) => r.number === pr.selected);
  const filters: [("open" | "closed"), string][] = [
    ["open", "Open"],
    ["closed", "Closed"],
  ];
  return (
    <div data-testid="git-prs-rail">
      <div className="git__switch-row">
        <div
          className="git__switch"
          role="radiogroup"
          aria-label="Pull request state"
          onKeyDown={(e) => {
            if (e.key === "ArrowRight" || e.key === "ArrowLeft") {
              e.preventDefault();
              pr.setFilter(pr.filter === "open" ? "closed" : "open");
            }
          }}
        >
          {filters.map(([k, label]) => (
            <button
              key={k}
              type="button"
              role="radio"
              className="btn btn--sm git__switch-option"
              aria-checked={pr.filter === k}
              data-active={pr.filter === k}
              tabIndex={pr.filter === k ? 0 : -1}
              onClick={() => pr.setFilter(k)}
            >
              {label}
            </button>
          ))}
        </div>
        <button
          type="button"
          className="btn btn--ghost btn--icon btn--sm"
          aria-label="Refresh pull requests"
          disabled={list.status === "loading"}
          onClick={pr.refreshList}
        >
          <Icon.Refresh size={12} />
        </button>
      </div>
      {list.status === "loading" && rows.length === 0 && (
        <RegionNote kind="loading" testId="git-prs-loading">
          Loading pull requests…
        </RegionNote>
      )}
      {list.status === "error" && list.stale && (
        <>
          <StaleError error={list.error} onRetry={pr.refreshList} testId="git-prs-error" />
          <div className="git__stale" role="status" data-testid="git-prs-stale">
            <span className="badge badge--warn">stale</span> These pull requests
            are from the last successful read.
          </div>
        </>
      )}
      {list.status === "error" && !list.stale && (
        <RegionNote kind="error" onRetry={pr.refreshList} testId="git-prs-error">
          {rejectionMessage(list.error)}
        </RegionNote>
      )}
      {list.status === "ready" && rows.length === 0 && (
        <RegionNote kind="empty" testId="git-prs-empty">
          {pr.filter === "open"
            ? "There are no open pull requests."
            : "There are no closed pull requests."}
        </RegionNote>
      )}
      {rows.length > 0 && (
        <div
          role="listbox"
          aria-label="Pull requests"
          onKeyDown={(e) => moveRowFocus(e, '[role="option"]')}
        >
          {rows.map((row, i) => {
            const selected = row.number === pr.selected;
            const activate = () => pr.select(row.number);
            return (
              <div
                key={row.number}
                className="git__file git-pr-row"
                role="option"
                aria-selected={selected}
                tabIndex={i === (selectedIndex === -1 ? 0 : selectedIndex) ? 0 : -1}
                data-selected={selected || undefined}
                data-testid="git-pr-row"
                onClick={activate}
                onKeyDown={(e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    activate();
                  }
                }}
              >
                <div className="git-pr-row__top">
                  <span className="t-hash">#{row.number}</span>
                  <span className="git-pr-row__title">{row.title}</span>
                </div>
                <div className="git-pr-row__foot">
                  <span className="t-meta">{row.author}</span>
                  <StateBadge pr={row} />
                  {selected && <SelectedMark />}
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

function Header({ detail }: { detail: PullRequestDetail }) {
  return (
    <header className="git-pr__header" data-testid="git-pr-header">
      <h2 className="git-pr__title">
        {detail.title} <span className="t-hash">#{detail.number}</span>
      </h2>
      <div className="git-pr__meta">
        <StateBadge pr={detail} />
        <span className="git-pr__branches">
          {detail.headBranch} → {detail.baseBranch}
        </span>
        <span className="t-meta">
          {detail.author} · opened {formatIsoInstant(detail.createdAt)}
        </span>
      </div>
      <div className="git__file-path git-pr__link" title={detail.url}>
        {detail.url}
      </div>
      <div className="git-pr__description" data-testid="git-pr-description">
        {detail.body.trim() === "" ? (
          <div className="t-meta">This pull request has no description.</div>
        ) : (
          <CommentMarkdown body={detail.body} className="git-pr__body" />
        )}
      </div>
    </header>
  );
}

/** GIT-FR-FNQA: the large view of the selected pull request. */
export function PullRequestView({ pr }: { pr: PullRequestsController }) {
  if (pr.selected === null) {
    return (
      <>
        <div className="git__right-head">
          <span className="t-eyebrow">PULL REQUEST</span>
        </div>
        <RegionNote kind="empty" testId="git-pr-none">
          Select a pull request to read its description and its timeline.
        </RegionNote>
      </>
    );
  }
  const { detail, timeline } = pr;
  const items = timeline.data?.items ?? [];
  return (
    <>
      <div className="git__right-head">
        <span className="t-eyebrow">PULL REQUEST</span>
        <span className="t-hash">#{pr.selected}</span>
        <span className="spacer" />
        <button
          type="button"
          className="btn btn--ghost btn--icon btn--sm"
          aria-label="Refresh pull request"
          disabled={detail.status === "loading" || timeline.status === "loading"}
          onClick={pr.refreshSelection}
        >
          <Icon.Refresh size={12} />
        </button>
      </div>
      <div className="git-pr__scroll" data-testid="git-pr-scroll">
        {detail.status === "loading" && !detail.data && (
          <RegionNote kind="loading" testId="git-pr-loading">
            Loading pull request…
          </RegionNote>
        )}
        {detail.status === "error" && (
          <>
            {detail.stale ? (
              <StaleError
                error={detail.error}
                onRetry={pr.refreshSelection}
                testId="git-pr-error"
              />
            ) : (
              <RegionNote
                kind="error"
                onRetry={pr.refreshSelection}
                testId="git-pr-error"
              >
                {rejectionMessage(detail.error)}
              </RegionNote>
            )}
            {detail.stale && (
              <div className="git__stale" role="status" data-testid="git-pr-stale">
                <span className="badge badge--warn">stale</span> This is the
                last successful read.
              </div>
            )}
          </>
        )}
        {detail.data && <Header detail={detail.data} />}

        <section className="git-pr__timeline" aria-label="Timeline">
          <div className="git__section-head">Timeline</div>
          {timeline.status === "loading" && !timeline.data && (
            <RegionNote kind="loading" testId="git-pr-timeline-loading">
              Loading timeline…
            </RegionNote>
          )}
          {timeline.status === "error" && (
            <>
              {timeline.stale ? (
                <StaleError
                  error={timeline.error}
                  onRetry={pr.refreshSelection}
                  testId="git-pr-timeline-error"
                />
              ) : (
                <RegionNote
                  kind="error"
                  onRetry={pr.refreshSelection}
                  testId="git-pr-timeline-error"
                >
                  {rejectionMessage(timeline.error)}
                </RegionNote>
              )}
              {timeline.stale && (
                <div className="git__stale" role="status">
                  <span className="badge badge--warn">stale</span> This is the
                  last successful read.
                </div>
              )}
            </>
          )}
          {timeline.data?.truncated && (
            <div
              className="git__stale"
              role="status"
              data-testid="git-pr-truncated"
            >
              This timeline is truncated. GitHub holds more activity than is
              shown here.
            </div>
          )}
          {timeline.status === "ready" && items.length === 0 && (
            <RegionNote kind="empty" testId="git-pr-timeline-empty">
              This pull request has no activity yet.
            </RegionNote>
          )}
          {items.length > 0 && (
            <ol className="git-pr__items">
              {items.map((item) => (
                <TimelineItem key={item.id} item={item} />
              ))}
            </ol>
          )}
        </section>
      </div>
    </>
  );
}
