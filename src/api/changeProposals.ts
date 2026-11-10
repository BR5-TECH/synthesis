// Change edit proposals (CEP-change-edit-proposals.md, contract surface).

import { invoke } from "@tauri-apps/api/core";

import type {
  ChangeDecisionOutcome,
  ChangeEditHunks,
  ChangeEditProposal,
} from "../types";

/** CEP-FR-HUFE: every proposal of one change, the most recent first. */
export const listChangeEditProposals = (streamId: string, change: string) =>
  invoke<ChangeEditProposal[]>("list_change_edit_proposals", { streamId, change });

/** CEP-FR-HUFE: the hunks of one proposal, each with its path. */
export const loadChangeEditProposalHunks = (proposalId: string) =>
  invoke<ChangeEditHunks>("load_change_edit_proposal_hunks", { proposalId });

/** CEP-FR-OPGW: accept one hunk. It writes the file in the stream's working copy. */
export const acceptChangeEditHunk = (proposalId: string, hunkId: string, feedback?: string) =>
  invoke<ChangeDecisionOutcome>("accept_change_edit_hunk", {
    proposalId,
    hunkId,
    ...(feedback === undefined ? {} : { feedback }),
  });

/** CEP-FR-UCVO: reject one hunk. It writes nothing into the working copy. */
export const rejectChangeEditHunk = (proposalId: string, hunkId: string, feedback?: string) =>
  invoke<ChangeDecisionOutcome>("reject_change_edit_hunk", {
    proposalId,
    hunkId,
    ...(feedback === undefined ? {} : { feedback }),
  });

/** CEP-FR-UCVO: hold one hunk for discussion, or release it. */
export const setChangeEditHunkDiscussing = (
  proposalId: string,
  hunkId: string,
  discussing: boolean,
) =>
  invoke<ChangeEditProposal>("set_change_edit_hunk_discussing", {
    proposalId,
    hunkId,
    discussing,
  });

/** CEP-FR-UCVO: the author's own text for one hunk. */
export const editChangeEditHunk = (
  proposalId: string,
  hunkId: string,
  after: string,
  baselineChecksum: string,
) =>
  invoke<{ checksum: string }>("edit_change_edit_hunk", {
    proposalId,
    hunkId,
    after,
    baselineChecksum,
  });

/** CEP-FR-UCVO: reject every hunk still undecided. */
export const declineChangeEditProposal = (proposalId: string, feedback?: string) =>
  invoke<ChangeDecisionOutcome>("decline_change_edit_proposal", {
    proposalId,
    ...(feedback === undefined ? {} : { feedback }),
  });

/** CEP-FR-IYHP: commit the files still owed, and append a comment still owed. */
export const completeChangeEditDecision = (proposalId: string) =>
  invoke<ChangeDecisionOutcome>("complete_change_edit_decision", { proposalId });
