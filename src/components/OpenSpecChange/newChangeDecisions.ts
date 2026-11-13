/**
 * What a decision on a new-change proposal does after it lands, and what a
 * refusal says (`../../../specifications/ui/OCW-openspec-change-workspace.md`
 * OCW-FR-BGDN, OCW-FR-IJAI, OCW-FR-ANKT).
 */
import * as api from "../../api";
import { logInfo, logWarn } from "../../logging";
import { requestDiscussionFoot } from "../../state/discussionSession";
import { noteNewChangeChanged } from "../../state/openspec/newChangeProposals";
import { showToast } from "../../state/toasts";
import {
  NEW_CHANGE_ERRORS,
  type NewChangeDecisionOutcome,
  type NewChangeProposal,
} from "../../types";
import { dispatchDecision } from "../DraftDiscussion/decisionDispatch";
import { refusalCode, runHoldingStream } from "./decisions";

/** OCW-FR-BGDN: a typed refusal of a new-change decision or of a confirmation, in words. */
export function newChangeMessage(code: string): string {
  const E = NEW_CHANGE_ERRORS;
  if (code.includes(E.alreadyDecided)) return "This proposed change has already been decided.";
  if (code.includes(E.notFound)) return "This proposed change is no longer available.";
  if (code.includes(E.changeExists)) {
    return "A change of this name is already on the stream, so it was not created. Nothing was changed.";
  }
  if (code.includes(E.changeNotFound)) {
    return "The original change is no longer on its stream. Nothing was changed.";
  }
  if (code.includes(E.unknownStream)) return "The work stream is gone. Nothing was changed.";
  if (code.includes(E.splitConfirmed)) {
    return "The split of this change is confirmed, so nothing more is decided in it.";
  }
  if (code.includes(E.pending)) {
    return "A proposed change is still waiting for a decision. Decide it first.";
  }
  if (code.includes(E.nothingAccepted)) {
    return "No proposed change is accepted, so there is no split to confirm.";
  }
  if (code.includes(E.discussionResolved)) {
    return "The discussion this proposal was posted in is resolved. Reopen it to decide. Nothing was changed.";
  }
  if (code.includes(E.writeFailed)) return "The files could not be written. Nothing was changed.";
  return "The proposed change could not be decided. Nothing was changed.";
}

/**
 * OCW-FR-IJAI / CEP-FR-ESQR: the statement of `stream_busy` on a card, with
 * the name of the run that holds the stream where it is known.
 */
export async function newChangeBusyMessage(streamId: string): Promise<string> {
  const title = await runHoldingStream(streamId);
  const who = title !== "" ? `The run “${title}”` : "A graduation run";
  return `${who} holds this stream, so the change was not created. Try again when the run stops working.`;
}

/**
 * OCW-FR-IJAI: Retry of an acceptance whose commit failed. It invokes
 * `"complete new change decision"`, and a commit that fails again raises the
 * toast again.
 */
export function retryNewChangeCommit(proposal: NewChangeProposal): void {
  void api.completeNewChangeDecision(proposal.id).then(
    (outcome) => {
      noteNewChangeChanged({
        streamId: outcome.proposal.streamId,
        change: outcome.proposal.change,
        proposal: outcome.proposal,
        cause: "committed",
      });
      raiseAcceptanceToast(outcome);
    },
    (e: unknown) => {
      const code = refusalCode(e);
      logWarn(["frontend"], "the commit of a new change failed again", {
        streamId: proposal.streamId,
        proposalId: proposal.id,
        error: code.slice(0, 80),
      });
      // A proposal that is gone or owes nothing has no commit to retry.
      if (code.includes(NEW_CHANGE_ERRORS.notFound) || code.includes(NEW_CHANGE_ERRORS.alreadyDecided)) return;
      raiseAcceptanceToast({
        proposal: { ...proposal, commitOwed: true },
        committed: false,
        originKind: "change_discussion",
      });
    },
  );
}

/**
 * OCW-FR-IJAI: an acceptance whose commit failed raises an Error toast with
 * Retry. An acceptance whose commit landed raises an Info toast that names the
 * new change.
 */
export function raiseAcceptanceToast(outcome: NewChangeDecisionOutcome): void {
  const { proposal } = outcome;
  if (proposal.state !== "accepted") return;
  const key = `openspec-new-change-commit:${proposal.id}`;
  const name = proposal.createdChange ?? proposal.name;
  if (outcome.committed) {
    showToast({
      key,
      level: "Info",
      title: `Change ${name} added`,
      body: "Its files are in one commit on the stream branch.",
      address: null,
    });
    return;
  }
  if (!proposal.commitOwed) return;
  logWarn(["frontend"], "a new change was written and its commit failed", {
    streamId: proposal.streamId,
    proposalId: proposal.id,
  });
  showToast(
    {
      key,
      level: "Error",
      title: `Change ${name} is not committed`,
      body: "Its files are written on the stream, but the commit failed.",
      address: null,
      link: { label: "Retry", onActivate: () => retryNewChangeCommit(proposal) },
    },
    { priority: true },
  );
}

/**
 * OCW-FR-BGDN: a decision landed. The store takes the record at once, the
 * discussion follows the decision comment, a decision with feedback tells the
 * agents of the conversation, and an acceptance raises its toast.
 */
export function settleNewChangeDecision(outcome: NewChangeDecisionOutcome, feedback: string): void {
  const { proposal } = outcome;
  noteNewChangeChanged({
    streamId: proposal.streamId,
    change: proposal.change,
    proposal,
    cause: "decided",
  });
  logInfo(["frontend"], "a proposed change was decided", {
    streamId: proposal.streamId,
    proposalId: proposal.id,
    state: proposal.state,
    committed: outcome.committed,
    withFeedback: feedback !== "",
  });
  requestDiscussionFoot(proposal.threadId);
  // DCR-FR-NMJH terms: a decision with feedback dispatches, one without does not.
  if (feedback !== "" && outcome.commentId) {
    void dispatchDecision(
      {
        id: proposal.id,
        threadId: proposal.threadId,
        target: { kind: "change", streamId: proposal.streamId, change: proposal.change },
      },
      { commentId: outcome.commentId },
      undefined,
      [],
      feedback,
    ).catch((e: unknown) => {
      logWarn(["ai", "frontend"], "could not tell the agents what was decided", {
        proposalId: proposal.id,
        error: refusalCode(e).slice(0, 80),
      });
    });
  }
  raiseAcceptanceToast(outcome);
}
