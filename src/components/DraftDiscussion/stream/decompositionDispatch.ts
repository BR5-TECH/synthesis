/**
 * The agent turns a decomposition dispatches (`DCR-draft-change-review.md`
 * DCR-FR-NMJH, DCR-FR-UVMY).
 *
 * Best-effort, as every decision dispatch is: a turn that could not be
 * dispatched is logged and changes no decision.
 */
import { dispatchAgentTurn, listProjectAgents } from "../../../api";
import { logInfo, logWarn } from "../../../logging";
import { activeAgents } from "../../../state/activeAgents";
import { refreshThread } from "../../../state/conversationThreads";
import { originFor } from "../../../state/discussionOrigin";
import type { NewDraftProposal } from "../../../types";
import { agentRoster } from "../../agentTags";
import { dispatchDecision } from "../decisionDispatch";

/** DCR-FR-NMJH: a decision with feedback dispatches, one without does not. */
export function dispatchAfterDecision(
  proposal: NewDraftProposal,
  commentId: string | null | undefined,
  feedback: string,
): void {
  if (feedback.trim() === "" || !commentId) return;
  void dispatchDecision(
    { id: proposal.id, threadId: proposal.threadId, draftId: proposal.draftId },
    { commentId },
    undefined,
    [],
    feedback,
  );
}

/**
 * DCR-FR-UVMY: Ask for next draft. One turn for each agent of the active set,
 * with the newest human comment (the decision comment) as the trigger, or to
 * the agent of the newest decided proposal where that set is empty.
 */
export async function askForNextDraft(newest: NewDraftProposal): Promise<number> {
  const [thread, enrolled] = await Promise.all([
    refreshThread(newest.threadId),
    listProjectAgents()
      .then((agents) => (Array.isArray(agents) ? agents : []))
      .catch(() => []),
  ]);
  if (!thread) return 0;
  const roster = agentRoster(enrolled);
  const trigger = [...thread.comments].reverse().find((c) => c.author.kind === "human");
  if (!trigger) return 0;
  let targets = activeAgents(thread, roster);
  if (targets.length === 0 && newest.agent.kind === "agent") {
    const handle = newest.agent.handle;
    if (roster.nicknames.includes(handle)) targets = [handle];
  }
  logInfo(["ai", "frontend"], "the next proposed draft was asked for", {
    proposalId: newest.id,
    agents: targets.length,
  });
  for (const nickname of targets) {
    void dispatchAgentTurn({
      nickname,
      origin: originFor(thread),
      triggerCommentId: trigger.id,
    }).catch((error: unknown) => {
      logWarn(["ai", "frontend"], "could not ask an agent for the next draft", {
        proposalId: newest.id,
        nickname,
        error: error instanceof Error ? error.message.slice(0, 80) : "unknown",
      });
    });
  }
  return targets.length;
}
