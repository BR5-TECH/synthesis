/**
 * `MIV-map-issue-view.md` MIV-FR-EDKQ .. MIV-FR-TZKJ: what the issue view of a
 * GitHub plan reads of one issue: its labels, its milestone, and its
 * sub-issues, through `"read github plan issue (number)"`.
 */
import { useEffect, useState } from "react";

import * as api from "../../api";
import { errorLogCode } from "../../errorLogCode";
import { logInfo, logWarn } from "../../logging";
import type { FlowItem } from "../../state/flowDocument";
import type { GithubPlanIssueRead, ItemDeliveryState, TrackerFacts } from "../../types";
import type { IssueViewContext } from "../Flow";
import { BoardStatus, KindChip } from "../Flow/boardItems";
import { SideField } from "../Flow/issueView/IssueSidebar";

/** The issue whose detail the open issue view reads. */
export interface DetailTarget {
  itemId: string;
  number: number;
  updatedAt: string;
}

/** The detail read of the open issue view. */
export type IssueDetail =
  | { itemId: string; status: "loading" }
  | { itemId: string; status: "ready"; read: GithubPlanIssueRead }
  | { itemId: string; status: "failed" };

/**
 * MIV-FR-EDKQ / MIV-FR-DFVC / MIV-FR-BCOP: read the detail once when the view
 * opens, and once more each time `updatedAt` changes. An answer for an issue
 * the view no longer shows is discarded.
 */
export function useGithubIssueDetail(target: DetailTarget | null): IssueDetail | null {
  const [detail, setDetail] = useState<IssueDetail | null>(null);
  const itemId = target?.itemId ?? null;
  const number = target?.number ?? null;
  const updatedAt = target?.updatedAt ?? null;
  useEffect(() => {
    if (itemId === null || number === null) {
      setDetail(null);
      return;
    }
    let live = true;
    setDetail({ itemId, status: "loading" });
    api
      .readGithubPlanIssue(number)
      .then((read) => {
        if (!live) return;
        logInfo(["frontend"], "issue detail read", {
          stale: read.stale,
          code: read.error?.code ?? "",
        });
        setDetail({ itemId, status: "ready", read });
      })
      .catch((e: unknown) => {
        // MIV-FR-IHRB: the record holds the error code alone.
        logWarn(["frontend"], "issue detail could not be read", { code: errorLogCode(e) });
        if (live) setDetail({ itemId, status: "failed" });
      });
    return () => {
      live = false;
    };
  }, [itemId, number, updatedAt]);
  return detail !== null && detail.itemId === itemId ? detail : null;
}

const NOT_READ = "Could not be read";

/** MIV-FR-HSRA / MIV-FR-BCOP: the **Labels** and **Milestone** fields. */
export function GithubIssueDetails({ detail }: { detail: IssueDetail | null }) {
  let labels: React.ReactNode;
  let milestone: React.ReactNode;
  if (detail === null || detail.status === "loading") {
    labels = milestone = <span className="miv-none">Loading…</span>;
  } else if (detail.status === "failed") {
    labels = milestone = <span className="miv-none">{NOT_READ}</span>;
  } else {
    const { issue, stale } = detail.read;
    labels = stale ? (
      <span className="miv-none">Labels could not be read</span>
    ) : issue.labels.length === 0 ? (
      <span className="miv-none">None</span>
    ) : (
      <span className="miv-labels">
        {issue.labels.map((label) => (
          <span key={label} className="badge miv-label">
            {label}
          </span>
        ))}
      </span>
    );
    milestone = issue.milestoneTitle ? (
      <span className="miv-milestone">{issue.milestoneTitle}</span>
    ) : (
      <span className="miv-none">None</span>
    );
  }
  return (
    <>
      <SideField label="Labels" testId="miv-field-labels">
        {labels}
      </SideField>
      <SideField label="Milestone" testId="miv-field-milestone">
        {milestone}
      </SideField>
    </>
  );
}

/** A plan item the sub-issue list can name. */
export interface PlanIssue {
  item: FlowItem;
  tracker: TrackerFacts;
  state?: ItemDeliveryState;
}

/**
 * MIV-FR-ZQAX: the URL of issue `number` of the repository that `issueUrl`
 * names. The issue number of `issueUrl` is replaced; a URL with no issue
 * number gives the issue path under the repository `owner/name` of its host.
 */
export function siblingIssueUrl(issueUrl: string, number: number, repository: string): string {
  const swapped = issueUrl.replace(/\/issues\/\d+(?=[/?#]|$)/, `/issues/${number}`);
  if (swapped !== issueUrl) return swapped.replace(/[?#].*$/, "");
  let origin = "https://github.com";
  try {
    origin = new URL(issueUrl).origin;
  } catch {
    // Not a URL: the public host is the fallback.
  }
  return `${origin}/${repository}/issues/${number}`;
}

/**
 * MIV-FR-ZQAX / MIV-FR-TZKJ: the sub-issues of the answer. A sub-issue of the
 * plan opens its own issue view; another one is a tracker link.
 */
export function SubIssues({
  detail,
  issueUrl,
  repository,
  planIssue,
  context,
}: {
  detail: IssueDetail | null;
  issueUrl: string;
  repository: string;
  planIssue: (number: number) => PlanIssue | undefined;
  context: IssueViewContext;
}) {
  const label = (count?: number) => (
    <h3 className="miv-section__label">
      Sub-issues{count !== undefined && <span className="miv-count">{count}</span>}
    </h3>
  );
  if (detail === null || detail.status === "loading") {
    return (
      <section className="miv-section miv-subs" aria-label="Sub-issues" data-testid="miv-sub-issues">
        {label()}
        <span className="miv-none">Loading…</span>
      </section>
    );
  }
  if (detail.status === "failed") {
    return (
      <section className="miv-section miv-subs" aria-label="Sub-issues" data-testid="miv-sub-issues">
        {label()}
        <span className="miv-none">{NOT_READ}</span>
      </section>
    );
  }
  const numbers = detail.read.issue.subIssues;
  if (numbers.length === 0) return null;
  return (
    <section className="miv-section miv-subs" aria-label="Sub-issues" data-testid="miv-sub-issues">
      {label(numbers.length)}
      <ul className="miv-rows">
        {numbers.map((n) => {
          const known = planIssue(n);
          if (!known) {
            return (
              <li key={n}>
                <button
                  type="button"
                  className="miv-row miv-row--link"
                  aria-label={`Open ${repository}#${n} on GitHub`}
                  onClick={() => context.openTracker(siblingIssueUrl(issueUrl, n, repository))}
                >
                  <span className="miv-row__ref">#{n}</span>
                  <span className="miv-row__name miv-none">Not in the plan</span>
                </button>
              </li>
            );
          }
          const facts = {
            state: known.state,
            blockers: [],
            overlap: null,
            tracker: { repository, facts: known.tracker },
            pending: false,
            references: [],
          };
          return (
            <li key={n}>
              <button
                type="button"
                className="miv-row"
                aria-label={`Open #${n} ${known.item.name}`}
                onClick={() => context.showItem(known.item.id)}
              >
                <KindChip kind={known.item.kind} />
                <span className="miv-row__ref">#{n}</span>
                <span className="miv-row__name">{known.item.name || "Unnamed"}</span>
                <span className="miv-row__status">
                  <BoardStatus facts={facts} />
                </span>
              </button>
            </li>
          );
        })}
      </ul>
    </section>
  );
}
