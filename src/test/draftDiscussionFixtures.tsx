/**
 * Shared builders for the draft discussion column's component tests
 * (`../components/DraftDiscussion*.test.tsx`).
 *
 * The column renders its conversation through the shared `DiscussionSurface`,
 * which reads the outstanding turns, the reading position and the unread state
 * from the session store. A test seeds those with the store's own functions
 * rather than through props.
 */
import { vi } from "vitest";

import { DiscussionColumn } from "../components/DraftDiscussion";
import type { Comment, Discussion, Participant } from "../types";

export const DRAFT = "d1";
export const HUMAN: Participant = { kind: "human", login: "raver119" };

export type ColumnProps = Parameters<typeof DiscussionColumn>[0];

export function comment(
  id: string,
  author: Participant,
  body: string,
  over: Partial<Comment> = {},
): Comment {
  return {
    id,
    author,
    body,
    quotes: [],
    attachments: [],
    createdAt: "2026-01-01T00:00:00Z",
    ...over,
  };
}

/** A whole-target discussion of the draft. */
export function discussion(
  id: string,
  comments: Comment[],
  over: Partial<Discussion> = {},
): Discussion {
  return {
    id,
    target: { kind: "draft", draftId: DRAFT },
    fragmentTarget: null,
    comments,
    locked: false,
    resolved: false,
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
    ...over,
  };
}

/** A discussion of the draft whose comments say `bodies`, all by the author. */
export function discussionSaying(id: string, bodies: string[]): Discussion {
  return discussion(
    id,
    bodies.map((body, at) => comment(`${id}-c${at}`, HUMAN, body)),
  );
}

/** A fragment discussion of the draft's prompt. */
export function fragmentDiscussion(
  id: string,
  quote: string,
  over: Partial<Discussion> = {},
): Discussion {
  return discussion(id, [comment(`${id}-c0`, HUMAN, `about ${quote}`)], {
    fragmentTarget: {
      owner: { kind: "draft", draftId: DRAFT },
      path: "prompt.md",
      start: 0,
      end: quote.length,
      quote,
    },
    ...over,
  });
}

export function columnElement(over: Partial<ColumnProps> = {}) {
  return (
    <DiscussionColumn
      draftId={DRAFT}
      discussions={[discussionSaying("disc-1", ["opening"])]}
      selectedThreadId={null}
      onSelectThread={vi.fn()}
      identity={HUMAN}
      identityBlock={null}
      agents={[]}
      onReply={vi.fn(async () => {})}
      onSetLock={vi.fn(async () => {})}
      onSetResolved={vi.fn(async () => {})}
      errors={{}}
      blocked={false}
      anchor={null}
      onOpenDiscussion={vi.fn(async () => discussionSaying("disc-2", ["x"]))}
      {...over}
    />
  );
}
