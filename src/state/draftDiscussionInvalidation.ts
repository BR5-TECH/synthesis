/**
 * What the application forgets when a draft is deleted
 * (`../../specifications/core/CMS-comments-storage.md` CMS-FR-39,
 * `../../specifications/core/DRS-draft-storage.md` DRS-FR-21).
 *
 * A draft owner's discussions live exactly as long as the draft does, and
 * deleting the draft is the only thing that ends them. The backend removes the
 * logs and the attachments. This module is the frontend half: the discussions
 * are gone, so what is held about them — the unsent text and attachments, the
 * reading position, the outstanding turns, the focus requests, and the cached
 * conversations — goes with them. Nothing is left that could render a
 * discussion the backend no longer has, and a draft that is created later
 * cannot inherit the text of one that was deleted.
 *
 * Graduating a draft calls nothing here: it removes no discussion
 * (CMS-FR-39, NAW-FR-20).
 */
import { logDebug } from "../logging";
import { discussionTargetKey, discussionDraftId } from "../types";
import { forgetThreads, heldThreads } from "./conversationThreads";
import { consumePendingFocus } from "./discussionFocus";
import { clearDiscussionSession } from "./discussionSession";

/** Forget every discussion of one draft, and the opening composer of the draft. */
export function invalidateDraftDiscussions(draftId: string): void {
  const owned = heldThreads().filter(
    (d) =>
      discussionDraftId(d) === draftId ||
      (d.fragmentTarget?.owner.kind === "draft" &&
        d.fragmentTarget.owner.draftId === draftId),
  );
  for (const d of owned) {
    clearDiscussionSession(d.id);
    consumePendingFocus(d.id);
  }
  forgetThreads(owned.map((d) => d.id));
  // The opening composer is keyed by the target rather than by a discussion.
  clearDiscussionSession(discussionTargetKey({ kind: "draft", draftId }));
  logDebug(["frontend"], "the discussions of a deleted draft were forgotten", {
    draftId,
    discussions: owned.length,
  });
}
