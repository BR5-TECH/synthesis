/**
 * The shell's half of the discussion route
 * (`CVP-conversation-presentation.md` CVP-FR-06, CVP-FR-54,
 * `TAB-tabs.md` TAB-FR-23, TAB-FR-QXMV).
 *
 * `revealDiscussion` (`../../state/revealDiscussion.ts`) decides where a
 * discussion goes. The closures here do the shell work for each branch: open or
 * focus a conversation tab, open the owner surface of an artifact or a draft,
 * bind a note's tab to the discussion its opening post created, and close the
 * conversation tabs that a deletion makes meaningless.
 *
 * Plain closures rather than a hook, like the other groups under `./`.
 */
import type { Dispatch, MutableRefObject, SetStateAction } from "react";

import { logDebug } from "../../logging";
import {
  clearDiscussionSession,
  getDiscussionSession,
} from "../../state/discussionSession";
import type { EditSessionStore } from "../../state/editSessions";
import {
  revealDiscussion as route,
  revealKey,
  type DiscussionReveal,
  type RevealOutcome,
} from "../../state/revealDiscussion";
import {
  discussionTargetKey,
  type Discussion,
  type OpenableArtifact,
  type Tab,
} from "../../types";
import { conversationTabId, neverEmpty, railSurfaceFor } from "./tabRecords";

export interface DiscussionActionDeps {
  tabsRef: MutableRefObject<Tab[]>;
  setTabs: Dispatch<SetStateAction<Tab[]>>;
  setActiveTab: Dispatch<SetStateAction<string>>;
  activateTab: (id: string) => void;
  openArtifact: (item: OpenableArtifact) => void;
  openDraft: (draft: { id: string; name: string }) => void;
  sessions: EditSessionStore;
  setFocusThread: (
    thread: { artifactId: string; threadId: string } | null,
  ) => void;
}

export interface DiscussionActions {
  /** The one route every reveal calls. */
  revealDiscussion: (reveal: DiscussionReveal) => RevealOutcome;
  /** Turn a note's opening tab into the tab of the discussion it created. */
  bindConversationTab: (tabId: string, discussion: Discussion) => void;
  /** NTS-FR-30 / CVP-FR-61: the note is gone, so its tab is too. */
  dropNoteConversationTab: (noteId: string) => void;
}

function basename(path: string): string {
  const cut = path.lastIndexOf("/");
  return cut === -1 ? path : path.slice(cut + 1);
}

/**
 * CVP-FR-57: the one name of a conversation, `Chat: <owner>`. An artifact is
 * named by its file name, as its Editor tab is.
 */
export function chatName(reveal: DiscussionReveal): string {
  const owner =
    reveal.target.kind === "artifact"
      ? basename(reveal.ownerLabel)
      : reveal.ownerLabel;
  return `Chat: ${owner}`;
}

/** Whether a conversation tab is the one showing this discussion. */
export function tabShows(tab: Tab, reveal: DiscussionReveal): boolean {
  if (tab.kind !== "conversation") return false;
  if (reveal.discussionId && tab.threadId === reveal.discussionId) return true;
  return (
    reveal.target.kind === "note" &&
    tab.noteId !== undefined &&
    tab.noteId === reveal.target.noteId
  );
}

export function createDiscussionActions(
  deps: DiscussionActionDeps,
): DiscussionActions {
  const {
    tabsRef,
    setTabs,
    setActiveTab,
    activateTab,
    openArtifact,
    openDraft,
    sessions,
    setFocusThread,
  } = deps;

  const findConversationTab = (reveal: DiscussionReveal): string | null =>
    tabsRef.current.find((t) => tabShows(t, reveal))?.id ?? null;

  /**
   * TAB-FR-QXMV: open the conversation tab at the end of the strip, or focus the
   * one that already shows the discussion. The test runs inside the updater, so
   * two requests landing before a render leave one tab (TAB-FR-23).
   */
  const openConversationTab = (reveal: DiscussionReveal) => {
    const label = chatName(reveal);
    const fresh: Tab = {
      id: conversationTabId(revealKey(reveal)),
      label,
      tooltip: reveal.subject,
      kind: "conversation",
      threadId: reveal.discussionId,
      noteId: reveal.target.kind === "note" ? reveal.target.noteId : undefined,
      ownerTarget: reveal.target,
      ownerLabel: reveal.ownerLabel,
      subject: reveal.subject,
    };
    setTabs((ts) => {
      const held = ts.find((t) => tabShows(t, reveal));
      if (!held) return [...ts, fresh];
      const next: Tab = {
        ...held,
        label,
        tooltip: reveal.subject,
        threadId: held.threadId ?? reveal.discussionId,
        ownerTarget: reveal.target,
        ownerLabel: reveal.ownerLabel,
        subject: reveal.subject,
      };
      const same =
        next.label === held.label &&
        next.tooltip === held.tooltip &&
        next.threadId === held.threadId &&
        next.ownerLabel === held.ownerLabel &&
        next.subject === held.subject;
      return same ? ts : ts.map((t) => (t === held ? next : t));
    });
    const existing = findConversationTab(reveal);
    activateTab(existing ?? fresh.id);
  };

  /**
   * CMP-FR-10 / CMP-FR-11 / CMT-FR-36: open the owner and land on the
   * discussion. The rail's surface and open state are written into the
   * artifact's session before the tab opens, so the Editor mounts already
   * showing the rail. The pending focus the route already holds is what lands
   * focus when the discussion's surface mounts.
   */
  const openOwner = (reveal: DiscussionReveal) => {
    const { target } = reveal;
    if (target.kind === "artifact") {
      const id = target.artifactId;
      sessions.update(id, {
        ...railSurfaceFor(id),
        railOpen: true,
        ...(reveal.resolved ? { resolvedOpen: true } : {}),
      });
      openArtifact({ id, name: basename(id) });
      if (reveal.discussionId) {
        setFocusThread({ artifactId: id, threadId: reveal.discussionId });
      }
      return;
    }
    if (target.kind === "draft") {
      openDraft({ id: target.draftId, name: reveal.ownerLabel });
    }
  };

  const revealDiscussion = (reveal: DiscussionReveal): RevealOutcome =>
    route(reveal, { findConversationTab, openConversationTab, openOwner });

  /**
   * CVP-FR-60: the opening post succeeded. The same tab now shows the discussion
   * the backend returned, in place: its id stays, so the tab does not remount.
   */
  const bindConversationTab = (tabId: string, discussion: Discussion) => {
    setTabs((ts) =>
      ts.map((t) =>
        t.id === tabId && t.threadId !== discussion.id
          ? { ...t, threadId: discussion.id }
          : t,
      ),
    );
  };

  const closeWhere = (matches: (t: Tab) => boolean) => {
    let remaining: Tab[] = [];
    setTabs((ts) => {
      remaining = neverEmpty(ts.filter((t) => !matches(t)));
      tabsRef.current = remaining;
      return remaining;
    });
    // SNV-FR-65: a removal, not an activation.
    setActiveTab((current) =>
      remaining.some((t) => t.id === current)
        ? current
        : remaining[remaining.length - 1].id,
    );
  };

  /**
   * NTS-FR-30 / CVP-FR-61: the backend reported the note and its discussion
   * deleted. The tab goes, and so does what the session store held for it. The
   * caller runs this on success alone.
   */
  const dropNoteConversationTab = (noteId: string) => {
    const doomed = tabsRef.current.filter(
      (t) => t.kind === "conversation" && t.noteId === noteId,
    );
    for (const tab of doomed) {
      if (tab.threadId) clearDiscussionSession(tab.threadId);
    }
    clearDiscussionSession(discussionTargetKey({ kind: "note", noteId }));
    logDebug(["frontend"], "a deleted note's conversation tab was closed", {
      noteId,
      tabs: doomed.length,
    });
    closeWhere((t) => t.kind === "conversation" && t.noteId === noteId);
  };

  return {
    revealDiscussion,
    bindConversationTab,
    dropNoteConversationTab,
  };
}

/**
 * TAB-FR-25 / CVP-FR-60: what closing a conversation tab discards. A tab of an
 * existing discussion discards nothing: the session store keeps the text, the
 * attachments, the scroll position, and the unread state. A note's opening tab
 * has no discussion, so its unsent text and attachments go with it.
 */
export function forgetClosedConversationTab(tab: Tab | undefined): void {
  if (!tab || tab.kind !== "conversation" || tab.threadId || !tab.noteId) return;
  const key = discussionTargetKey({ kind: "note", noteId: tab.noteId });
  if (getDiscussionSession(key).body !== "" || getDiscussionSession(key).attachments.length > 0) {
    logDebug(["frontend"], "an unposted opening message was discarded", {
      noteId: tab.noteId,
    });
  }
  clearDiscussionSession(key);
}
