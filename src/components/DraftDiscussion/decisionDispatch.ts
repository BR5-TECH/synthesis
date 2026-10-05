/**
 * Telling the conversation what the author decided about a proposed change
 * (`../../../specifications/ui/DCR-draft-change-review.md` DCR-FR-15).
 *
 * It moved here with the review itself: the decisions are taken in the document
 * column now, but what a decision owes the conversation has not changed at all
 * — and neither has the reason it is a module of its own. The one thing this
 * redesign does change is *when* it runs: a proposal is decided one change at a
 * time, and only the decision that leaves nothing undecided appends a comment
 * (DCP-FR-15), so only that decision has anything for a turn to answer.
 */
import { dispatchAgentTurn, listProjectAgents } from "../../api";
import { logInfo, logWarn } from "../../logging";
import { originFor } from "../../state/discussionOrigin";
import { loggableTurnFailure } from "../CommentRail/messages";
import { refreshThread } from "../../state/conversationThreads";
import { agentRoster } from "../agentTags";
import { decisionTargets } from "../../state/activeAgents";
import type {
  ConversationOrigin,
  Discussion,
  DecisionOutcome,
  DraftChangeProposal,
  ProjectAgent,
} from "../../types";

/**
 * DCR-FR-15: tell the conversation what was decided, on **exactly the terms any
 * human comment in it dispatches on** (per `CMT-comments.md` CTA-FR-XBIN,
 * CTA-FR-SSUZ).
 *
 * The decision's comment is one the author wrote (DCP-FR-15), so a turn goes to
 * each distinct agent of the conversation's active set as it stands with that
 * comment appended, and none at all where that set is empty. A decision carrying
 * feedback that names agents is a tagged comment like any other and reaches
 * exactly those; a decision carrying none is an untagged comment and reaches
 * whoever the author was last addressing there. There is no route by which a
 * proposal's own agent is dispatched to **for having proposed** — a decision is
 * a comment in a conversation, and a conversation reaches the people it is being
 * held with.
 *
 * Both inputs are read **again** here rather than taken from what this surface
 * happened to be holding, and both readings matter. The conversation must carry
 * the decision's own comment for its feedback to settle the set, and the
 * `"discussion changed"` event that would have delivered it has no ordering
 * against the decision call's own response. The roster must be the project's as
 * it stands, and this modal can be reached without any surface having published
 * one — an empty roster resolves no tag at all, which would silently send the
 * decision to nobody. A read that fails falls back to what was held, which is
 * `decisionTargets`' second reading.
 *
 * Best-effort and never surfaced: the decision has already landed, and an agent
 * that could not be re-dispatched is a conversation that stalls rather than a
 * change the author has to make again. It is logged, because a stalled
 * conversation is otherwise invisible to anyone debugging it. One agent's
 * failure neither suppresses another's turn nor repeats it (CMT-FR-79).
 *
 * Nothing is dispatched when the decision's comment could not be appended —
 * there is then no comment for a turn to answer.
 */
export async function dispatchDecision(
  proposal: DraftChangeProposal,
  outcome: DecisionOutcome,
  held: Discussion | undefined,
  agents: readonly ProjectAgent[],
  feedback: string,
): Promise<void> {
  if (outcome.commentId === undefined) return;
  const [refreshed, enrolled] = await Promise.all([
    refreshThread(proposal.threadId),
    agents.length > 0 ? agents : currentRoster(),
  ]);
  const thread = refreshed ?? held;
  const roster = agentRoster([...enrolled]);
  const targets = decisionTargets(thread, outcome.commentId, feedback, roster);
  if (targets.length === 0) {
    // DCR-FR-15: an ordinary outcome rather than a failure — the decision is not
    // refused or retried on this account. Recorded because a decision that told
    // nobody is otherwise indistinguishable from one whose dispatch was lost.
    logInfo(["ai", "frontend"], "a decision reached no agent", {
      proposalId: proposal.id,
      threadId: proposal.threadId,
      conversationRead: thread === undefined ? "unavailable" : "read",
      enrolled: roster.nicknames.length,
    });
    return;
  }
  // AGC-FR-05: the held thread's own origin. A thread that could not be read
  // still names its discussion and owner, and the backend reads the target and
  // the fragment target again from the discussion that id names.
  const origin: ConversationOrigin = thread
    ? originFor(thread)
    : {
        discussionId: proposal.threadId,
        target: { kind: "draft", draftId: proposal.draftId },
        fragmentTarget: null,
      };
  for (const nickname of targets) {
    void dispatchAgentTurn({
      nickname,
      origin,
      triggerCommentId: outcome.commentId,
    }).catch((error: unknown) => {
      // Not surfaced (PCR-FR-14). The log carries the typed code only: the raw
      // error can quote the argument it refused (see `loggableTurnFailure`).
      const raw = error instanceof Error ? error.message : String(error);
      logWarn(["ai", "frontend"], "could not tell an agent what was decided", {
        proposalId: proposal.id,
        nickname,
        failure: loggableTurnFailure(raw),
      });
    });
  }
}

/**
 * The project's enrolment, for a decision taken before any surface published it.
 *
 * `useProjectAgents` is a read of a cache the chrome's own roster control fills
 * (AGT-FR-02); it never loads anything itself. A review opened from a
 * notification can therefore reach a decision while that cache is still empty,
 * and an empty roster resolves no tag at all — so a decision that should have
 * reached the whole room would quietly reach nobody. Read here rather than on
 * open, because the author can decide the moment the modal appears.
 */
async function currentRoster(): Promise<readonly ProjectAgent[]> {
  // A read that fails, or that answers with anything other than a list, is the
  // same thing here: no enrolment this decision can resolve a tag against. Both
  // resolve to an empty roster rather than to a raised error, because this whole
  // path is best-effort — see `dispatchDecision`.
  return listProjectAgents()
    .then((enrolled) => (Array.isArray(enrolled) ? enrolled : []))
    .catch(() => []);
}
