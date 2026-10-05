// Conversation origins for tests, in the shape the backend takes
// (`../../specifications/core/AGC-agent-conversations.md` AGC-FR-05).
//
// A comment origin carries a fragment target, because the fragment target is
// what makes a discussion a comment on a passage rather than a discussion of
// the whole target.

import type { ConversationOrigin, FragmentTarget } from "../types";

function fragmentOf(owner: FragmentTarget["owner"], path: string): FragmentTarget {
  return { owner, path, start: 0, end: 1, quote: "x" };
}

export function artifactCommentOrigin(
  discussionId: string,
  artifactId = "a.md",
): ConversationOrigin {
  const target = { kind: "artifact", artifactId } as const;
  return { discussionId, target, fragmentTarget: fragmentOf(target, artifactId) };
}

export function artifactDiscussionOrigin(
  discussionId: string,
  artifactId: string,
): ConversationOrigin {
  return { discussionId, target: { kind: "artifact", artifactId }, fragmentTarget: null };
}

export function draftCommentOrigin(
  discussionId: string,
  draftId: string,
  path = "prompt.md",
): ConversationOrigin {
  const target = { kind: "draft", draftId } as const;
  return { discussionId, target, fragmentTarget: fragmentOf(target, path) };
}

export function draftDiscussionOrigin(
  discussionId: string,
  draftId: string,
): ConversationOrigin {
  return { discussionId, target: { kind: "draft", draftId }, fragmentTarget: null };
}

export function noteDiscussionOrigin(
  discussionId: string,
  noteId: string,
): ConversationOrigin {
  return { discussionId, target: { kind: "note", noteId }, fragmentTarget: null };
}

/** A fragment target serde reads: absent, null, or every field present. */
function fragmentReads(fragment: unknown): boolean {
  if (fragment == null) return true;
  const f = fragment as Record<string, unknown>;
  const owner = f.owner as Record<string, unknown> | undefined;
  return (
    typeof owner?.kind === "string" &&
    typeof f.path === "string" &&
    typeof f.start === "number" &&
    typeof f.end === "number" &&
    typeof f.quote === "string"
  );
}

/**
 * AGC-FR-05: refuse an origin the backend would refuse, the way serde does when
 * Tauri reads the `origin` argument. A mocked `dispatch_agent_turn` calls this,
 * so a frontend that sends a shape the backend cannot read fails its tests
 * rather than only the running application.
 */
export function expectBackendOrigin(origin: unknown): ConversationOrigin {
  const o = origin as Record<string, unknown> | null;
  const target = o?.target as Record<string, unknown> | undefined;
  const idField =
    target?.kind === "draft"
      ? "draftId"
      : target?.kind === "artifact"
        ? "artifactId"
        : target?.kind === "note"
          ? "noteId"
          : null;
  if (
    !o ||
    typeof o.discussionId !== "string" ||
    !target ||
    idField === null ||
    typeof target[idField] !== "string" ||
    !fragmentReads(o.fragmentTarget)
  ) {
    throw new Error(
      "invalid args `origin` for command `dispatch_agent_turn`: missing field `discussionId` or `target`",
    );
  }
  return o as unknown as ConversationOrigin;
}
