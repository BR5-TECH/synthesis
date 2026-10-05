// The agent-conversation origin and the storage locator of one discussion.
//
// Both follow from the discussion's owner target and its optional fragment
// target. They never depend on the surface where the discussion is shown.

import type { Discussion, DiscussionTarget } from "../types";
import type { ConversationOrigin } from "../types";

/**
 * AGC-FR-05: the origin a turn dispatched from this discussion names. It is the
 * discussion's id, its owner target, and its fragment target. Every discussion
 * has one, whatever surface shows it.
 */
export function originFor(d: Discussion): ConversationOrigin {
  return {
    discussionId: d.id,
    target: d.target,
    fragmentTarget: d.fragmentTarget ?? null,
  };
}

/** AGC-FR-05: the id of the draft that owns the origin's discussion, if any. */
export function originDraftId(o: ConversationOrigin): string | null {
  return o.target.kind === "draft" ? o.target.draftId : null;
}

/** AGC-FR-05: the id of the artifact that owns the origin's discussion, if any. */
export function originArtifactId(o: ConversationOrigin): string | null {
  return o.target.kind === "artifact" ? o.target.artifactId : null;
}

/**
 * The kind of conversation an origin names, derived from its owner kind and
 * whether it has a fragment target. The same derivation as the backend's
 * `OriginKind`: it is not sent and not stored.
 */
export type OriginKind =
  | "artifact_comment"
  | "artifact_discussion"
  | "draft_comment"
  | "draft_discussion"
  | "note_discussion";

/** AGC-FR-05: the kind of conversation the origin names. */
export function originKind(o: ConversationOrigin): OriginKind {
  const fragment = o.fragmentTarget != null;
  switch (o.target.kind) {
    case "artifact":
      return fragment ? "artifact_comment" : "artifact_discussion";
    case "draft":
      return fragment ? "draft_comment" : "draft_discussion";
    case "note":
      return "note_discussion";
  }
}

/** The optional locator hint the storage commands take for one owner target. */
export function locatorOf(target: DiscussionTarget): {
  draftId?: string;
  artifactId?: string;
  noteId?: string;
} {
  switch (target.kind) {
    case "draft":
      return { draftId: target.draftId };
    case "artifact":
      return { artifactId: target.artifactId };
    case "note":
      return { noteId: target.noteId };
  }
}
