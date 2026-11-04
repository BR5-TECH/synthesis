/**
 * The decomposition commands (`DCP-draft-change-proposals.md`, "New-draft
 * proposals"). This module is the only place their command names live.
 */

import { invoke } from "@tauri-apps/api/core";

import type { DraftRecord, NewDraftDecisionOutcome, NewDraftProposal } from "../types";

/** DCP-FR-ZDSP / DCR-FR-RDLK: every new-draft proposal, newest first. */
export const listNewDraftProposals = (draftId: string) =>
  invoke<NewDraftProposal[]>("list_new_draft_proposals", { draftId });

/** DCR-FR-HWLA: the proposed body, for the draft preview tab. */
export const loadNewDraftProposalBody = (proposalId: string) =>
  invoke<{ body: string }>("load_new_draft_proposal_body", { proposalId });

/** DCR-FR-UTKG / DCP-FR-KOOA: accept one proposal and create its draft. */
export const acceptNewDraftProposal = (proposalId: string, feedback?: string | null) =>
  invoke<NewDraftDecisionOutcome>("accept_new_draft_proposal", {
    proposalId,
    feedback: feedback ?? null,
  });

/** DCR-FR-FMRQ / DCP-FR-FZEP: reject one proposal; nothing is created. */
export const rejectNewDraftProposal = (proposalId: string, feedback?: string | null) =>
  invoke<NewDraftDecisionOutcome>("reject_new_draft_proposal", {
    proposalId,
    feedback: feedback ?? null,
  });

/** DCR-FR-BELO / DCP-FR-VLLP: the author ends the decomposition. */
export const confirmDraftDecomposition = (draftId: string) =>
  invoke<DraftRecord>("confirm_draft_decomposition", { draftId });
