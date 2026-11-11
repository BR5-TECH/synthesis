/**
 * The review of a change edit proposal in the change workspace
 * (`../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-COLR,
 * OCW-FR-DTRX, OCW-FR-TAZJ, OCW-FR-MLXJ, OCW-FR-EJGH, OCW-FR-XJMR).
 *
 * The review is the shared one (`../DraftDiscussion/useProposalReview.tsx`):
 * the same decorations, chip, hunk edits, and focus. What is the change's own
 * is here: the commands of `CEP-change-edit-proposals.md`, the read of the
 * change after each decision, the refusal stated at the hunk, the toasts of a
 * close, and the event that moves the proposals on.
 */
import { useCallback, useEffect, useMemo, useState } from "react";

import * as api from "../../api";
import { onChangeEditProposalsChanged } from "../../events";
import { logInfo, logWarn } from "../../logging";
import { changeHunkCandidateBuffers } from "../../state/candidateBuffers";
import { rereadHunks } from "../../state/proposalHunks";
import { usePlanProposal } from "../../state/openspec/planProposal";
import {
  asReviewable,
  changeReviewKey,
  noteChangeProposalChanged,
  rereadChangeProposals,
  useChangeProposals,
  type ReviewableChangeProposal,
} from "../../state/openspec/changeProposals";
import { dispatchDecision } from "../DraftDiscussion/decisionDispatch";
import {
  useProposalReview,
  type ProposalReviewKind,
} from "../DraftDiscussion/useProposalReview";
import type { EditorReview } from "../Editor/props";
import {
  CHANGE_PROPOSAL_ERRORS,
  isUndecided,
  type ChangeDecisionOutcome,
  type ChangeEditProposal,
  type ProjectAgent,
} from "../../types";
import {
  changeDecisionMessage,
  raiseCloseToast,
  refusalCode,
  streamBusyMessage,
} from "./decisions";

/** OCW-FR-DTRX: a refused decision, stated at its hunk. */
export interface HunkRefusal {
  hunkId: string;
  message: string;
}

export interface ChangeReview {
  /** CEP-FR-ZOIG: the one pending proposal of the change, if any. */
  pending: ChangeEditProposal | undefined;
  /** The shared review over every hunk of the pending proposal. */
  review: EditorReview | undefined;
  /** OCW-FR-COLR: the path of each hunk, by hunk id. */
  pathOf: (hunkId: string) => string | undefined;
  /** OCW-FR-DTRX: the last refusal, at its hunk. */
  refusal: HunkRefusal | null;
  /** OCW-FR-EJGH: the count of the undecided hunks across the change. */
  undecided: number;
  /**
   * OCW-FR-EJGH: Reject all. It resolves null when the decline landed, and
   * the refusal in words when it did not.
   */
  rejectAll: () => Promise<string | null>;
  /**
   * OCW-FR-XJMR: the pending proposal is the plan proposal of a plan update
   * whose question set stands, so no hunk of it takes a decision.
   */
  planHeld: boolean;
}

/** OCW-FR-XJMR: the one sentence the review bar says while the plan questions stand. */
export const PLAN_QUESTIONS_FIRST =
  "Answer the questions in the plan thread first. Then decide these edits.";

export function useChangeReview(input: {
  streamId: string;
  change: string;
  /** OCW-FR-DTRX: read the change again after a decision. */
  onDecided: () => void;
  /** DCR-FR-MTFD: open, or show, the discussion of one hunk. */
  onDiscussHunk: (hunkId: string) => void;
  /** PCR-FR-14: the project's agents, for the decision's own dispatch. */
  agents: readonly ProjectAgent[];
}): ChangeReview {
  const { streamId, change, onDecided, onDiscussHunk, agents } = input;
  const proposals = useChangeProposals(streamId, change);
  const pending = proposals.find((p) => p.state === "pending");
  const [refusal, setRefusal] = useState<HunkRefusal | null>(null);
  // OCW-FR-XJMR: while the plan questions stand, the chips are unavailable.
  const planHeld = usePlanProposal(streamId, change, pending?.id ?? null).held;

  /**
   * OCW-FR-TAZJ: the tab follows the event for its stream and its change, and
   * reads the proposals again on it. The payload is folded in first, so the
   * review moves at once.
   */
  useEffect(() => {
    let stop: (() => void) | null = null;
    let cancelled = false;
    void onChangeEditProposalsChanged((payload) => {
      if (payload.streamId !== streamId || payload.change !== change) return;
      logInfo(["frontend"], "an edit proposal of the open change moved on", {
        streamId,
        proposalId: payload.proposal.id,
        cause: payload.cause,
      });
      noteChangeProposalChanged(payload);
      void rereadChangeProposals(streamId, change);
      // A hunk accepted elsewhere wrote a file, so the change is read again.
      if (payload.cause === "decided" || payload.cause === "committed") onDecided();
    })
      .then((fn) => {
        if (cancelled) fn();
        else stop = fn;
      })
      .catch((e: unknown) => {
        logWarn(["frontend"], "could not follow the edit proposals of a change", {
          streamId,
          error: refusalCode(e),
        });
      });
    return () => {
      cancelled = true;
      stop?.();
    };
  }, [streamId, change, onDecided]);

  /** OCW-FR-MLXJ: Retry of a commit that failed at the close. */
  const retryCommit = useCallback(
    (proposalId: string) => {
      void api
        .completeChangeEditDecision(proposalId)
        .then((outcome) => {
          noteChangeProposalChanged({ streamId, change, proposal: outcome.proposal, cause: "committed" });
          void rereadChangeProposals(streamId, change);
          onDecided();
          raiseCloseToast(change, { ...outcome, resolved: true }, retryCommit);
        })
        .catch((e: unknown) => {
          logWarn(["frontend"], "the commit of a change edit proposal failed again", {
            streamId,
            proposalId,
            error: refusalCode(e),
          });
        });
    },
    [streamId, change, onDecided],
  );

  /**
   * OCW-FR-DTRX / PCR-FR-14 / OCW-FR-MLXJ: a decision landed. The record moves
   * on, the change and the proposals are read again, the decision comment
   * dispatches the agent turns, and a close raises its toast.
   */
  const settle = useCallback(
    (outcome: ChangeDecisionOutcome, how: string) => {
      setRefusal(null);
      noteChangeProposalChanged({ streamId, change, proposal: outcome.proposal, cause: "decided" });
      void rereadChangeProposals(streamId, change);
      onDecided();
      logInfo(["frontend"], "decided a hunk of a change edit proposal", {
        streamId,
        proposalId: outcome.proposal.id,
        how,
        resolved: outcome.resolved,
        committed: outcome.committed,
      });
      if (outcome.commentId !== undefined) {
        void dispatchDecision(
          {
            id: outcome.proposal.id,
            threadId: outcome.proposal.threadId,
            target: { kind: "change", streamId, change },
          },
          outcome,
          undefined,
          agents,
          "",
        ).catch((e: unknown) => {
          logWarn(["ai", "frontend"], "could not tell the agents what was decided", {
            proposalId: outcome.proposal.id,
            error: refusalCode(e),
          });
        });
      }
      raiseCloseToast(change, outcome, retryCommit);
    },
    [streamId, change, onDecided, agents, retryCommit],
  );

  /** OCW-FR-DTRX: a refusal, stated at its hunk. Nothing else moves. */
  const refuse = useCallback(
    async (proposalId: string, hunkId: string, error: unknown) => {
      const code = refusalCode(error);
      logWarn(["frontend"], "a decision on a change edit proposal was refused", {
        streamId,
        proposalId,
        code,
      });
      const message = code.includes(CHANGE_PROPOSAL_ERRORS.streamBusy)
        ? await streamBusyMessage(streamId)
        : changeDecisionMessage(code);
      setRefusal({ hunkId, message });
    },
    [streamId],
  );

  const kind = useMemo<ProposalReviewKind<ReviewableChangeProposal>>(
    () => ({
      source: "change",
      reviewKey: changeReviewKey(streamId, change),
      buffers: changeHunkCandidateBuffers,
      noun: "file",
      message: changeDecisionMessage,
      decide: async (proposal, hunkId, how) => {
        try {
          const outcome =
            how === "accept"
              ? await api.acceptChangeEditHunk(proposal.id, hunkId)
              : await api.rejectChangeEditHunk(proposal.id, hunkId);
          settle(outcome, how);
          return true;
        } catch (error) {
          const code = refusalCode(error);
          // CEP-FR-IYHP: the hunk landed, and only what follows it is owed. It
          // is not a refusal, so the change and the proposals are read again.
          if (code.includes(CHANGE_PROPOSAL_ERRORS.acceptanceIncomplete)) {
            logWarn(["frontend"], "a hunk of a change landed with its follow-up owed", {
              streamId,
              proposalId: proposal.id,
            });
            void rereadChangeProposals(streamId, change);
            onDecided();
            return true;
          }
          await refuse(proposal.id, hunkId, error);
          return false;
        }
      },
      decline: async (proposal) => {
        try {
          settle(await api.declineChangeEditProposal(proposal.id), "declined");
        } catch (error) {
          const first = proposal.hunks[0]?.id ?? "";
          await refuse(proposal.id, first, error);
        }
      },
    }),
    [streamId, change, settle, refuse, onDecided],
  );

  const reviewable = useMemo(() => (pending ? asReviewable(pending) : undefined), [pending]);
  const shared = useProposalReview({
    kind,
    proposal: reviewable,
    locked: planHeld,
    viewingVersion: true,
    onDiscussHunk,
  });
  // OCW-FR-XJMR: accept, reject, and discuss are disabled through `busy`, and
  // the edit of a hunk's proposed text is not offered.
  const review = useMemo<EditorReview | undefined>(() => {
    if (!shared || !planHeld) return shared;
    const none = () => {};
    return {
      ...shared,
      busy: true,
      hunks: shared.hunks.map((hunk) => ({ ...hunk, editable: false })),
      onAccept: none,
      onReject: none,
      onDiscuss: none,
      onEdit: none,
    };
  }, [shared, planHeld]);

  const pathOf = useCallback(
    (hunkId: string) => pending?.hunks.find((h) => h.id === hunkId)?.path,
    [pending],
  );

  const undecided = useMemo(
    () => (pending ? pending.hunks.filter((h) => isUndecided(h.state)).length : 0),
    [pending],
  );

  /**
   * OCW-FR-EJGH: decline the pending proposal, after the bar's confirmation.
   * A decline that lands settles as every decision does, so the tab reads the
   * change and the proposals again.
   */
  const rejectAll = useCallback(async (): Promise<string | null> => {
    if (!pending) return null;
    if (planHeld) return PLAN_QUESTIONS_FIRST;
    try {
      const outcome = await api.declineChangeEditProposal(pending.id);
      settle(outcome, "declined");
      rereadHunks(pending.id, "change");
      return null;
    } catch (error) {
      const code = refusalCode(error);
      logWarn(["frontend"], "Reject all of a change edit proposal was refused", {
        streamId,
        proposalId: pending.id,
        code,
      });
      return code.includes(CHANGE_PROPOSAL_ERRORS.streamBusy)
        ? streamBusyMessage(streamId)
        : changeDecisionMessage(code);
    }
  }, [pending, settle, streamId, planHeld]);

  return { pending, review, pathOf, refusal, undecided, rejectAll, planHeld };
}
