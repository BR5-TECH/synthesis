/**
 * What the conversation tab needs to continue a discussion
 * (`CVP-conversation-presentation.md` CVP-FR-33, CVP-FR-36).
 *
 * The tab renders a discussion with no owner layout behind it, so it needs the
 * discussion itself and the three operations a card invokes: reply, Lock, and
 * Resolve. Every one is the operation every owner invokes, and each one
 * dispatches turns by the conversation's own rules, so the agents a message
 * reaches do not depend on where it was typed.
 *
 * What the surface shows besides the discussion — the pending placeholders, the
 * failed contribution, the unread state — is read by the shared surface itself
 * from the session stores (`./useDiscussionRuntime.ts`), so this hook holds none
 * of it.
 *
 * The discussion comes from `../state/conversationThreads`, which keeps
 * `"read comment thread"` to a conversation no surface has listed.
 */
import { useCallback, useMemo, useRef } from "react";

import {
  addComment as addCommentApi,
  dispatchAgentTurn,
  setDiscussionLock,
  setDiscussionResolution,
} from "../api";
import { addressesEveryone, agentRoster } from "../components/agentTags";
import { logDebug, logWarn } from "../logging";
import { loggableTurnFailure } from "../components/CommentRail/messages";
import {
  dispatchTargets,
  withoutAwaitingReply,
} from "../state/activeAgents";
import { useProjectAgents } from "../state/agentRegistry";
import {
  publishThread,
  useConversationThread,
} from "../state/conversationThreads";
import { locatorOf, originFor } from "../state/discussionOrigin";
import {
  getDiscussionSession,
  setDiscussionError,
  setDiscussionTurnFailure,
  setDiscussionTurns,
  upsertDiscussionTurn,
} from "../state/discussionSession";
import { useSharedCommentIdentity } from "./useSharedCommentIdentity";
import type {
  AttachmentInput,
  CommentQuote,
  Discussion,
  DiscussionTarget,
  Participant,
  ProjectAgent,
} from "../types";

function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
}

export { originFor };

/**
 * CTA-FR-QUXJ / CTA-FR-DGOC: dispatch a turn to each agent a message addresses.
 *
 * For a reply the active set is read from the discussion's own comments. For a
 * discussion that has just been opened, the message has no earlier comment, so
 * it reaches the agents it names and nobody where it names none. The same rule
 * serves both. Never logs the body: it is user content.
 */
export function dispatchTurnsFor(
  discussion: Discussion,
  body: string,
  agents: readonly ProjectAgent[],
): void {
  const roster = agentRoster([...agents]);
  const targets = dispatchTargets(discussion, body, roster);
  if (targets.length === 0) {
    logDebug(["ai", "frontend"], "a comment reached no agent", {
      threadId: discussion.id,
      enrolled: roster.nicknames.length,
    });
    return;
  }
  const triggerCommentId =
    discussion.comments[discussion.comments.length - 1]?.id;
  if (!triggerCommentId) return;
  const origin = originFor(discussion);
  if (addressesEveryone(body, roster)) {
    // AGT-FR-35: what a reader asking "why three answers?" needs.
    logDebug(["ai", "frontend"], "a comment addressed every agent that can answer", {
      threadId: discussion.id,
      agents: targets.join(" "),
      count: targets.length,
    });
  }
  // CTA-FR-JQDM / AGC-FR-29: dispatching ends the wait, so the surface that
  // caused it forgets it.
  setDiscussionTurns(
    discussion.id,
    withoutAwaitingReply(
      getDiscussionSession(discussion.id).turns,
      targets,
      discussion.id,
    ),
  );
  // CTA-FR-UUXA: a new message retires the refusal the last one met, once for
  // the whole batch, so one agent's acceptance does not hide another's refusal.
  setDiscussionTurnFailure(discussion.id, undefined);
  for (const nickname of targets) {
    void dispatchAgentTurn({ nickname, origin, triggerCommentId }).then(
      (turn) => {
        // Upsert: the backend publishes the registration event before the call
        // resolves, with no ordering between the two.
        upsertDiscussionTurn(discussion.id, turn);
      },
      (e: unknown) => {
        // CTA-FR-UUXA: the refusal renders at the foot of the discussion. The
        // log carries the typed code only (see `loggableTurnFailure`).
        setDiscussionTurnFailure(discussion.id, errorText(e));
        logWarn(["ai", "frontend"], "an agent turn could not be dispatched", {
          threadId: discussion.id,
          nickname,
          failure: loggableTurnFailure(errorText(e)),
        });
      },
    );
  }
}

export interface ConversationSession {
  thread: Discussion | undefined;
  identity: Participant | null;
  identityError: string | null;
  agents: readonly ProjectAgent[];
  reply: (
    discussionId: string,
    body: string,
    quotes: CommentQuote[],
    attachments: AttachmentInput[],
  ) => Promise<void>;
  setLock: (discussionId: string, locked: boolean) => Promise<void>;
  setResolved: (discussionId: string, resolved: boolean) => Promise<void>;
}

export function useConversationSession(
  threadId: string | null,
  target: DiscussionTarget | undefined,
): ConversationSession {
  const thread = useConversationThread(threadId);
  const agents = useProjectAgents();
  // The identity is project-wide and shared, so it is never read again here.
  const { identity, identityError } = useSharedCommentIdentity();

  const agentsRef = useRef<readonly ProjectAgent[]>(agents);
  agentsRef.current = agents;
  const locator = useMemo(() => (target ? locatorOf(target) : {}), [target]);

  const reply = useCallback(
    async (
      discussionId: string,
      body: string,
      quotes: CommentQuote[],
      attachments: AttachmentInput[],
    ) => {
      try {
        const updated = await addCommentApi({
          ...locator,
          discussionId,
          body,
          quotes,
          attachments,
        });
        setDiscussionError(discussionId, undefined);
        publishThread(updated);
        dispatchTurnsFor(updated, body, agentsRef.current);
      } catch (e) {
        // CMT-FR-34 / CMT-FR-51: the typed error renders inline and the body and
        // every pending attachment stay, so the message is corrected and sent
        // without being retyped. The rejection tells the composer not to clear.
        setDiscussionError(discussionId, errorText(e));
        throw e;
      }
    },
    [locator],
  );

  const setLock = useCallback(
    async (discussionId: string, locked: boolean) => {
      try {
        const updated = await setDiscussionLock({ ...locator, discussionId, locked });
        setDiscussionError(discussionId, undefined);
        publishThread(updated);
      } catch (e) {
        setDiscussionError(discussionId, errorText(e));
      }
    },
    [locator],
  );

  const setResolved = useCallback(
    async (discussionId: string, resolved: boolean) => {
      try {
        const updated = await setDiscussionResolution({
          ...locator,
          discussionId,
          resolved,
        });
        setDiscussionError(discussionId, undefined);
        publishThread(updated);
      } catch (e) {
        setDiscussionError(discussionId, errorText(e));
      }
    },
    [locator],
  );

  return { thread, identity, identityError, agents, reply, setLock, setResolved };
}
