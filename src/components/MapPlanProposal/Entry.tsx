/**
 * MPV-FR-IBSF .. MPV-FR-FNIS: the summary row of a plan proposal in a Map
 * discussion. It is the only part of the proposal the discussion shows; the
 * review opens as a sheet over the Delivery canvas.
 */
import { useCallback, useEffect, useSyncExternalStore } from "react";

import { formatRelative } from "../ProjectPicker";
import {
  decidedCount,
  proposalStatus,
  rowStatuses,
  summaryOf,
} from "../../state/mapPlan/derive";
import { useMapPlan } from "./host";
import { StatusWord } from "./parts";

export function MapPlanEntry({
  discussionId,
  proposalId,
  proposer,
}: {
  discussionId: string;
  proposalId: string;
  /** MPV-FR-AAFF: the handle of the agent whose comment carries the proposal. */
  proposer?: string;
}) {
  const { store, host } = useMapPlan();
  useSyncExternalStore(store.subscribe, store.getVersion);
  const snap = store.snapshot();

  useEffect(() => store.want(discussionId), [store, discussionId]);
  useEffect(() => {
    if (proposer) store.noteProposer(proposalId, proposer);
  }, [store, proposalId, proposer]);
  const buttonRef = useCallback(
    (el: HTMLButtonElement | null) => store.registerReturn(proposalId, el),
    [store, proposalId],
  );

  const list = snap.proposals.get(discussionId);
  if (list === null || list === undefined) {
    return (
      <p className="pp-entry pp-entry--note" role="status">
        {snap.unavailable.has(discussionId) ? "The plan proposal could not be read." : "Reading the plan proposal…"}
      </p>
    );
  }
  const proposal = list.find((p) => p.id === proposalId);
  if (!proposal) return null;

  const status = proposalStatus(rowStatuses(proposal, snap.busy));
  const decided = decidedCount(proposal);
  const total = proposal.rows.length;
  // MPV-FR-GYLV: the decided count after the first decision, the time once no row waits.
  const note =
    status === "pending" || status === "applying"
      ? decided > 0
        ? `${decided} of ${total}`
        : null
      : formatRelative(proposal.createdAt);
  const shown = snap.review?.proposalId === proposalId;
  const label = shown ? "Hide review" : status === "pending" || status === "applying" ? "Review" : "Show review";
  const summary = summaryOf(proposal);

  const toggle = () => {
    if (shown) {
      store.closeReview();
      return;
    }
    host?.showDelivery();
    store.openReview(discussionId, proposalId);
  };

  return (
    // The button is the control; a click anywhere on the row does the same for a pointer.
    <div
      className="pp-entry"
      data-testid="map-plan-entry"
      data-status={status}
      onClick={(e) => {
        if (host && e.target === e.currentTarget) toggle();
      }}
    >
      <span className="pp-chg" aria-hidden="true">
        CHG
      </span>
      <span className="pp-entry__name">Plan proposal</span>
      <span className="pp-entry__sum" title={summary}>
        {summary}
      </span>
      <StatusWord status={status} note={note} />
      {host && (
        <button
          ref={buttonRef}
          type="button"
          className="btn btn--default btn--sm"
          aria-expanded={shown}
          aria-label={`${label}: plan proposal, ${summary}, ${status}`}
          onClick={toggle}
          data-testid="map-plan-entry-toggle"
        >
          {label}
        </button>
      )}
    </div>
  );
}
