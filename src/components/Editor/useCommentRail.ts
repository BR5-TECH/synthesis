/**
 * The Editor's comment rail (`../../../specifications/ui/CMT-comments.md`,
 * hosted per EDT-FR-61): the two kinds of conversation it renders, where it
 * renders, and the per-thread channels each card reaches.
 *
 * Split out of the Editor whole because routing is the substance of it. The
 * Discussion section at the rail's head is held by the action control and the
 * aligned cards below it by `useComments`, and every channel below picks
 * between them by which kind the thread is — a rule that only stays correct
 * while all of it is read in one place.
 */
import { useCallback, useMemo, useRef, useState } from "react";
import { useComments } from "../../hooks/useComments";
import { useDiscussionControl } from "../../hooks/useDiscussionControl";
import { EditSessionStore } from "../../state/editSessions";
import type { EditSession } from "../../state/editSessions/types";
import type { EditMode } from "../../state/editHistory";
import type { UseArtifactDocument } from "../../hooks/useArtifactDocument";
import type {
  AttachmentInput,
  CommentQuote,
  DiscussionTarget,
} from "../../types";
import type { CommentSelection } from "./selection";

export interface CommentRailDeps {
  artifactId: string;
  sessions: EditSessionStore;
  session: EditSession;
  doc: UseArtifactDocument;
  mode: EditMode;
  isMarkdown: boolean;
  showComments: boolean;
  showActions: boolean;
  /**
   * CMT-FR-06: the artifact's Markdown source, which is what an anchor is a
   * range in — so the rail reads the same document a save would write.
   */
  currentDoc: () => string;
  /** Hiding the rail abandons a draft in flight rather than arming it unseen. */
  setDraft: (draft: CommentSelection | null) => void;
}

export function useCommentRail(deps: CommentRailDeps) {
  const {
    artifactId,
    sessions,
    session,
    doc,
    mode,
    isMarkdown,
    showComments,
    showActions,
    currentDoc,
    setDraft,
  } = deps;

  // ---- Comment rail (CMT-comments.md, hosted per EDT-FR-61) ---------------

  /**
   * CMT-FR-06: anchors are ranges in the artifact's **Markdown source**, so the
   * rail reads the same document a save would write — which is what makes an
   * anchor mean the same passage in either editing mode.
   */
  const commentSource = useCallback(() => currentDoc(), [currentDoc]);
  // CMT-FR-02: the rail belongs to WYSIWYG mode. Threads are still loaded in raw
  // text mode so the action cluster's count stays truthful (CMT-FR-29).
  const comments = useComments(artifactId, commentSource, showComments);

  /**
   * EDT-FR-64 / ACT-FR-02: the action control is bound to the open artifact or
   * plain text file. It is present in **both** editing modes, unlike the
   * comments, because starting a conversation about the file in front of the
   * author is not a thing the shape of the text on screen has any bearing on.
   */
  const discussionTarget = useMemo<DiscussionTarget>(
    () => ({ kind: "artifact", artifactId }),
    [artifactId],
  );
  // ACT-FR-22: the rail has already read the roster, the catalogue, and the
  // running turns — all three facts about the project rather than about one
  // conversation — so the control reuses them instead of reading them again.
  const control = useDiscussionControl(discussionTarget, showActions, undefined, {
    agents: comments.agents,
    allTurns: comments.allTurns,
  });
  /** ACT-FR-12: the rail's card menus and the control's surfaces are one set. */
  const [dismissControl, setDismissControl] = useState(0);

  /**
   * CMT-FR-30: the rail opens by default for an artifact carrying at least one
   * unresolved thread and stays closed for one carrying none.
   *
   * The default is *computed*, never written to the session record: only an
   * explicit toggle records a choice there, so an artifact that was merely opened
   * and read is still dropped when its tab closes (EDT-FR-28) rather than
   * retained on the strength of a default nobody chose.
   *
   * It is latched on the first load that has threads to judge by, rather than
   * recomputed as the count moves. Recomputing would close the rail the instant
   * the author resolved the last thread — pulling the surface out from under them
   * at exactly the moment they might want to reopen it.
   */
  const unresolvedInTab = comments.unresolved + control.discussions.unresolved;
  const railDefaultRef = useRef<boolean | null>(null);
  if (
    railDefaultRef.current === null &&
    doc.loaded &&
    (comments.threads.length > 0 || control.discussions.discussions.length > 0)
  ) {
    // CMT-FR-30 / CMT-FR-56: the rail opens for a tab carrying anything
    // unresolved — a discussion waiting on the author is waiting on them just as
    // a remark pinned to a passage is.
    railDefaultRef.current = unresolvedInTab > 0;
  }
  const railOpen =
    showComments && (session.railOpen ?? railDefaultRef.current ?? false);
  /**
   * CMT-FR-02: the rail renders wherever the Editor sets the file's text on its
   * page — a Markdown file's WYSIWYG surface, and the one surface a source file
   * has (ESH-FR-SSDV). A Markdown file toggled to raw text is the one place it
   * does not: that surface is the file's own syntax, and a card aligned to a
   * line of it would be aligned to a passage the author is reading as source.
   */
  const railOnSource = !isMarkdown;
  const railHere = isMarkdown ? mode === "wysiwyg" : true;
  const toggleRail = useCallback(() => {
    // Hiding the rail hides the composer with it, so a draft in flight is
    // abandoned rather than left armed where it cannot be seen or cancelled.
    if (railOpen) setDraft(null);
    sessions.update(artifactId, { railOpen: !railOpen });
  }, [artifactId, railOpen, sessions]);

  /**
   * ACT-FR-19 / ACT-FR-20: the discussions pin at the rail's head wherever the
   * tab lends that rail a margin, which is wherever the rail renders at all
   * (CMT-FR-02) — a Markdown file's WYSIWYG surface, and a source file's own
   * surface. A Markdown file in raw-text mode has no margin, and there they are
   * read in the floating panel.
   */
  const discussionsInRail = railHere;

  /**
   * ACT-FR-19: the rail renders two kinds of conversation, and each card's
   * channels have to reach the hook that owns *its* kind.
   *
   * The Discussion section at the rail's head is held by the action control, the
   * aligned cards below it by `useComments`, and the two conversations differ in
   * every rule that matters to a card: a discussion's follow-up reaches every
   * agent already in it even when it names nobody (CTA-FR-HCMJ), an anchored reply
   * reaches only whom it tags (CTA-FR-DNMV); a discussion's turn carries the
   * `artifact_discussion` origin that gets the agent the whole file (AGC-FR-07),
   * an anchored one the passage. Pointing every channel at the anchored hook —
   * which is what having one rail invites — silently gives a discussion the
   * anchored rules: an untagged follow-up dispatches nobody and a tagged one
   * dispatches under the wrong origin, whose turn then belongs to neither card.
   *
   * So each per-thread channel below is routed by which kind the thread is, and
   * the two read-only ones are the union — thread ids are unique across the two
   * logs, so neither merge can collide.
   */
  const discussionIds = useMemo(
    () => new Set(control.discussions.discussions.map((d) => d.thread.id)),
    [control.discussions.discussions],
  );
  const railReply = useCallback(
    (
      threadId: string,
      body: string,
      quotes: CommentQuote[],
      attachments: AttachmentInput[],
    ) =>
      discussionIds.has(threadId)
        ? control.discussions.reply(threadId, body, quotes, attachments)
        : comments.reply(threadId, body, quotes, attachments),
    [discussionIds, control.discussions, comments],
  );
  const railSetLock = useCallback(
    (threadId: string, locked: boolean) =>
      discussionIds.has(threadId)
        ? control.discussions.setLock(threadId, locked)
        : comments.setLock(threadId, locked),
    [discussionIds, control.discussions, comments],
  );
  const railSetResolved = useCallback(
    (threadId: string, resolved: boolean) =>
      discussionIds.has(threadId)
        ? control.discussions.setResolved(threadId, resolved)
        : comments.setResolved(threadId, resolved),
    [discussionIds, control.discussions, comments],
  );
  // CTA-FR-QXIG / CTA-FR-QTNB: what is still coming and why it stopped coming, for
  // both kinds at once — a card takes only the entries filed under its own id.
  const railPendingTurns = useMemo(
    () => [...comments.pendingTurns, ...control.discussions.pendingTurns],
    [comments.pendingTurns, control.discussions.pendingTurns],
  );
  const railTurnFailures = useMemo(
    () => ({ ...comments.turnFailures, ...control.discussions.turnFailures }),
    [comments.turnFailures, control.discussions.turnFailures],
  );
  const railErrors = useMemo(
    () => ({ ...comments.errors, ...control.discussions.errors }),
    [comments.errors, control.discussions.errors],
  );
  // CTA-FR-STWA: the offer to ask again, for both kinds at once — a card takes
  // only the entry filed under its own thread id.
  const railFailedTurns = useMemo(
    () => ({ ...comments.failedTurns, ...control.discussions.failedTurns }),
    [comments.failedTurns, control.discussions.failedTurns],
  );
  // CTA-FR-XSGX: and the unsupported-image notice, on the same terms — a card
  // takes only the entry filed under its own thread id.
  const railImageNotices = useMemo(
    () => ({ ...comments.imageNotices, ...control.discussions.imageNotices }),
    [comments.imageNotices, control.discussions.imageNotices],
  );
  const railRetryingTurnIds = useMemo(
    () =>
      new Set([
        ...comments.retryingTurnIds,
        ...control.discussions.retryingTurnIds,
      ]),
    [comments.retryingTurnIds, control.discussions.retryingTurnIds],
  );
  const railRetryTurn = useCallback(
    (turnId: string) => {
      // Routed by which hook holds the offer, for the reason `railCancelTurn`
      // is: the entry is cleared optimistically in the hook that holds it, and
      // retrying through the other would leave the failed contribution up.
      const mine = Object.values(control.discussions.failedTurns).some(
        (t) => t.id === turnId,
      );
      void (mine ? control.discussions.retryTurn : comments.retryTurn)(turnId);
    },
    [control.discussions, comments],
  );
  const railCancelTurn = useCallback(
    (turnId: string) => {
      // Routed by the turn's own origin rather than by its thread, because a
      // cancellation is optimistic in the hook that holds the turn and doing it
      // in the other leaves the pending contribution on the card.
      const mine = control.discussions.pendingTurns.some((t) => t.id === turnId);
      void (mine ? control.discussions.cancelTurn : comments.cancelTurn)(turnId);
    },
    [control.discussions, comments],
  );

  return {
    comments,
    control,
    dismissControl,
    setDismissControl,
    unresolvedInTab,
    railOpen,
    railOnSource,
    railHere,
    toggleRail,
    discussionsInRail,
    railReply,
    railSetLock,
    railSetResolved,
    railPendingTurns,
    railTurnFailures,
    railErrors,
    railFailedTurns,
    railImageNotices,
    railRetryingTurnIds,
    railRetryTurn,
    railCancelTurn,
  };
}
