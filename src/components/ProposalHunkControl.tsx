/**
 * CMT-FR-48: a `proposal_hunk` reference renders as a **control** naming one
 * change of a draft change proposal (CMS-FR-TQVB).
 *
 * The opening comment of a hunk discussion carries one (DCP-FR-FHWN,
 * DCR-FR-VBQN). Activating it scrolls the draft's document column to that
 * change. The state comes from `DCP-draft-change-proposals.md` by way of the
 * store, never from the attachment, which carries the identity alone
 * (CMT-FR-67). It fetches nothing of its own (CMT-FR-49).
 *
 * DCR-FR-17: once the proposal closes, the change has no decoration left to
 * scroll to. The control then only says how the change was decided, and the
 * discussion stays readable around it.
 */
import { Icon } from "./icons";
import { hunkPosition, hunkStateOf, useProposalReference } from "../state/draftProposals";
import { isShownInDocument, showHunkInDocument } from "./DraftDiscussion/hunkDiscussion";
import { isUndecided, type HunkState, type ProposalHunkReference } from "../types";

/** The one word the control says about where the change stands. */
export function hunkStateWord(state: HunkState): string {
  if (state === "accepted") return "accepted";
  if (state === "rejected") return "rejected";
  return isUndecided(state) ? "undecided" : state;
}

export function ProposalHunkControl({
  attachment,
}: {
  attachment: ProposalHunkReference;
}) {
  // CTA-FR-LOOE: asking for the reading it needs is part of rendering it.
  const reference = useProposalReference(attachment.draftId, attachment.proposalId);
  const proposal = reference.kind === "known" ? reference.proposal : null;
  const state = proposal ? hunkStateOf(proposal, attachment.hunkId) : undefined;
  const position = proposal ? hunkPosition(proposal, attachment.hunkId) : 0;
  const total = proposal?.hunkCount ?? 0;
  // DCR-FR-17: only an open proposal is decorated, so only then is there
  // something in the document to go to.
  const reachable = isShownInDocument(proposal, state);
  const word =
    state !== undefined
      ? hunkStateWord(state)
      : reference.kind === "gone"
        ? "no longer available"
        : null;
  const name = position > 0 ? `Change ${position} of ${total}` : attachment.path;

  return (
    <button
      type="button"
      className="comment-attach__proposal"
      data-testid="comment-attachment-proposal-hunk"
      data-state={state ?? (reference.kind === "gone" ? "missing" : "unresolved")}
      disabled={!reachable}
      title={word === null ? attachment.path : `${name} — ${word} — ${attachment.path}`}
      // CMT-FR-48: the route to one change is a scroll of the document column
      // to it, by way of the review's own focused change.
      onClick={() => showHunkInDocument(attachment)}
    >
      <Icon.Diff size={11} />
      <span className="comment-attach__chip-name">{name}</span>
      {word !== null && <span className="comment-attach__chip-type">{word}</span>}
    </button>
  );
}
