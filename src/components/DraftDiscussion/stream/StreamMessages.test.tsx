/**
 * What the column draws **between** its messages
 * (`../../../../specifications/ui/DDS-draft-discussion.md` DDS-FR-GKMT,
 * DDS-FR-CLBK).
 *
 * The unread divider and the collapsed-history row are the column's, not the
 * conversation's, so they reach this component as a render hook. Where they land
 * is what this file holds: one comment can produce two items — a block for its
 * body and a row for the change it announces — and a mark placed per item rather
 * than per comment is drawn twice, once of them in the middle of an exchange.
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => []) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import { StreamMessages } from "./StreamMessages";
import type { PairedHalf } from "../../CommentRail/questionPairs";
import type { Attachment, Comment, Participant } from "../../../types";

const HELGA: Participant = { kind: "agent", agentId: "a1", handle: "helga" };

const PROPOSAL: Attachment = {
  kind: "proposal",
  proposalId: "p1",
  draftId: "d1",
  path: "Post draft to Github.md",
};

function comment(id: string, body: string, attachments: Attachment[] = []): Comment {
  return {
    id,
    author: HELGA,
    body,
    quotes: [],
    attachments,
    createdAt: "2026-01-01T00:00:00Z",
  };
}

function renderStream(comments: Comment[], marked: string[]) {
  return render(
    <StreamMessages
      comments={comments}
      pairing={new Map<string, PairedHalf>()}
      isOwn={() => false}
      roster={{ nicknames: [], ready: [] }}
      threadId="disc-1"
      draftId="d1"
      visibleFrom={0}
      beforeComment={(id) =>
        marked.includes(id) ? <hr data-testid="mark" data-for={id} /> : null
      }
      authorOf={() => "helga"}
      pendingTurns={[]}
      onCancelTurn={vi.fn()}
    />,
  );
}

afterEach(cleanup);

describe("a mark belongs to a comment, not to an item (DDS-FR-GKMT)", () => {
  it("DDS-FR-GKMT: draws one divider above a comment that carries a change as well as a body", () => {
    renderStream([comment("c1", "I changed two things.", [PROPOSAL])], ["c1"]);
    expect(screen.getAllByTestId("mark")).toHaveLength(1);
  });

  it("DDS-FR-GKMT: the divider stands above the message rather than between it and its own row", () => {
    renderStream([comment("c1", "I changed two things.", [PROPOSAL])], ["c1"]);
    const column = document.querySelector(".dds-stream__column")!;
    const order = [...column.children]
      .map((el) => el.getAttribute("data-testid") ?? el.className)
      // The day divider opens every transcript and is not what this is about.
      .filter((name) => name !== "dds-day-divider");
    expect(order).toEqual(["mark", "dds-message", "dds-change-row"]);
  });

  it("DDS-FR-GKMT: a comment the column marks nothing above draws no divider", () => {
    renderStream([comment("c1", "nothing to mark")], []);
    expect(screen.queryByTestId("mark")).toBeNull();
  });
});
