/**
 * Reviewing a proposal inside the draft's document
 * (`../../../specifications/ui/DCR-draft-change-review.md`).
 *
 * A hook rather than a component, because what the review produces is not a
 * surface of its own: it is what the **editing surface** renders — decorations
 * in the prose, a bar at the foot of the page, and a chip over the space each
 * change reserves for it. The tab holds the draft; this holds what has been
 * proposed for it.
 *
 * DCR-FR-GNRL: the review itself is `../DraftDiscussion/useProposalReview.tsx`,
 * which the review of a prompt or DOC file uses too. What is the draft's own is
 * here: which commands decide a change, the autosave to bring forward before an
 * acceptance, and the version history to read again after one.
 */
import { useMemo } from "react";

import * as api from "../../api";
import type { DraftChangeProposal, ProjectAgent } from "../../types";
import { decisionMessage } from "../DraftDiscussion";
import { dispatchDecision } from "../DraftDiscussion/decisionDispatch";
import {
  useProposalReview,
  type ProposalReviewKind,
} from "../DraftDiscussion/useProposalReview";
import type { EditorReview } from "../Editor/props";
import { hunkCandidateBuffers } from "../../state/candidateBuffers";

export function useDraftReview({
  draftId,
  proposal,
  locked,
  viewingVersion,
  path,
  flushBeforeApply,
  reloadAfterApply,
  reloadHistory: loadHistory,
  onDiscussHunk,
  agents,
}: {
  draftId: string;
  /** DCP-FR-04: the draft's one undecided proposal, if it has one. */
  proposal: DraftChangeProposal | undefined;
  /** NAW-FR-44: a run holds the prompt, so nothing here may write it. */
  locked: boolean;
  /** NAW-FR-09: a past version is read-only, so it carries no review bar. */
  viewingVersion: boolean;
  /** The draft file an acceptance rewrites, which is the prompt. */
  path: string | null;
  /** NAW-FR-13: write the pending buffer before an acceptance lands on it. */
  flushBeforeApply: (path: string) => Promise<void>;
  /** NAW-FR-42: read the prompt again once an acceptance has rewritten it. */
  reloadAfterApply: (path: string) => void;
  reloadHistory: () => void;
  /** DCR-FR-MTFD: open, or show, the discussion of one change. */
  onDiscussHunk: (hunkId: string) => void;
  /** CTA-FR-VQFJ: the project's agents, for the decision's own dispatch. */
  agents: readonly ProjectAgent[];
}): EditorReview | undefined {
  const kind = useMemo<ProposalReviewKind<DraftChangeProposal>>(
    () => ({
      source: "draft",
      reviewKey: draftId,
      buffers: hunkCandidateBuffers,
      noun: "draft",
      message: decisionMessage,
      decide: async (pending, hunkId, how) => {
        let outcome;
        if (how === "accept") {
          // The autosave timer can still be counting down when the author
          // accepts. Written first, the acceptance overwrites it and the timer
          // has nothing left to put back over the accepted text.
          if (path !== null) await flushBeforeApply(path);
          outcome = await api.acceptDraftChangeHunk(pending.id, hunkId);
          if (path !== null) reloadAfterApply(path);
        } else {
          outcome = await api.rejectDraftChangeHunk(pending.id, hunkId);
        }
        loadHistory();
        // DCR-FR-15: the decision that leaves nothing undecided is the one that
        // appended a comment. An unresolving decision carries no `commentId`
        // and dispatches nothing, which `dispatchDecision` reads off the
        // outcome itself.
        void dispatchDecision(pending, outcome, undefined, agents, "");
        return true;
      },
      decline: async (pending) => {
        await api.declineDraftChangeProposal(pending.id);
      },
    }),
    [draftId, path, flushBeforeApply, reloadAfterApply, loadHistory, agents],
  );
  return useProposalReview({ kind, proposal, locked, viewingVersion, onDiscussHunk });
}
