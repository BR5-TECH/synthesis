// The split of an OpenSpec change into new changes
// (NCP-new-change-proposals.md, contract surface; CMS-comments-storage.md
// CMS-FR-QYGP, CMS-FR-MTUL).

import type { Participant } from "./comments";

/** NCP contract: `NewChangeProposalState`. */
export type NewChangeState = "pending" | "accepted" | "rejected" | "withdrawn";

/** NCP contract: one proposed new change. */
export interface NewChangeProposal {
  id: string;
  /** NCP-FR-NDHQ: the split this proposal belongs to. */
  setId: string;
  /** The original change, and its stream. */
  streamId: string;
  change: string;
  /** The proposed new change name. */
  name: string;
  /** Change-relative paths of the proposed files. */
  paths: string[];
  position: number;
  total: number;
  agent: Participant;
  rationale: string;
  threadId: string;
  commentId: string;
  state: NewChangeState;
  revision: number;
  /** NCP-FR-KRSA: the name an acceptance created. */
  createdChange?: string | null;
  /** NCP-FR-KRSA: the files are written and their commit failed. */
  commitOwed: boolean;
  createdAt: string;
  decidedAt?: string | null;
}

/** NCP contract: `ChangeSplit`. */
export interface ChangeSplit {
  setId: string;
  /** NCP-FR-MQCS: the accepted change names, oldest first. */
  accepted: string[];
  /** Present once the author confirmed the split. */
  confirmedAt?: string | null;
}

/** NCP contract: what `"list new change proposals"` returns. */
export interface NewChangeProposalList {
  /** Most recently created first. */
  proposals: NewChangeProposal[];
  split: ChangeSplit | null;
}

/** NCP contract: `NewChangeDecisionOutcome`. */
export interface NewChangeDecisionOutcome {
  proposal: NewChangeProposal;
  committed: boolean;
  commentId?: string | null;
  originKind: "change_discussion" | "change_comment";
}

/** NCP-FR-VLOO: why `"new change proposals changed"` was emitted. */
export type NewChangeProposalChangeCause =
  | "recorded"
  | "updated"
  | "decided"
  | "committed"
  | "confirmed";

/** NCP-FR-VLOO: what `"new change proposals changed"` carries. */
export interface NewChangeProposalsChangedPayload {
  streamId: string;
  change: string;
  proposal?: NewChangeProposal | null;
  cause: NewChangeProposalChangeCause;
}

/** NCP contract: the typed refusals of the new-change commands. */
export const NEW_CHANGE_ERRORS = {
  noProjectOpen: "no_project_open",
  notFound: "proposal_not_found",
  alreadyDecided: "already_decided",
  unknownStream: "unknown_stream",
  changeNotFound: "change_not_found",
  streamBusy: "stream_busy",
  changeExists: "openspec_change_exists",
  invalidName: "invalid_change_name",
  missingProposalFile: "missing_proposal_file",
  pathOutsideChange: "path_outside_change",
  pending: "new_change_proposal_pending",
  nothingAccepted: "nothing_accepted",
  splitConfirmed: "split_confirmed",
  discussionResolved: "discussion_resolved",
  writeFailed: "write_failed",
} as const;

/** CMS-FR-QYGP: a reference to one new-change proposal. */
export interface NewChangeProposalReference {
  kind: "newChangeProposal";
  proposalId: string;
  streamId: string;
  /** The original change. */
  change: string;
  /** Present on the comment an update appended. */
  update?: "change" | "withdraw";
}

/** CMS-FR-MTUL: the comment is about one new-change proposal. */
export interface NewChangeSubjectReference {
  kind: "newChangeSubject";
  proposalId: string;
  streamId: string;
  change: string;
}
