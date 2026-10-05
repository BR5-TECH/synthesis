/**
 * The shapes and harnesses every Comments-panel test builds from
 * (`../../specifications/ui/CMP-comments-panel.md`).
 *
 * Shared rather than copied, on the model of `streamFixtures.ts`: more than one
 * test file renders the same panel, and a thread that drifted between them
 * would let one of them pass against a record the backend never sends.
 */

import { useState } from "react";

import { Comments } from "../components/Comments";
import type { DiscussionReveal } from "../state/revealDiscussion";
import { useCommentsPanelState } from "../hooks/useCommentsPanelState";
import {
  buildDiscussion,
  buildListItem,
  type DiscussionOverrides,
} from "./discussionFixtures";
import type {
  Comment,
  DiscussionListItem,
  Participant,
} from "../types";

// Narrowed to their union members, which is what an in-file `const` of a
// union type narrows to on its own: a test spreads `agent` and adds `title`.
export const human: Extract<Participant, { kind: "human" }> = {
  kind: "human",
  login: "raver119",
};
export const agent: Extract<Participant, { kind: "agent" }> = {
  kind: "agent",
  agentId: "claude_code",
  handle: "claude",
};

export function comment(
  id: string,
  body: string,
  author: Participant = human,
): Comment {
  return {
    id,
    author,
    body,
    quotes: [],
    attachments: [],
    createdAt: "2024-01-01T00:00:00Z",
  };
}

export function thread(
  over: DiscussionOverrides,
  ownerUnavailable = false,
): DiscussionListItem {
  return buildListItem(
    buildDiscussion(over, {
      path: "specs/onboarding.md",
      range: { start: 0, end: 17, quote: "the first session" },
      comments: [comment("c1", "Which session?")],
    }),
    ownerUnavailable,
  );
}

/** Mount the panel with its filter state held above it, as `VPanel` does. */
export function Harness({
  onReveal = () => {},
}: {
  onReveal?: (r: DiscussionReveal) => void;
}) {
  const panel = useCommentsPanelState();
  return <Comments panel={panel} onReveal={onReveal} />;
}

/**
 * `VPanel` swapping surfaces: `<Comments>` unmounts while the filter state,
 * which lives in the hook above it, does not (CMP-FR-15).
 */
export function SwitchableHarness() {
  const panel = useCommentsPanelState();
  const [showing, setShowing] = useState(true);
  return (
    <>
      <button onClick={() => setShowing((v) => !v)}>toggle surface</button>
      {showing && <Comments panel={panel} onReveal={() => {}} />}
    </>
  );
}

/** Each group header as `"<label> <count>"` — the two spans read separately. */
export const groupHeaders = () =>
  Array.from(document.querySelectorAll(".note-group__header")).map((h) => {
    const spans = Array.from(h.querySelectorAll("span")).map((s) => s.textContent);
    return spans.join(" ");
  });

export const rows = () => Array.from(document.querySelectorAll(".comment-row"));
