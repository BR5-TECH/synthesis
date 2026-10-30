/**
 * Reviewing a proposal to a prompt or DOC file in place, in its Editor tab
 * (`../../../specifications/ui/PCR-prompt-change-review.md` PCR-FR-QHZN,
 * PCR-FR-AXPW, PCR-FR-LGVY, PCR-FR-14, PCR-FR-15; `DCR-draft-change-review.md`
 * DCR-FR-GNRL).
 *
 * The review is the draft's (`../DraftDiscussion/useProposalReview.tsx`): the
 * same decorations, chip, review bar, gutter map, keys, and hunk edits. What is
 * the file's own is here — the prompt module's commands, and the order an
 * acceptance takes against the tab's edit session.
 */
import { useCallback, useMemo, useState, type ReactNode } from "react";

import * as api from "../../api";
import { logInfo, logWarn } from "../../logging";
import { fileDecisionMessage } from "../DraftDiscussion/messages";
import { dispatchDecision } from "../DraftDiscussion/decisionDispatch";
import {
  useProposalReview,
  type ProposalReviewKind,
} from "../DraftDiscussion/useProposalReview";
import { promptHunkCandidateBuffers } from "../../state/candidateBuffers";
import { artifactReviewKey } from "../../state/draftDiscussion";
import type { EditSessionStore } from "../../state/editSessions";
import { notePromptProposalChanged } from "../../state/promptProposals";
import {
  PROMPT_PROPOSAL_ERRORS,
  type ProjectAgent,
  type PromptChangeProposal,
  type PromptDecisionOutcome,
} from "../../types";
import type { EditorReview } from "./props";

/** PCR-FR-14: tell the conversation what was decided. Best-effort. */
function dispatchOutcome(
  proposal: PromptChangeProposal,
  outcome: PromptDecisionOutcome,
  agents: readonly ProjectAgent[],
): void {
  void dispatchDecision(proposal, outcome, undefined, agents, "").catch((error: unknown) => {
    logWarn(["ai", "frontend"], "could not tell the agents what was decided", {
      proposalId: proposal.id,
      threadId: proposal.threadId,
      error: String(error),
    });
  });
}

export function useArtifactReview({
  artifactId,
  sessions,
  proposal,
  proposals,
  enabled,
  locked,
  onDiscussHunk,
  agents,
}: {
  artifactId: string;
  /** PCR-FR-AXPW: the tab's edit session store. */
  sessions: EditSessionStore;
  /** PCP-FR-04: the file's one undecided proposal, if it has one. */
  proposal: PromptChangeProposal | undefined;
  /** PCR-FR-15: every proposal of the file, for one whose comment is owed. */
  proposals: readonly PromptChangeProposal[];
  /** False where the host passes a review of its own (the draft column). */
  enabled: boolean;
  /** The surface takes no edits, so nothing here may write the file. */
  locked: boolean;
  /** PCR-FR-HDMC: open, or show, the discussion of one change. */
  onDiscussHunk: (hunkId: string) => void;
  /** PCR-FR-14: the project's agents, for the decision's own dispatch. */
  agents: readonly ProjectAgent[];
}): { review: EditorReview | undefined; footer: ReactNode } {
  /** PCR-FR-15: the proposal whose acceptance landed with its comment owed. */
  const [owedHere, setOwedHere] = useState<PromptChangeProposal | null>(null);
  const [retrying, setRetrying] = useState(false);
  const kind = useMemo<ProposalReviewKind<PromptChangeProposal>>(
    () => ({
      source: "prompt",
      reviewKey: artifactReviewKey(artifactId),
      buffers: promptHunkCandidateBuffers,
      noun: "file",
      message: fileDecisionMessage,
      decide: async (pending, hunkId, how) => {
        if (how === "reject") {
          // PCP-FR-RJHN: a rejection writes nothing to the file.
          const outcome = await api.rejectPromptChangeHunk(pending.id, hunkId);
          settle(pending, outcome, "rejected");
          return true;
        }
        return accept(pending, hunkId);
      },
      decline: async (pending) => {
        const outcome = await api.declinePromptChangeProposal(pending.id);
        settle(pending, outcome, "declined");
      },
    }),
    // `accept` and `settle` read only these.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [artifactId, sessions, agents],
  );

  /**
   * PCR-FR-14: after a decision lands, the record moves on and the resolving
   * decision dispatches. The record is folded in from the outcome, so the
   * review follows at once; the event that also carries it changes nothing.
   */
  function settle(
    pending: PromptChangeProposal,
    outcome: PromptDecisionOutcome,
    how: "accepted" | "rejected" | "declined",
  ): void {
    notePromptProposalChanged({ artifactId, proposal: outcome.proposal });
    logInfo(["frontend"], "decided a change of a prompt proposal", {
      proposalId: pending.id,
      artifactId,
      how,
      resolved: outcome.resolved,
    });
    // PCR-FR-14 / PCP-FR-RSLV: only the resolving decision carries a comment
    // id, so only it dispatches.
    if (outcome.resolved) dispatchOutcome(pending, outcome, agents);
  }

  /**
   * PCR-FR-AXPW: accept one change, in the order the requirement fixes.
   *
   * The session's pending write is brought forward and the session is
   * quiesced, so no write of the author's buffer races the acceptance; a
   * standing external-change modal is dismissed with neither resolution taken.
   * Then the acceptance. On success the session is reset to what the file now
   * holds. On a refusal the quiesce is lifted and the typed error is said.
   */
  async function accept(pending: PromptChangeProposal, hunkId: string): Promise<boolean> {
    const session = sessions.get(artifactId);
    // The author's typing reaches disk first, so the backend resolves the
    // change against the file the author is looking at.
    if (session && session.dirty && !session.conflict) {
      const flushed = await sessions.flush(artifactId);
      if (!flushed.ok) {
        logWarn(["frontend"], "the file could not be written before an acceptance", {
          artifactId,
          blocked: flushed.blocked ?? "unknown",
        });
        throw new Error(PROMPT_PROPOSAL_ERRORS.writeFailed);
      }
    }
    const release = sessions.holdExternalChanges(artifactId);
    /**
     * EXC-FR-UVJY: the session takes the file's new text. The acceptance has
     * landed already, so a reset that fails is logged and is not a refusal.
     */
    const reset = async () => {
      try {
        await sessions.resetRestored(artifactId);
      } catch (error) {
        logWarn(["frontend"], "the file could not be read again after an acceptance", {
          artifactId,
          error: String(error),
        });
      }
    };
    let outcome: Awaited<ReturnType<typeof api.acceptPromptChangeHunk>>;
    try {
      await sessions.quiesce([artifactId]);
      sessions.dismissConflict(artifactId);
      outcome = await api.acceptPromptChangeHunk(pending.id, hunkId);
    } catch (error) {
      const reason = String(error);
      if (reason.includes(PROMPT_PROPOSAL_ERRORS.acceptanceIncomplete)) {
        // PCR-FR-15: not a refusal. The change is applied and the
        // conversation is not told yet, so the session takes the file's new
        // text and the retry is offered.
        await reset();
        release();
        logWarn(["frontend"], "a prompt change was applied with its comment owed", {
          proposalId: pending.id,
          artifactId,
        });
        setOwedHere(pending);
        return true;
      }
      // PCR-FR-AXPW: a refusal resets nothing and lifts the quiesce.
      sessions.unquiesce(artifactId);
      release();
      logWarn(["frontend"], "a change of a prompt proposal was refused", {
        proposalId: pending.id,
        artifactId,
        error: reason,
      });
      throw error;
    }
    // PCR-FR-AXPW: the acceptance landed, so it is settled and dispatched
    // before the session is reset, and a reset that fails changes neither.
    settle(pending, outcome, "accepted");
    try {
      await reset();
    } finally {
      release();
    }
    return true;
  }

  const review = useProposalReview({
    kind,
    proposal: enabled ? proposal : undefined,
    locked,
    viewingVersion: false,
    onDiscussHunk,
  });

  /**
   * PCR-FR-15: an owed comment, from this session's own acceptance or from a
   * record that says so. A record that no longer owes one clears it.
   */
  const owedRecord = proposals.find((p) => p.commentOwed);
  const knownOwed =
    owedHere !== null && proposals.find((p) => p.id === owedHere.id)?.commentOwed === false
      ? null
      : owedHere;
  const owed = enabled ? (owedRecord ?? knownOwed) : null;

  /**
   * PCR-FR-15: the retry invokes `complete_prompt_change_decision` and nothing
   * else — never a second acceptance. On success it dispatches (PCR-FR-14).
   */
  const retry = useCallback(() => {
    if (owed === null) return;
    setRetrying(true);
    void api
      .completePromptChangeDecision(owed.id)
      .then((outcome) => {
        notePromptProposalChanged({ artifactId, proposal: outcome.proposal });
        if (outcome.commentId === undefined) {
          logWarn(["frontend"], "a prompt decision comment is still owed", {
            proposalId: owed.id,
          });
          return;
        }
        setOwedHere(null);
        dispatchOutcome(owed, outcome, agents);
      })
      .catch((error: unknown) => {
        logWarn(["frontend"], "a prompt decision comment could not be completed", {
          proposalId: owed.id,
          error: String(error),
        });
      })
      .finally(() => setRetrying(false));
  }, [owed, artifactId, agents]);

  /**
   * PCR-FR-15: not a refusal. The change is applied and the conversation is not
   * told yet; the statement stands at the foot of the document column, where
   * the review bar stands, with the retry.
   */
  const footer: ReactNode =
    owed !== null ? (
      <div className="dds-review" role="region" aria-label="Decision comment owed">
        <p className="dds-review__standing t-ui-xs" role="status" aria-live="polite">
          The change was applied to this file. The discussion has not been told yet.
        </p>
        <div className="dds-review__row">
          <div className="spacer" />
          <button className="btn btn--sm" disabled={retrying} onClick={retry}>
            Try again
          </button>
        </div>
      </div>
    ) : null;

  return { review, footer };
}
