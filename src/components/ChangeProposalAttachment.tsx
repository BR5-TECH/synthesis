/**
 * CMT-FR-48 / OCW-FR-COLR: a `change_proposal` reference renders as a
 * **control** on the terms of its `prompt_proposal` sibling. It names the
 * change, says how much the proposal changes, and opens or focuses the change
 * workspace tab on its stream at the first undecided hunk.
 *
 * CMT-FR-67: the state comes from `CEP-change-edit-proposals.md` by way of the
 * store, never from the attachment, which carries the identity alone.
 */
import { Icon } from "./icons";
import { ProposalCount, proposalControlLabel } from "./CommentAttachments";
import { reviewedHunkTotal } from "../state/draftProposals";
import {
  asReviewable,
  showChangeProposal,
  useChangeProposalReference,
} from "../state/openspec/changeProposals";
import { undecidedCount, type Attachment } from "../types";

export function ChangeProposalAttachmentView({
  attachment,
}: {
  attachment: Extract<Attachment, { kind: "changeProposal" }>;
}) {
  const reference = useChangeProposalReference(
    attachment.streamId,
    attachment.change,
    attachment.proposalId,
  );
  const proposal = reference.kind === "known" ? reference.proposal : null;
  const state = proposal ? proposal.state : null;
  const label = proposalControlLabel(state, reference.kind === "gone");
  const changes = proposal ? reviewedHunkTotal(asReviewable(proposal)) : 0;
  const left = undecidedCount(proposal?.counts);

  return (
    <button
      type="button"
      className="comment-attach__proposal"
      data-testid="comment-attachment-change-proposal"
      data-state={state ?? (reference.kind === "gone" ? "missing" : "unresolved")}
      disabled={state === null}
      title={label === null ? attachment.change : `${label} — ${attachment.change}`}
      onClick={() =>
        showChangeProposal(attachment.streamId, attachment.change, attachment.proposalId)
      }
    >
      <Icon.Diff size={11} />
      <span className="comment-attach__chip-name">{attachment.change}</span>
      <ProposalCount update={attachment.update} changes={changes} />
      {label !== null && <span className="comment-attach__chip-type">{label}</span>}
      {left > 0 && (
        <span className="comment-attach__proposal-route">Review in the change</span>
      )}
    </button>
  );
}
