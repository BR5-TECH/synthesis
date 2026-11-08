// The typed refusals of the prompt proposal operations (PCP-prompt-change-proposals).
// Re-exported from `./comments`.

/** PCP contract surface: the typed refusals its operations can return. */
export const PROMPT_PROPOSAL_ERRORS = {
  noProjectOpen: "no_project_open",
  notFound: "proposal_not_found",
  /** PCP-FR-19: the target no longer names a file the project holds. */
  artifactNotFound: "artifact_not_found",
  /** PCP-FR-19: the target's resolved artifact type is no longer `prompt`. */
  notAPrompt: "not_a_prompt_artifact",
  alreadyDecided: "already_decided",
  /** PCP-FR-EDTW: the stored changes moved on since this surface read them. */
  candidateStale: "candidate_stale",
  discussionResolved: "discussion_resolved",
  /**
   * PCP-FR-14: a failure at or before the commit point. **The file has not
   * changed** — the backend never wrote it on any failing path — and the
   * proposal is still the author's to decide (PCR-FR-15).
   */
  writeFailed: "write_failed",
  /**
   * PCP-FR-14: the acceptance **landed** and its decision comment is still
   * owed. Not a refusal, and never rendered as one (PCR-FR-15).
   */
  acceptanceIncomplete: "acceptance_incomplete",
  /** PCP-FR-14: another acceptance of this proposal is in flight. */
  acceptanceInProgress: "acceptance_in_progress",
  /** PCP-FR-ZQAH: `hunkId` names no change of this proposal. */
  hunkNotFound: "hunk_not_found",
  /** PCP-FR-ZQAH: the change has already been accepted or rejected. */
  hunkAlreadyDecided: "hunk_already_decided",
  /** PCP-FR-ZQAH: the text this change names is no longer in the file. */
  anchorLost: "anchor_lost",
  /** PCP-FR-18: the agent withdrew the change (PCR-FR-QHZN, DCR-FR-NYDA). */
  hunkWithdrawn: "hunk_withdrawn",
} as const;
