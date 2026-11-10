// Change edit proposals (CEP-change-edit-proposals.md, contract surface).
//
// The edits an agent proposes to one OpenSpec change on one work stream. The
// hunk model is the one of the prompt change proposals, with a change-relative
// path on each hunk.

import type {
  HunkAnchor,
  HunkCounts,
  HunkKind,
  HunkPlacement,
  HunkState,
  Participant,
} from "./comments";

/** CEP-FR-HUFE: one row of a proposal's ledger, with the path of its hunk. */
export interface ChangeEditLedgerRow {
  id: string;
  /** CEP-FR-WJKY: the path relative to `openspec/changes/<change>/`. */
  path: string;
  kind: HunkKind;
  /** CEP-FR-HRCX: the hunk creates a file that the change does not hold. */
  newFile: boolean;
  state: HunkState;
  edited: boolean;
  revision: number;
  decidedAt?: string | null;
}

/** CEP contract: one change edit proposal. */
export interface ChangeEditProposal {
  id: string;
  streamId: string;
  change: string;
  rationale: string;
  threadId: string;
  commentId: string;
  state: "pending" | "accepted" | "rejected";
  candidateEdited: boolean;
  commentOwed: boolean;
  /** CEP-FR-IYHP: a commit at the close failed, and the files wait for one. */
  commitOwed: boolean;
  hunkCount: number;
  createdAt: string;
  decidedAt?: string | null;
  counts: HunkCounts;
  /** CEP-FR-HUFE: every path an accepted hunk of this proposal wrote. */
  writtenPaths: string[];
  /** The ledger, in proposal order. */
  hunks: ChangeEditLedgerRow[];
  originChecksums?: Record<string, string> | null;
  agent: Participant;
}

/** CEP-FR-HUFE: one hunk, with its text, as `load change edit proposal hunks` gives it. */
export interface ChangeEditHunk {
  id: string;
  kind: HunkKind;
  revision: number;
  note?: string | null;
  before?: string | null;
  after?: string | null;
  anchor: HunkAnchor;
  path: string;
  newFile: boolean;
  state: HunkState;
}

/** CEP-FR-HUFE: the hunks of one proposal and where each lands now. */
export interface ChangeEditHunks {
  hunks: ChangeEditHunk[];
  /** Parallel to `hunks`, index for index. */
  resolutions: HunkPlacement[];
  checksum: string;
}

/** CEP contract: what a decision gives back. */
export interface ChangeDecisionOutcome {
  proposal: ChangeEditProposal;
  /** CEP-FR-FOJW: this decision closed the proposal and its commit landed. */
  committed: boolean;
  /** CEP-FR-RWTQ: this decision left no hunk undecided. */
  resolved: boolean;
  /** The decision comment, where one was appended. */
  commentId?: string;
  originKind: "change_discussion" | "change_comment";
}

/** CEP-FR-SQCG: what happened to the proposal an event carries. */
export type ChangeProposalChangeCause =
  | "recorded"
  | "updated"
  | "decided"
  | "held"
  | "committed"
  | "reconciled";

/** CEP-FR-SQCG: what `"change-edit-proposals-changed"` carries. */
export interface ChangeProposalsChangedPayload {
  streamId: string;
  change: string;
  proposal: ChangeEditProposal;
  cause: ChangeProposalChangeCause;
}

/** CEP contract: the typed refusals of the change edit proposal commands. */
export const CHANGE_PROPOSAL_ERRORS = {
  noProjectOpen: "no_project_open",
  notFound: "proposal_not_found",
  unknownStream: "unknown_stream",
  changeNotFound: "change_not_found",
  pathOutsideChange: "path_outside_change",
  fileExists: "file_exists",
  /** CEP-FR-ESQR: a graduation run of the stream is working or reviewing. */
  streamBusy: "stream_busy",
  alreadyDecided: "already_decided",
  candidateStale: "candidate_stale",
  anchorLost: "anchor_lost",
  hunkNotFound: "hunk_not_found",
  hunkAlreadyDecided: "hunk_already_decided",
  hunkWithdrawn: "hunk_withdrawn",
  writeFailed: "write_failed",
  acceptanceInProgress: "acceptance_in_progress",
  acceptanceIncomplete: "acceptance_incomplete",
  discussionResolved: "discussion_resolved",
} as const;
