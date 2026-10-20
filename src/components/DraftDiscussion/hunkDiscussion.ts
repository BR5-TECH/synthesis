/**
 * What a discussion says about the one change it is about
 * (`../../../specifications/ui/DCR-draft-change-review.md` DCR-FR-VBQN;
 * `DDS-draft-discussion.md` DDS-FR-WKTQ).
 *
 * The opening comment's `proposal_hunk` attachment is the whole of the link
 * between a change and its discussion (DCP-FR-FHWN). The discussion column
 * reads it to name the change above the conversation, and the workspace reads it
 * to find the discussion a change already has.
 */
import { logDebug } from "../../logging";
import { openReview } from "../../state/draftProposals";
import { revealHunk } from "../../state/draftDiscussion";
import type {
  Discussion,
  DraftChangeProposal,
  HunkState,
  ProposalHunkReference,
} from "../../types";

/**
 * DCR-FR-VBQN: the discussion of one change, if it has one.
 *
 * A hunk discussion is one whose **opening** comment names the change. A later
 * comment that names it is a reply in some other conversation, not the
 * conversation about the change.
 */
export function hunkDiscussionOf(
  discussions: readonly Discussion[],
  proposalId: string,
  hunkId: string,
): Discussion | undefined {
  return discussions.find((d) =>
    (d.comments[0]?.attachments ?? []).some(
      (a) => a.kind === "proposalHunk" && a.proposalId === proposalId && a.hunkId === hunkId,
    ),
  );
}

/** The `proposal_hunk` the opening comment of a discussion carries, if any. */
export function openingHunkOf(discussion: Discussion): ProposalHunkReference | undefined {
  const found = (discussion.comments[0]?.attachments ?? []).find(
    (a) => a.kind === "proposalHunk",
  );
  return found?.kind === "proposalHunk" ? found : undefined;
}

/**
 * CMT-FR-48 / DDS-FR-PBXN: whether the document holds a change to go to.
 *
 * DCR-FR-17: only an open proposal decorates the document, and DCR-FR-07: an
 * accepted change carries no decoration, so the review does not visit it
 * (DCR-FR-QPRV).
 */
export function isShownInDocument(
  proposal: DraftChangeProposal | null,
  state: HunkState | undefined,
): boolean {
  return proposal?.state === "pending" && state !== undefined && state !== "accepted";
}

/**
 * CMT-FR-48 / DDS-FR-PBXN: scroll the draft's document column to one change,
 * by way of the review's own focused change.
 */
export function showHunkInDocument(reference: ProposalHunkReference): void {
  logDebug(["frontend"], "a change was asked to be shown in the document", {
    draftId: reference.draftId,
    proposalId: reference.proposalId,
    hunkId: reference.hunkId,
  });
  revealHunk(reference.draftId, reference.hunkId);
  openReview(reference.proposalId);
}
