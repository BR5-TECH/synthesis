/**
 * The Editor's discussion column (`../../../specifications/ui/CMT-comments.md`,
 * hosted per EDT-FR-61): the two kinds of conversation it renders, the column
 * they render in, and the per-thread channels each discussion reaches.
 *
 * Split out of the Editor whole because routing is the substance of it. The
 * whole-file discussions are held by the action control and the fragment ones
 * by `useComments`, and every channel below picks between them by which kind
 * the thread is — a rule that only stays correct while all of it is read in
 * one place.
 */
import { useCallback, useMemo, useState } from "react";
import { useComments } from "../../hooks/useComments";
import { useDiscussionControl } from "../../hooks/useDiscussionControl";
import { fileColumn, setColumnHidden } from "../../state/discussionColumn";
import { useDiscussionSplit, type SurfaceTurns } from "../DraftDiscussion";
import type { OwnerAvailability } from "../discussion";
import type { UseArtifactDocument } from "../../hooks/useArtifactDocument";
import type { EditSession } from "../../state/editSessions/types";
import type {
  AttachmentInput,
  CommentQuote,
  Discussion,
  DiscussionTarget,
} from "../../types";

export interface EditorDiscussionDeps {
  artifactId: string;
  session: EditSession;
  doc: UseArtifactDocument;
  showComments: boolean;
  showActions: boolean;
  /**
   * CMT-FR-06: the artifact's Markdown source, which is what an anchor is a
   * range in — so the column reads the same document a save would write.
   */
  currentDoc: () => string;
}

export function useEditorDiscussions(deps: EditorDiscussionDeps) {
  const { artifactId, session, doc, showComments, showActions, currentDoc } = deps;

  /**
   * CMT-FR-06: anchors are ranges in the artifact's **Markdown source**, so the
   * column reads the same document a save would write — which is what makes an
   * anchor mean the same passage in either editing mode.
   */
  const commentSource = useCallback(() => currentDoc(), [currentDoc]);
  // CMT-FR-02: the column stands on every surface of the file, so the
  // fragment discussions are loaded in every mode.
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
  // ACT-FR-22: the column's hook has already read the roster, the catalogue, and the
  // running turns — all three facts about the project rather than about one
  // conversation — so the control reuses them instead of reading them again.
  const control = useDiscussionControl(discussionTarget, showActions, undefined, {
    agents: comments.agents,
    allTurns: comments.allTurns,
  });
  /** ACT-FR-12: the column's menus and the control's surfaces are one set. */
  const [dismissControl, setDismissControl] = useState(0);

  // ---- The column (CMT-FR-01, per DDS-FR-KTVW) ---------------------------

  /**
   * CMT-FR-30: the column's split and hidden state are kept per file, keyed by
   * its project-relative path, and a file whose column was never hidden shows
   * it.
   */
  const column = fileColumn(artifactId);
  // An Editor embedded in a draft tab hosts no column, and the draft tab owns
  // the accelerator there.
  const split = useDiscussionSplit(column, showComments);
  const columnShown = showComments && !split.view.hidden;
  /** DDS-FR-XQMF / CMT-FR-05: show the column where it is hidden. */
  const showColumn = useCallback(() => {
    if (split.view.hidden) setColumnHidden(column, false);
  }, [column, split.view.hidden]);

  /**
   * CMT-FR-53: the file's discussions, whole-target and fragment alike, in the
   * order they were opened.
   */
  const columnDiscussions = useMemo<Discussion[]>(
    () =>
      [
        ...(showActions ? control.discussions.discussions.map((d) => d.thread) : []),
        ...comments.threads.map((t) => t.thread),
      ].sort((a, b) => a.createdAt.localeCompare(b.createdAt)),
    [showActions, control.discussions.discussions, comments.threads],
  );

  /**
   * CMT-FR-19 / CMT-FR-RPLC: a fragment discussion whose passage is no longer in
   * the file is orphaned, and every discussion of a file that is gone is
   * unavailable.
   */
  const orphanedIds = useMemo(
    () =>
      new Set(comments.threads.filter((t) => t.anchor === null).map((t) => t.thread.id)),
    [comments.threads],
  );
  const ownerUnavailable = doc.error !== null && session.buffer === "";
  const availabilityOf = useCallback(
    (discussion: Discussion): OwnerAvailability =>
      ownerUnavailable
        ? "unavailable"
        : orphanedIds.has(discussion.id)
          ? "orphaned"
          : "available",
    [ownerUnavailable, orphanedIds],
  );

  /**
   * ACT-FR-19: the column renders two kinds of conversation, and each
   * discussion's channels have to reach the hook that owns *its* kind.
   *
   * The whole-file discussions are held by the action control, the fragment
   * ones by `useComments`, and the two conversations differ in every rule that
   * matters to a discussion: a discussion's follow-up reaches every
   * agent already in it even when it names nobody (CTA-FR-HCMJ), an anchored reply
   * reaches only whom it tags (CTA-FR-DNMV); a discussion's turn carries the
   * `artifact_discussion` origin that gets the agent the whole file (AGC-FR-07),
   * an anchored one the passage. Pointing every channel at the anchored hook —
   * which is what having one column invites — silently gives a discussion the
   * anchored rules: an untagged follow-up dispatches nobody and a tagged one
   * dispatches under the wrong origin, whose turn then belongs to neither.
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

  /**
   * CTA-FR-QXIG / CTA-FR-QTNB / CTA-FR-STWA / CTA-FR-XSGX: what the surface of
   * one discussion shows of its turns. The Editor's hooks hold them, so the
   * surface reads none for itself.
   */
  const turnsFor = useCallback(
    (id: string): SurfaceTurns => ({
      pendingTurns: railPendingTurns.filter((t) => t.origin.discussionId === id),
      turnFailure: railTurnFailures[id],
      failedTurn: railFailedTurns[id],
      imageNotice: railImageNotices[id],
      retryingTurnIds: railRetryingTurnIds,
      onRetryTurn: railRetryTurn,
      onCancelTurn: railCancelTurn,
      liveTurns: false,
    }),
    [
      railPendingTurns,
      railTurnFailures,
      railFailedTurns,
      railImageNotices,
      railRetryingTurnIds,
      railRetryTurn,
      railCancelTurn,
    ],
  );

  return {
    comments,
    control,
    dismissControl,
    setDismissControl,
    column,
    split,
    columnShown,
    showColumn,
    columnDiscussions,
    availabilityOf,
    ownerUnavailable,
    railReply,
    railSetResolved,
    railErrors,
    turnsFor,
  };
}
