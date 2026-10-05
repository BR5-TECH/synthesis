/**
 * A conversation tab (`TAB-tabs.md` TAB-FR-QXMV,
 * `CVP-conversation-presentation.md` CVP-FR-54, CVP-FR-57, CVP-FR-60).
 *
 * A regular tab of the main viewport that hosts the shared discussion surface
 * with owner `"tab"`. It opens for a note's discussion and for a discussion
 * whose owner no longer resolves. For a note that has no discussion yet it hosts
 * the shared opening composer alone: mounting it creates nothing, and the
 * author's post is what creates the discussion. Posting turns this same tab
 * into the tab of the created discussion, in place and without a remount.
 *
 * It owns no state of the conversation. The unsent text, the attachments, the
 * scroll position, the unread state, and the pending placeholders are in the
 * session stores, so closing the tab and opening it again finds all of them
 * (CVP-FR-47).
 */
import { useEffect, useRef } from "react";

import * as api from "../api";
import { DiscussionComposer, DiscussionSurface } from "./discussion";
import type { OpenDiscussionRequest } from "./discussion";
import { identityBlockFor } from "../hooks/useComments";
import {
  dispatchTurnsFor,
  useConversationSession,
} from "../hooks/useConversationSession";
import { logInfo } from "../logging";
import {
  publishThread,
  threadIsMissing,
} from "../state/conversationThreads";
import { announceDiscussion } from "../state/discussionAnnouncer";
import { consumePendingFocus } from "../state/discussionFocus";
import { useOwnerAvailable } from "../state/ownerAvailability";
import {
  discussionTargetKey,
  type Discussion,
  type DiscussionTarget,
  type Tab,
} from "../types";

export function ConversationTabView({
  tab,
  onOpened,
}: {
  tab: Tab;
  /** CVP-FR-60: the opening post created this discussion. */
  onOpened: (tabId: string, discussion: Discussion) => void;
}) {
  const threadId = tab.threadId ?? null;
  const session = useConversationSession(threadId, tab.ownerTarget);
  const rootRef = useRef<HTMLDivElement>(null);
  const openedBody = useRef("");

  const thread = session.thread;
  const target: DiscussionTarget | undefined = thread?.target ?? tab.ownerTarget;
  const available = useOwnerAvailable(target);
  const ownerLabel = tab.ownerLabel ?? "";
  const name = tab.label;
  const subject = tab.subject ?? "";
  const identityBlock = identityBlockFor(session.identityError);

  // CVP-FR-43: an owner becoming unavailable while the tab is open is announced.
  const wasAvailable = useRef(available);
  useEffect(() => {
    if (wasAvailable.current && !available) {
      announceDiscussion(`${tab.label}: the thing it is about is no longer in the project.`);
    }
    wasAvailable.current = available;
  }, [available, tab.label]);

  // CVP-FR-42: a reveal that ran before this tab mounted left a request. The
  // surface consumes it for a discussion. The opening composer takes it here.
  const opening = !threadId && tab.noteId !== undefined;
  useEffect(() => {
    if (!opening || tab.noteId === undefined) return;
    const wanted = consumePendingFocus(
      discussionTargetKey({ kind: "note", noteId: tab.noteId }),
    );
    if (!wanted) return;
    rootRef.current?.querySelector("textarea")?.focus();
  }, [opening, tab.noteId]);

  /**
   * NTS-FR-27 / CMS-FR-62: the one operation that creates a note's discussion.
   * It is atomic and idempotent. Where the note already has a discussion, it
   * returns that one and appends nothing, so the message is appended as an
   * ordinary comment instead of being dropped.
   */
  const open = async (request: OpenDiscussionRequest): Promise<Discussion> => {
    const noteId = tab.noteId as string;
    logInfo(["frontend"], "opening a note discussion", { noteId });
    let created = await api.getOrCreateNoteDiscussion({
      noteId,
      body: request.body,
      attachments: request.attachments,
    });
    const justOpened =
      created.comments.length === 1 && created.comments[0]?.body === request.body;
    if (!justOpened) {
      created = await api.addComment({
        noteId,
        discussionId: created.id,
        body: request.body,
        quotes: [],
        attachments: request.attachments,
      });
    }
    openedBody.current = request.body;
    return created;
  };

  const opened = (created: Discussion) => {
    publishThread(created);
    onOpened(tab.id, created);
    dispatchTurnsFor(created, openedBody.current, session.agents);
  };

  return (
    <div
      ref={rootRef}
      className="conversation-tab"
      tabIndex={-1}
      aria-label={name}
      data-testid="conversation-tab-view"
      data-thread-id={threadId ?? ""}
    >
      <div className="conversation-tab__header">
        <div className="conversation-tab__heading">
          {/* CVP-FR-57: the chat, named by what it is about. */}
          <div className="conversation-tab__title" title={name}>
            {name}
          </div>
          <div className="conversation-tab__subtitle">
            <span title={subject}>❝ {subject}</span>
            {thread && (
              <span>
                {thread.resolved ? "Resolved" : thread.locked ? "Locked" : "Active"}
              </span>
            )}
          </div>
        </div>
      </div>

      <div className="conversation-tab__body">
        {thread ? (
          <DiscussionSurface
            discussion={thread}
            owner="tab"
            variant="embedded"
            agents={session.agents}
            identity={session.identity}
            identityBlock={identityBlock}
            disabled={identityBlock !== null}
            availability={available ? "available" : "unavailable"}
            ownerLabel={ownerLabel}
            onReply={session.reply}
            onSetLock={session.setLock}
            onSetResolved={session.setResolved}
          />
        ) : opening && tab.ownerTarget ? (
          <div className="note-opener" data-testid="note-discussion-opener">
            <div className="note-opener__empty">
              {identityBlock ? (
                <div className="comment-rail__blocked" role="status">
                  {identityBlock.message}
                  {identityBlock.route && (
                    <div className="comment-rail__route">{identityBlock.route}</div>
                  )}
                </div>
              ) : (
                <p className="note-opener__lead">
                  Say what you want to discuss about this note.
                  <br />
                  The conversation starts when you send it.
                </p>
              )}
            </div>
            <div className="comment-card__reply">
              <DiscussionComposer
                mode="opening"
                target={tab.ownerTarget}
                agents={session.agents}
                disabled={identityBlock !== null}
                ariaLabel="Start a discussion about this note"
                placeholder="What about this note?"
                onOpen={open}
                onOpened={opened}
              />
            </div>
          </div>
        ) : (
          <div className="conversation-tab__loading" role="status">
            {threadId && threadIsMissing(threadId)
              ? "This conversation could not be read."
              : "Loading the conversation…"}
          </div>
        )}
      </div>
    </div>
  );
}
