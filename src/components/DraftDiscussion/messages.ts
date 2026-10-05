/**
 * DCR-FR-16 / DCR-FR-27: what the review says when a decision is refused.
 *
 * Words rather than a typed token, because the author is the one who has to
 * decide what to do next — and every one of these says what happened to the
 * draft, which is the thing they most need to know. Nothing here is a colour or
 * a tone: a refusal is legible without colour discrimination (DCR-FR-28).
 */
import { PROPOSAL_ERRORS } from "../../types";
import type { DraftChangeProposal } from "../../types";

/** The proposing agent, as the surface names it. */
export function authorHandle(proposal: DraftChangeProposal): string {
  return proposal.agent.kind === "agent"
    ? `@${proposal.agent.handle}`
    : proposal.agent.login;
}

/** DCR-FR-16: a typed refusal in the terms the author can act on. */
export function decisionMessage(error: string): string {
  // DCP-FR-BMLX: the one refusal an author can do something about without
  // leaving the surface — rejecting still clears the change.
  if (error.includes(PROPOSAL_ERRORS.anchorLost)) {
    return "The text this change alters is no longer in the prompt. Reject it to clear it, or undo your edit to bring it back.";
  }
  if (error.includes(PROPOSAL_ERRORS.hunkAlreadyDecided)) {
    return "This change has already been decided.";
  }
  if (error.includes(PROPOSAL_ERRORS.hunkNotFound)) {
    return "This change is no longer part of the proposal.";
  }
  if (error.includes(PROPOSAL_ERRORS.alreadyDecided)) {
    return "This change has already been decided.";
  }
  if (error.includes(PROPOSAL_ERRORS.pathMissing)) {
    return "That file is no longer in the draft, so this change cannot be applied.";
  }
  if (error.includes(PROPOSAL_ERRORS.writeFailed)) {
    return "The draft file could not be written. Nothing was changed.";
  }
  if (error.includes(PROPOSAL_ERRORS.acceptanceInProgress)) {
    return "Another change to this draft is still being applied. Try again in a moment.";
  }
  if (error.includes(PROPOSAL_ERRORS.historyRecoveryFailed)) {
    return "This draft's History could not be reconciled, so nothing about it can be decided until it is.";
  }
  if (error.includes(PROPOSAL_ERRORS.notFound)) {
    return "This proposed change is no longer available.";
  }
  return error;
}
