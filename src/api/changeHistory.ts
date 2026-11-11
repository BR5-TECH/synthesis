// The versions, the statistics, and the split of an OpenSpec change on a work
// stream (CVH-change-version-history.md, CST-change-statistics.md, and
// NCP-new-change-proposals.md, contract surfaces). This module is the only
// place their command names live.

import { invoke } from "@tauri-apps/api/core";

import type {
  ChangeSplit,
  ChangeStatistics,
  ChangeVersionFiles,
  ChangeVersionList,
  NewChangeDecisionOutcome,
  NewChangeProposalList,
} from "../types";

/** CVH-FR-VZGR: every version of the change, most recent first. */
export const listChangeVersions = (streamId: string, change: string) =>
  invoke<ChangeVersionList>("list_change_versions", { streamId, change });

/** CVH-FR-VDZZ: the files of the change folder as one version holds them. */
export const readChangeVersion = (streamId: string, change: string, commitId: string) =>
  invoke<ChangeVersionFiles>("read_change_version", { streamId, change, commitId });

/** CST-FR-WPHC: the folded statistics of one change. */
export const readChangeStatistics = (streamId: string, change: string) =>
  invoke<ChangeStatistics>("read_change_statistics", { streamId, change });

/** CST-FR-TSOU: record one settled foreground interval of work on a change. */
export const recordChangeWorkingInterval = (
  streamId: string,
  change: string,
  startedAt: string,
  endedAt: string,
) =>
  invoke<null>("record_change_working_interval", { streamId, change, startedAt, endedAt });

/** NCP contract: the proposals of the split of one change, and the split. */
export const listNewChangeProposals = (streamId: string, change: string) =>
  invoke<NewChangeProposalList>("list_new_change_proposals", { streamId, change });

/** OCW-FR-BGDN: the proposed files, for View files. */
export const loadNewChangeProposalFiles = (proposalId: string) =>
  invoke<{ files: { path: string; text: string }[] }>("load_new_change_proposal_files", {
    proposalId,
  });

/** NCP-FR-UWOR: accept one proposal. It writes and commits the new change. */
export const acceptNewChangeProposal = (proposalId: string, feedback?: string | null) =>
  invoke<NewChangeDecisionOutcome>("accept_new_change_proposal", {
    proposalId,
    feedback: feedback ?? null,
  });

/** NCP-FR-TWDN: reject one proposal. Nothing is written into the working copy. */
export const rejectNewChangeProposal = (proposalId: string, feedback?: string | null) =>
  invoke<NewChangeDecisionOutcome>("reject_new_change_proposal", {
    proposalId,
    feedback: feedback ?? null,
  });

/** NCP-FR-KRSA: make the commit an acceptance still owes. */
export const completeNewChangeDecision = (proposalId: string) =>
  invoke<NewChangeDecisionOutcome>("complete_new_change_decision", { proposalId });

/** NCP-FR-MQCS: the author ends the split. */
export const confirmChangeSplit = (streamId: string, change: string) =>
  invoke<ChangeSplit>("confirm_change_split", { streamId, change });
