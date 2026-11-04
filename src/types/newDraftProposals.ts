/**
 * The new-draft proposals of a decomposition (`DCP-draft-change-proposals.md`,
 * "New-draft proposals"; `DRS-draft-storage.md` DRS-FR-MXQF, DRS-FR-LPCX).
 */

import type { Participant } from "./comments";
import type { DraftRecord } from "./drafts";

/** DCP contract surface: `NewDraftProposalState`. */
export type NewDraftState = "pending" | "accepted" | "rejected" | "withdrawn";

/** DCP contract surface: `NewDraftProposal`. */
export interface NewDraftProposal {
  id: string;
  /** DCP-FR-CKWC: the decomposition this proposal belongs to. */
  setId: string;
  /** The original draft. */
  draftId: string;
  name: string;
  position: number;
  total: number;
  agent: Participant;
  rationale: string;
  threadId: string;
  commentId: string;
  state: NewDraftState;
  revision: number;
  createdDraftId?: string | null;
  createdAt: string;
  decidedAt?: string | null;
}

/** DCP contract surface: `NewDraftDecisionOutcome`. */
export interface NewDraftDecisionOutcome {
  proposal: NewDraftProposal;
  createdDraft?: DraftRecord | null;
  commentId?: string | null;
  originKind: "draft_discussion" | "draft_comment";
}

/** DCP-FR-ZDSP: the payload of `"new draft proposals changed"`. */
export interface NewDraftProposalsChangedPayload {
  draftId: string;
  proposal?: NewDraftProposal | null;
  cause: "recorded" | "updated" | "decided" | "confirmed";
}

/** DRS-FR-MXQF: the place of a draft in a decomposition. */
export interface DraftSequence {
  setId: string;
  position: number;
  total: number;
  originalDraftId: string;
  originalName: string;
}

/** DRS-FR-LPCX: the marker of a superseded draft. */
export interface SupersededMarker {
  setId: string;
  confirmedAt: string;
  drafts: { id: string; name: string }[];
}

/** CMS-FR-YKGI: a reference to one new-draft proposal. */
export interface NewDraftProposalReference {
  kind: "newDraftProposal";
  proposalId: string;
  draftId: string;
  /** Present on the comment an update appended. */
  update?: "change" | "withdraw";
}

/** CMS-FR-KAXQ: the comment is about one new-draft proposal. */
export interface NewDraftSubjectReference {
  kind: "newDraftSubject";
  proposalId: string;
  draftId: string;
}

/** DRP-FR-IAWQ: the hover text and accessible name of a sequence badge. */
export const sequenceLabel = (sequence: DraftSequence, originalName: string): string =>
  `Part ${sequence.position} of ${sequence.total} of “${originalName}”`;

/** DRP-FR-NOXB / NAW-FR-MONI: the hover text of a superseded marker. */
export const supersededLabel = (marker: SupersededMarker): string =>
  `Superseded by ${marker.drafts.map((d) => d.name).join(", ")}`;
