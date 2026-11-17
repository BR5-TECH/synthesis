/**
 * MPV-FR-CTTA .. MPV-FR-IZLJ: the review sheet over the Delivery canvas: a
 * header, the list of rows, the detail of the selected row, and the review
 * bar. Its host is the positioned box of the canvas area, so the Discussion
 * column and its composer stay usable while it shows.
 */
import { useEffect, useRef, useSyncExternalStore, type KeyboardEvent } from "react";

import type { MapPlanProposal } from "../../types";
import {
  rowId,
  rowStatuses,
  rowTitle,
  summaryOf,
  type RowStatus,
} from "../../state/mapPlan/derive";
import type { MapPlanReviewStore } from "../../state/mapPlan/reviewStore";
import { Icon } from "../icons";
import { Detail } from "./Detail";
import type { MapPlanHost } from "./host";
import { OpBadge, RowKind, RowRef, StatusDot } from "./parts";
import { issueCount } from "./words";

/** MPV-FR-ENVX / MPV-FR-XAOG: the list of rows. An entry takes Enter or Space. */
function NavList({
  proposal,
  statuses,
  selectedKey,
  host,
  onSelect,
}: {
  proposal: MapPlanProposal;
  statuses: readonly RowStatus[];
  selectedKey: string | null;
  host: MapPlanHost | null;
  onSelect: (key: string) => void;
}) {
  const keyDown = (e: KeyboardEvent<HTMLDivElement>, at: number) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      onSelect(proposal.rows[at].key);
      return;
    }
    // The list is a listbox, so the arrow keys move between its options.
    const step = e.key === "ArrowDown" ? 1 : e.key === "ArrowUp" ? -1 : 0;
    const next = at + step;
    if (step === 0 || next < 0 || next >= proposal.rows.length) return;
    e.preventDefault();
    onSelect(proposal.rows[next].key);
    (e.currentTarget.parentElement?.children[next] as HTMLElement | undefined)?.focus();
  };
  return (
    <div className="pp-list" role="listbox" aria-label="Proposed changes">
      {proposal.rows.map((row, i) => (
        <div
          key={row.key}
          className="pp-nav"
          role="option"
          tabIndex={0}
          aria-selected={row.key === selectedKey}
          data-sel={row.key === selectedKey}
          data-testid="pp-nav"
          onClick={() => onSelect(row.key)}
          onKeyDown={(e) => keyDown(e, i)}
        >
          <div className="pp-nav__top">
            <OpBadge row={row} />
            <RowKind row={row} host={host} />
            <RowRef row={row} />
            <StatusDot status={statuses[i]} />
          </div>
          <div className="pp-nav__title">{rowTitle(row)}</div>
        </div>
      ))}
    </div>
  );
}

/** MPV-FR-MTSP .. MPV-FR-IZLJ: the review bar. */
function ReviewBar({
  proposal,
  statuses,
  selectedKey,
  onSelect,
  onAll,
}: {
  proposal: MapPlanProposal;
  statuses: readonly RowStatus[];
  selectedKey: string | null;
  onSelect: (key: string) => void;
  onAll: (action: "accept" | "reject") => void;
}) {
  const n = proposal.rows.length;
  const at = Math.max(0, proposal.rows.findIndex((r) => r.key === selectedKey));
  if (!statuses.includes("pending") && !statuses.includes("applying")) {
    const count = (s: RowStatus) => statuses.filter((x) => x === s).length;
    const failed = count("failed");
    return (
      <div className="pp-bar pp-bar--done" data-testid="pp-bar">
        <span>
          All rows decided: <b>{count("applied")}</b> applied, <b>{count("rejected")}</b> rejected
          {failed > 0 && (
            <>
              , <b>{failed}</b> failed
            </>
          )}
        </span>
      </div>
    );
  }
  const busy = statuses.includes("applying");
  return (
    <div className="pp-bar" data-testid="pp-bar">
      <span>
        reviewing change <b>{at + 1}</b> of {n}
      </span>
      <button
        type="button"
        className="btn btn--ghost btn--sm"
        disabled={at <= 0}
        onClick={() => onSelect(proposal.rows[at - 1].key)}
      >
        Previous
      </button>
      <button
        type="button"
        className="btn btn--ghost btn--sm"
        disabled={at >= n - 1}
        onClick={() => onSelect(proposal.rows[at + 1].key)}
      >
        Next
      </button>
      <span className="pp-bar__spacer" />
      <button type="button" className="btn btn--ghost btn--sm" disabled={busy} onClick={() => onAll("reject")}>
        Reject all
      </button>
      <button type="button" className="btn btn--primary btn--sm" disabled={busy} onClick={() => onAll("accept")}>
        Accept all
      </button>
    </div>
  );
}

export function ReviewSheet({ store, host }: { store: MapPlanReviewStore; host: MapPlanHost | null }) {
  useSyncExternalStore(store.subscribe, store.getVersion);
  const snap = store.snapshot();
  const review = snap.review;
  const sheet = useRef<HTMLDivElement>(null);
  const proposal = review ? store.proposalOf(review.discussionId, review.proposalId) : null;

  // MPV-FR-PBAD: the sheet takes the focus when it opens.
  const openedFor = review?.proposalId ?? null;
  useEffect(() => {
    if (openedFor !== null) sheet.current?.focus({ preventScroll: true });
  }, [openedFor]);

  // MPV-FR-XAOG: a decision removes the control that had the focus. Where the
  // focus fell to the page, it goes back to the sheet, so Escape still closes
  // it. A focus the author moved elsewhere stays where it is.
  const refocus = useRef(false);
  useEffect(() => {
    if (!refocus.current) return;
    const active = document.activeElement;
    const el = sheet.current;
    if (!el || el.contains(active)) return;
    if (active === null || active === document.body) el.focus({ preventScroll: true });
    else refocus.current = false;
  });

  if (!review || !proposal) return null;
  const statuses = rowStatuses(proposal, snap.busy);
  const at = proposal.rows.findIndex((r) => r.key === review.selectedKey);
  const index = at >= 0 ? at : 0;
  const row = proposal.rows[index];
  const { discussionId } = review;
  const keepFocus = () => {
    refocus.current = true;
  };
  const decide = (action: "accept" | "reject" | "reopen") => {
    keepFocus();
    // MPV-FR-CAQL: only a decision the backend answered moves the selection.
    void store.decide(discussionId, proposal.id, row.key, action).then((answered) => {
      if (answered && action !== "reopen") store.advanceFrom(row.key);
      keepFocus();
    });
  };
  const reprepareId = rowId(proposal.id, "");

  return (
    <div
      ref={sheet}
      className="pp-sheet"
      role="dialog"
      aria-modal="false"
      aria-label="Plan proposal review"
      tabIndex={-1}
      data-testid="pp-sheet"
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          e.stopPropagation();
          store.closeReview(true);
        }
      }}
    >
      <div className="pp-sheet__head">
        <span className="pp-sheet__icon" aria-hidden="true">
          <Icon.Layers size={14} />
        </span>
        <span className="pp-sheet__title">Plan proposal</span>
        <span className="pp-sheet__meta">{issueCount(proposal)}</span>
        <span className="pp-sheet__sum">{summaryOf(proposal)}</span>
        <button
          type="button"
          className="btn btn--ghost btn--icon btn--sm"
          aria-label="Close review"
          onClick={() => store.closeReview(true)}
        >
          <Icon.X size={14} />
        </button>
      </div>
      <div className="pp-sheet__body">
        <NavList
          proposal={proposal}
          statuses={statuses}
          selectedKey={row.key}
          host={host}
          onSelect={store.select}
        />
        <Detail
          key={row.key}
          proposal={proposal}
          row={row}
          status={statuses[index]}
          refusal={snap.refusals.get(rowId(proposal.id, row.key)) ?? null}
          reprepareRefusal={snap.refusals.get(reprepareId) ?? null}
          reprepareBusy={snap.busy.has(reprepareId)}
          proposer={snap.proposers.get(proposal.id) ?? null}
          host={host}
          actions={{
            accept: () => decide("accept"),
            reject: () => decide("reject"),
            reopen: () => decide("reopen"),
            reprepare: () => void store.requestReprepare(discussionId, proposal.id),
            discuss: () => host?.discuss(row.kind === "modify_issue" ? row.fields.number : null),
          }}
        />
      </div>
      <ReviewBar
        proposal={proposal}
        statuses={statuses}
        selectedKey={row.key}
        onSelect={store.select}
        onAll={(action) => {
          void store.decideAll(discussionId, proposal.id, action).then(() => {
            store.advanceFrom(row.key);
            keepFocus();
          });
        }}
      />
    </div>
  );
}
