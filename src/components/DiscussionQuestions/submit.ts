/**
 * Sending a set of answers, and telling the discussion's agents about it
 * (`../../../specifications/ui/DQA-discussion-question-answering.md`
 * DQA-FR-KDVU, DQA-FR-CIRK).
 *
 * The submission is **one backend operation** (DQA-FR-KDVU): the whole ordered
 * set goes in one call, the comments are appended and the pending set deleted in
 * one committed step, and this surface never appends a comment of its own.
 *
 * The dispatch that follows is **one routing batch** (DQA-FR-CIRK). The
 * discussion's active agents are resolved **once**, after the append has
 * committed, and exactly one fresh turn goes to each of them for the whole
 * submission, naming the **final answer comment** as its trigger. A set of ten
 * questions therefore costs one turn per active agent rather than ten
 * (DQA-FR-QMEA), and the answers reach the agents as fresh later turns rather
 * than by resuming the turn that asked, which ended before there was an answer
 * (ADQ-FR-ZXAF).
 *
 * Modelled on `../DraftDiscussion/decisionDispatch.ts`, which dispatches on
 * exactly these terms for exactly this reason.
 */
import { originFor } from "../../state/discussionOrigin";
import {
  dispatchAgentTurn,
  listProjectAgents,
  submitDiscussionQuestionAnswers,
} from "../../api";
import { logInfo, logWarn } from "../../logging";
import { publishThread } from "../../state/conversationThreads";
import { publishSet } from "../../state/questionSets";
import { setDiscussionTurnFailure } from "../../state/discussionSession";
import { loggableTurnFailure } from "../CommentRail/messages";
import { activeAgents } from "../../state/activeAgents";
import { agentRoster } from "../agentTags";
import type {
  Discussion,
  ProjectAgent,
  QuestionAnswerInput,
} from "../../types";

/**
 * DQA-FR-KDVU: submit the whole ordered set, then dispatch the routing batch.
 *
 * Rejects where the backend refused, so the block renders the typed error at its
 * foot and keeps everything the author entered (DQA-FR-WKTP). Everything after
 * the append has committed is best-effort and never re-raised: the comments are
 * committed and the set is gone, so a dispatch that failed is a conversation
 * that stalls rather than answers the author has to enter again (DQA-FR-XBTL).
 */
export async function submitQuestionAnswers(
  threadId: string,
  setId: string,
  answers: QuestionAnswerInput[],
): Promise<void> {
  const submitted = await submitDiscussionQuestionAnswers({
    discussionId: threadId,
    setId,
    answers,
  });

  // DQA-FR-NKAX: the appended comments are ordinary comments of the conversation
  // from this moment. Published rather than waited for: the command's response
  // and the `"discussion changed"` event travel the same bridge with no
  // ordering between them.
  publishThread(submitted.discussion);
  // DQA-FR-IPFD: the block and its unsent draft go together.
  publishSet(threadId, null);

  await dispatchAnswerBatch(submitted.discussion, submitted.finalAnswerCommentId);
}

/**
 * DQA-FR-CIRK: one routing decision and one turn per active agent, for the whole
 * submission.
 *
 * The conversation is taken from what the submission returned rather than
 * re-read: it is the thread as it folds with every appended comment in it, which
 * is exactly what the active set must be resolved against. The roster is read
 * where this surface holds none, for the reason a decision reads one — an empty
 * roster resolves no tag at all, so a submission that should have reached the
 * room would quietly reach nobody.
 */
async function dispatchAnswerBatch(
  thread: Discussion,
  finalAnswerCommentId: string,
): Promise<void> {
  const roster = agentRoster([...(await currentRoster())]);
  // Resolved **once**, over the conversation with the whole submission in it.
  const targets = activeAgents(thread, roster);
  if (targets.length === 0) {
    // An ordinary outcome rather than a failure. Recorded because a submission
    // that told nobody is otherwise indistinguishable from one whose dispatch
    // was lost.
    // ADQ-FR-LZHV: no question, option, note, or comment body.
    logInfo(["ai", "frontend"], "a question submission reached no agent", {
      threadId: thread.id,
      enrolled: roster.nicknames.length,
    });
    return;
  }
  const origin = originFor(thread);
  // CTA-FR-UUXA: the submission retires the refusal an earlier dispatch met,
  // once for the whole batch, so one agent's acceptance does not hide another's
  // refusal. The pending contribution comes from the registration event
  // (CVP-FR-47).
  setDiscussionTurnFailure(thread.id, undefined);
  for (const nickname of targets) {
    // DQA-FR-QMEA: one turn per agent for the whole set, naming the final
    // answer as its trigger — so the turn's input carries every earlier
    // question and answer in `discussion_history` and the last answer in
    // `current_comment`.
    void dispatchAgentTurn({
      nickname,
      origin,
      triggerCommentId: finalAnswerCommentId,
    }).catch(
      (error: unknown) => {
        // DQA-FR-XBTL: nothing is rolled back. One agent's failure neither
        // suppresses another's turn nor repeats it. The refusal renders at the
        // foot of the discussion (CTA-FR-UUXA).
        const raw = errorText(error);
        setDiscussionTurnFailure(thread.id, raw);
        logWarn(["ai", "frontend"], "could not tell an agent the answers", {
          threadId: thread.id,
          nickname,
          failure: loggableTurnFailure(raw),
        });
      },
    );
  }
}

function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
}

/** The project's enrolment, for a submission taken before any surface published it. */
async function currentRoster(): Promise<readonly ProjectAgent[]> {
  return listProjectAgents()
    .then((enrolled) => (Array.isArray(enrolled) ? enrolled : []))
    .catch(() => []);
}
