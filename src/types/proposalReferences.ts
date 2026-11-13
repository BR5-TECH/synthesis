// The proposal references a comment can carry (CMS-comments-storage.md
// CMS-FR-42, CMS-FR-QLMV, CMS-FR-TQVB, CMS-FR-YQAX, CMS-FR-BSQB).
// Re-exported from `./comments`.

/**
 * CMS-FR-BSQB: how many hunks one update of a proposal changed, added, and
 * withdrawn. The values are as the update appended them.
 */
export interface ProposalUpdateCounts {
  changed: number;
  added: number;
  withdrawn: number;
}

/**
 * CMS-FR-QLMV: a reference to the edits an agent proposed to an OpenSpec
 * change on a work stream (`CEP-change-edit-proposals.md`). Like the
 * `proposal` and `promptProposal` references it carries the proposal's
 * identity and nothing else, and no composer can produce one.
 */
export interface ChangeProposalReference {
  kind: "changeProposal";
  proposalId: string;
  streamId: string;
  change: string;
  /** CMS-FR-BSQB: present only on the comment an update appended. */
  update?: ProposalUpdateCounts;
}

/** CMS-FR-42: the `proposal_hunk` attachment, as stored and as supplied. */
export interface ProposalHunkReference {
  kind: "proposalHunk";
  proposalId: string;
  hunkId: string;
  /**
   * CMS-FR-TQVB: the draft whose proposal holds the change. Exactly one of
   * `draftId`, `artifactId`, and `change` is present.
   */
  draftId?: string;
  /**
   * CMS-FR-TQVB / CMS-FR-AHDQ: the prompt or DOC file whose proposal holds the
   * change (`PCP-prompt-change-proposals.md`).
   */
  artifactId?: string;
  /**
   * CMS-FR-YQAX: the OpenSpec change whose change edit proposal holds the
   * hunk.
   */
  change?: { streamId: string; change: string };
  /**
   * Draft-relative path, project-relative path for an artifact, or
   * change-relative path for a change (CMS-FR-YQAX).
   */
  path: string;
}
