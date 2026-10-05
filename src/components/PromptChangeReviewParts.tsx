/**
 * The small parts of the prompt change review modal: the candidate write state,
 * the proposing agent's handle, and the wording of a typed refusal.
 */
import type { CandidateBuffer } from "../state/candidateBuffers";
import { PROMPT_PROPOSAL_ERRORS } from "../types";
import type { PromptChangeProposal } from "../types";

/**
 * PCR-FR-23: what the toggle row's trailing end says about the candidate's
 * write — and only about that.
 *
 * It never says the file changed, because it never can: this write reaches
 * proposal storage and nothing else (PCP-FR-22).
 */
export function CandidateWriteState({ buffer }: { buffer: CandidateBuffer }) {
  const state = buffer.error
    ? "error"
    : buffer.conflict
      ? "conflict"
      : buffer.saving
        ? "saving"
        : buffer.dirty
          ? "dirty"
          : "saved";
  const says =
    buffer.error != null
      ? "Candidate not saved"
      : buffer.conflict != null
        ? "Candidate needs resolving"
        : buffer.saving
          ? "Saving candidate…"
          : buffer.dirty
            ? "Candidate not saved yet"
            : "Candidate saved";
  return (
    <span
      className="diff-toolbar__write-state"
      data-state={state}
      role="status"
      aria-live="polite"
    >
      {says}
    </span>
  );
}

/** The proposing agent, as the head names it. */
export function authorHandle(proposal: PromptChangeProposal): string {
  return proposal.agent.kind === "agent"
    ? `@${proposal.agent.handle}`
    : proposal.agent.login;
}

/**
 * PCR-FR-15: a typed refusal in the terms the author can act on.
 *
 * A `write_failed` says the file has **not** changed, which is true of every one
 * of them: the backend's artifact write is the last fallible step of its
 * transaction and is atomic, so a failure means it did not land (PCP-FR-14).
 * Nothing was restored, because nothing needed restoring.
 */
export function messageFor(error: string): string {
  if (error.includes(PROMPT_PROPOSAL_ERRORS.alreadyDecided)) {
    return "This change has already been decided.";
  }
  if (error.includes(PROMPT_PROPOSAL_ERRORS.artifactNotFound)) {
    return "This file is no longer in the project, so this change cannot be applied.";
  }
  if (error.includes(PROMPT_PROPOSAL_ERRORS.notAPrompt)) {
    return "This file is no longer in the project as a prompt, so this change cannot be applied.";
  }
  if (error.includes(PROMPT_PROPOSAL_ERRORS.discussionLocked)) {
    return "This conversation is locked, so the decision could not be recorded. This file has not changed.";
  }
  if (error.includes(PROMPT_PROPOSAL_ERRORS.acceptanceInProgress)) {
    return "This change is still being applied. Try again in a moment.";
  }
  if (error.includes(PROMPT_PROPOSAL_ERRORS.writeFailed)) {
    return "The change could not be applied. This file has not changed.";
  }
  if (error.includes(PROMPT_PROPOSAL_ERRORS.notFound)) {
    return "This proposed change is no longer available.";
  }
  return error;
}
