/**
 * The new-draft card of a decomposition (`DCR-draft-change-review.md`
 * DCR-FR-RDLK, DCR-FR-QWXP, DCR-FR-MGMZ, DCR-FR-UTKG, DCR-FR-FMRQ,
 * DCR-FR-NMJH, DCR-FR-AMLR, DCR-FR-HWLA, DCR-FR-TOOT, DCR-FR-FKNQ,
 * DCR-FR-THXH, DCR-FR-PBDD, DCR-FR-LFDJ).
 *
 * It reads its proposal from the one reading per draft that
 * `state/newDraftProposals.ts` keeps, and it decides nothing but on Accept and
 * Reject.
 */
import { useEffect, useRef, useState } from "react";

import * as api from "../../../api";
import { logInfo, logWarn } from "../../../logging";
import { requestDiscussionFoot } from "../../../state/discussionSession";
import { runMainBranchWrite, WRITE_CANCELLED } from "../../../state/mainBranchWrite";
import {
  clearRevealedCard,
  discussNewDraft,
  draftSummaryOf,
  openDraftPreview,
  useRevealedCard,
} from "../../../state/decompositionRoutes";
import {
  decompositionConfirmed,
  noteNewDraftChanged,
  useNewDraftProposals,
  useNewDraftStore,
} from "../../../state/newDraftProposals";
import {
  participantName,
  type AgentTurn,
  type Comment,
  type NewDraftProposal,
  type NewDraftProposalReference,
  type NewDraftState,
} from "../../../types";
import { formatRelative } from "../../ProjectPicker";
import { DecompositionNextStep } from "./DecompositionNextStep";
import { dispatchAfterDecision } from "./decompositionDispatch";
import { TypeChip } from "./TypeChip";

/** DCR-FR-QWXP: each state in words, with its own marker. */
const STATE: Record<NewDraftState, { word: string; mark: string }> = {
  pending: { word: "Waiting for you", mark: "●" },
  accepted: { word: "Accepted", mark: "✓" },
  rejected: { word: "Rejected", mark: "✕" },
  withdrawn: { word: "Withdrawn", mark: "–" },
};

/** DCR-FR-PBDD: why the original draft takes no decision now, or null. */
export function blockedReason(draftId: string): string | null {
  const draft = draftSummaryOf(draftId);
  if (!draft) return null;
  if (draft.githubIssue) return "This draft mirrors a GitHub task, so its proposals can not be decided.";
  if (draft.superseded) return "The decomposition of this draft is confirmed.";
  if (draft.status === "graduated") return "This draft is graduated, so its proposals can not be decided.";
  if (draft.status === "archived") return "This draft is archived. Restore it to decide its proposals.";
  if (draft.graduation?.locked) return "A graduation run holds this draft. Decide when the run ends.";
  return null;
}

/** The newest comment of the thread that names this proposal (DCR-FR-MGMZ). */
function newestReferenceOf(comments: readonly Comment[], proposalId: string): string | null {
  for (let i = comments.length - 1; i >= 0; i -= 1) {
    const c = comments[i];
    if (c.attachments.some((a) => a.kind === "newDraftProposal" && a.proposalId === proposalId)) {
      return c.id;
    }
  }
  return null;
}

/** DCR-FR-KNDA: the newest decided proposal of the set. */
function newestDecided(proposals: readonly NewDraftProposal[]): NewDraftProposal | undefined {
  return [...proposals]
    .filter((p) => p.state !== "pending")
    .sort((a, b) => (b.decidedAt ?? "").localeCompare(a.decidedAt ?? ""))[0];
}

export function NewDraftCard({
  reference,
  comment,
  comments,
  pendingTurns,
}: {
  reference: NewDraftProposalReference;
  /** The comment that carries this reference. */
  comment: Comment;
  /** Every comment of the thread, to find the newest reference. */
  comments: readonly Comment[];
  /** DCR-FR-KNDA: the row stands only while no turn of the conversation runs. */
  pendingTurns: readonly AgentTurn[];
}) {
  const { proposals } = useNewDraftProposals(reference.draftId);
  // DCR-FR-KNDA: a confirmation changes no proposal, so follow the store too.
  useNewDraftStore();
  const proposal = proposals.find((p) => p.id === reference.proposalId);
  const [feedback, setFeedback] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const cardRef = useRef<HTMLElement | null>(null);
  // CMT-FR-MPKL / DCR-FR-QQLE: a route asked for this card, so it is shown.
  const revealed = useRevealedCard();
  const holds = proposal ? newestReferenceOf(comments, proposal.id) === comment.id : false;
  useEffect(() => {
    if (!proposal || revealed !== proposal.id || !holds || !cardRef.current) return;
    cardRef.current.scrollIntoView?.({ block: "center" });
    cardRef.current.focus();
    clearRevealedCard(proposal.id);
  }, [revealed, proposal, holds]);

  const focusCard = () => requestAnimationFrame(() => cardRef.current?.focus());

  if (!proposal) {
    return (
      <div className="dds-newdraft-ref" data-testid="new-draft-card-unread">
        <TypeChip kind="newDraft" /> Proposed draft
      </div>
    );
  }

  const place = `Part ${proposal.position} of ${proposal.total}`;
  const state = STATE[proposal.state];

  // DCR-FR-MGMZ: an earlier comment of the same proposal is one line.
  if (!holds) {
    const verb =
      reference.update === "change"
        ? "Changed the proposed draft"
        : reference.update === "withdraw"
          ? "Withdrew the proposed draft"
          : "Proposed a draft";
    return (
      <div className="dds-newdraft-ref" data-testid="new-draft-reference">
        <TypeChip kind="newDraft" /> {verb} · {place} · {proposal.name}
      </div>
    );
  }

  const blocked = blockedReason(proposal.draftId);
  const decide = (accept: boolean) => {
    setBusy(true);
    setError(null);
    const sent = feedback.trim();
    const feedbackText = sent === "" ? null : sent;
    // MBW-FR-DBGA: an accept creates a draft, so it asks for the confirmation
    // on a main branch first. A reject writes nothing.
    const call = accept
      ? runMainBranchWrite("create_draft", () =>
          api.acceptNewDraftProposal(proposal.id, feedbackText),
        )
      : api.rejectNewDraftProposal(proposal.id, feedbackText);
    call.then(
      (outcome) => {
        setBusy(false);
        if (outcome === WRITE_CANCELLED) return;
        setFeedback("");
        logInfo(["frontend"], "a proposed draft was decided", {
          proposalId: proposal.id,
          decision: accept ? "accept" : "reject",
          withFeedback: sent !== "",
        });
        // DCR-FR-UTKG / DCR-FR-FMRQ: the card reads the decision at once,
        // and the backend's event folds into the same reading later.
        noteNewDraftChanged({
          draftId: outcome.proposal.draftId,
          proposal: outcome.proposal,
          cause: "decided",
        });
        dispatchAfterDecision(outcome.proposal, outcome.commentId, sent);
        // CVP-FR-CUKV: the decision is a post of the author, so the
        // discussion follows again.
        requestDiscussionFoot(outcome.proposal.threadId);
        // DCR-FR-FKNQ: the pressed control is gone, so focus stays on the card.
        focusCard();
      },
      (e: unknown) => {
        // DCR-FR-TOOT: the error stands beside the card, and the feedback stays.
        setBusy(false);
        setError(String(e));
        logWarn(["frontend"], "a proposed draft could not be decided", {
          proposalId: proposal.id,
          error: String(e).slice(0, 80),
        });
      },
    );
  };

  const newest = newestDecided(proposals);
  const anyPending = proposals.some((p) => p.state === "pending");
  const confirmed =
    decompositionConfirmed(proposal.draftId) || Boolean(draftSummaryOf(proposal.draftId)?.superseded);
  const isNewestDecided = newest?.id === proposal.id;
  const accepted = proposals.filter((p) => p.state === "accepted");

  return (
    <>
      <article
        ref={cardRef}
        tabIndex={-1}
        className="dds-question dds-newdraft"
        data-testid="new-draft-card"
        data-state={proposal.state}
        aria-label={`Proposed draft, part ${proposal.position} of ${proposal.total}, ${proposal.name}, ${state.word}`}
      >
        <div className="dds-question__head">
          <TypeChip kind="newDraft" />
          <span className="dds-question__author">{participantName(comment.author)}</span>
          <span className="dds-stream__time" title={comment.createdAt}>
            {formatRelative(comment.createdAt)}
          </span>
        </div>
        <div className="dds-newdraft__title">
          <span className="dds-newdraft__name">
            {place} · {proposal.name}
          </span>
          <span className="dds-newdraft__state" data-state={proposal.state}>
            <span aria-hidden="true">{state.mark}</span> {state.word}
          </span>
        </div>
        {proposal.state === "accepted" && (
          <p className="dds-newdraft__created">Created draft {proposal.name}</p>
        )}
        {proposal.state === "pending" && (
          <div className="dds-newdraft__decide">
            <input
              className="dds-newdraft__feedback"
              type="text"
              value={feedback}
              placeholder="Feedback (optional)"
              aria-label="Feedback for the agent (optional)"
              onChange={(e) => setFeedback(e.target.value)}
            />
            <div className="dds-newdraft__actions">
              <button
                type="button"
                className="btn btn--primary btn--sm"
                disabled={busy || blocked !== null}
                onClick={() => decide(true)}
              >
                Accept
              </button>
              <button
                type="button"
                className="btn btn--sm"
                disabled={busy || blocked !== null}
                onClick={() => decide(false)}
              >
                Reject
              </button>
              <button
                type="button"
                className="btn btn--ghost btn--sm"
                onClick={() => discussNewDraft(proposal.draftId, proposal.id)}
              >
                Discuss →
              </button>
              <button
                type="button"
                className="btn btn--ghost btn--sm"
                onClick={() => openDraftPreview(proposal.id, proposal.name)}
              >
                Preview ↗
              </button>
            </div>
            {blocked !== null && <p className="dds-newdraft__reason">{blocked}</p>}
          </div>
        )}
        {error !== null && (
          <p className="dds-newdraft__error" role="alert">
            {error}
          </p>
        )}
      </article>
      {isNewestDecided && confirmed && (
        <p className="dds-next-step__done" role="status" data-testid="decomposition-confirmed">
          Decomposition confirmed.
          {accepted.length > 0 ? ` The new drafts: ${accepted.map((p) => p.name).join(", ")}.` : ""}
        </p>
      )}
      {isNewestDecided && !confirmed && !anyPending && pendingTurns.length === 0 && (
        <DecompositionNextStep
          newest={proposal}
          accepted={accepted}
          blocked={blockedReason(proposal.draftId)}
          onDone={focusCard}
        />
      )}
    </>
  );
}
