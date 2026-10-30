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
import { openReview, useProposalReference } from "../../state/draftProposals";
import { revealHunk } from "../../state/draftDiscussion";
import { showPromptHunk, usePromptProposalReference } from "../../state/promptProposals";
import type { HunkSource } from "../../state/proposalHunks";
import type {
  Discussion,
  HunkCounts,
  HunkLedgerRow,
  HunkState,
  ProposalHunkReference,
} from "../../types";

/**
 * DCR-FR-GNRL / CMS-FR-TQVB: what a hunk reference reads of the proposal that
 * holds the change, whichever kind of proposal it is.
 */
export interface ReferencedProposal {
  state: "pending" | "accepted" | "rejected";
  hunkCount: number;
  counts?: HunkCounts;
  ledger?: readonly HunkLedgerRow[] | null;
}

/** DCR-FR-GNRL: where a hunk reference's proposal stands, as one answer. */
export type HunkProposalReference =
  | { readonly kind: "known"; readonly proposal: ReferencedProposal }
  | { readonly kind: "unresolved" }
  | { readonly kind: "gone" };

/** Which store reads the changes of the proposal a reference names. */
export function hunkSourceOf(reference: ProposalHunkReference): HunkSource {
  return reference.artifactId ? "prompt" : "draft";
}

/**
 * CMS-FR-TQVB: the proposal a hunk reference names, read from the draft store
 * or from the prompt store by the reference's own target. Both hooks run, and
 * the one whose target is absent asks for nothing.
 */
export function useHunkProposal(reference: ProposalHunkReference): HunkProposalReference {
  const draft = useProposalReference(
    reference.artifactId ? undefined : reference.draftId,
    reference.proposalId,
  );
  const prompt = usePromptProposalReference(reference.artifactId, reference.proposalId);
  return reference.artifactId ? prompt : draft;
}

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
  proposal: { state: string } | null,
  state: HunkState | undefined,
): boolean {
  return proposal?.state === "pending" && state !== undefined && state !== "accepted";
}

/**
 * CMT-FR-48 / DDS-FR-PBXN: scroll the document column to one change, by way of
 * the review's own focused change.
 *
 * PCR-FR-RFJC: a change of a prompt or DOC proposal is shown in its file's
 * Editor tab, which is opened or focused first.
 */
export function showHunkInDocument(reference: ProposalHunkReference): void {
  if (reference.artifactId) {
    showPromptHunk(reference.artifactId, reference.proposalId, reference.hunkId);
    return;
  }
  if (!reference.draftId) return;
  logDebug(["frontend"], "a change was asked to be shown in the document", {
    draftId: reference.draftId,
    proposalId: reference.proposalId,
    hunkId: reference.hunkId,
  });
  revealHunk(reference.draftId, reference.hunkId);
  openReview(reference.proposalId);
}
