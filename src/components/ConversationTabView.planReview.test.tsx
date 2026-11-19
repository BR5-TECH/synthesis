/**
 * The review of a plan proposal in the tabs that the viewport shows outside the
 * Map tab (`MPV-map-proposal-review.md` MPV-FR-HGLN, MPV-FR-ZQRE): a
 * conversation tab (`TAB-tabs.md` TAB-FR-QXMV) and the draft pane of the New
 * Artifact tab. The backend is mocked at `invoke`.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ComponentProps } from "react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (..._args: unknown[]) => () => {}),
  emit: vi.fn(async () => {}),
}));

import { Viewport } from "./Viewport";
import { DraftDocumentColumn } from "./NewArtifactWorkspace/DraftDocumentColumn";
import { InPlaceReviewScope, MapPlanEntry } from "./MapPlanProposal";
import { clearConversationThreads, publishThread } from "../state/conversationThreads";
import { resetDiscussionFocus } from "../state/discussionFocus";
import { clearAllDiscussionSessions } from "../state/discussionSession";
import { resetOwnerAvailability } from "../state/ownerAvailability";
import { resetSharedCommentIdentity } from "../hooks/useSharedCommentIdentity";
import { resetLogBufferForTest } from "../logging";
import { resetProjectIdentity } from "../state/projectIdentity";
import { resetToasts } from "../state/toasts";
import type { DraftSessionStore } from "../state/draftSessions";
import { CONTEXT_ESTIMATE, otherCommand } from "../test/invokeMocks";
import { AGENT, createRow, mapComment, proposal } from "../test/mapPlanFixtures";
import type { Discussion, Tab } from "../types";

const human = { kind: "human", login: "raver119" } as const;

function noteDiscussion(): Discussion {
  return {
    id: "t1",
    target: { kind: "note", noteId: "n1" },
    fragmentTarget: null,
    comments: [
      mapComment("c1", human, "Plan the work."),
      mapComment("c2", AGENT, "Here is the plan.", { attachments: [{ kind: "mapPlanProposal", proposalId: "p1" }] }),
    ],
    resolved: false,
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
  };
}

const noteTab: Tab = {
  id: "conv:t1",
  label: "Chat: Ask legal",
  tooltip: "",
  kind: "conversation",
  threadId: "t1",
  noteId: "n1",
  ownerTarget: { kind: "note", noteId: "n1" },
  ownerLabel: "Ask legal",
  subject: "what legal said",
};

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string) => {
    switch (cmd) {
      case "resolve_comment_author_identity":
        return Promise.resolve(human);
      case "list_agent_turns":
        return Promise.resolve([]);
      case "estimate_discussion_context":
        return Promise.resolve(CONTEXT_ESTIMATE);
      case "list_map_plan_proposals":
        return Promise.resolve([proposal([createRow("n1", "New issue")])]);
      case "get_delivery_source":
        return Promise.resolve({ kind: "none", reason: "no_source" });
      default:
        return otherCommand(cmd);
    }
  });
  clearConversationThreads();
  clearAllDiscussionSessions();
  resetDiscussionFocus();
  resetOwnerAvailability();
  resetSharedCommentIdentity();
  resetProjectIdentity();
  resetToasts();
  resetLogBufferForTest();
});
afterEach(cleanup);

function renderViewport(tab: Tab) {
  const props = {
    activeTab: tab.id,
    activeT: tab,
    onOpenDraft: vi.fn(),
    onConversationOpened: vi.fn(),
  } as unknown as ComponentProps<typeof Viewport>;
  return render(<Viewport {...props} />);
}

describe("a conversation tab (MPV-FR-HGLN, MPV-FR-ZQRE)", () => {
  it("MPV-FR-HGLN, MPV-FR-ZQRE, MPV-FR-FESO: the sheet covers the messages of the conversation and leaves the composer outside it", async () => {
    publishThread(noteDiscussion());
    renderViewport(noteTab);
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: /^Review:/ }));
    const sheet = await screen.findByTestId("pp-sheet");
    const box = screen.getByTestId("discussion-messages-box");
    expect(box).toContainElement(sheet);
    expect(box).toContainElement(screen.getByTestId("discussion-messages"));
    const composer = screen.getByRole("textbox");
    expect(sheet).not.toContainElement(composer);
    expect(box).not.toContainElement(composer);
    // MPV-FR-FESO: a click in the composer while the sheet shows reaches it.
    await user.click(composer);
    expect(document.activeElement).toBe(composer);
    expect(composer).not.toBeDisabled();
    expect(screen.getByTestId("pp-sheet")).toBeInTheDocument();
  });
});

describe("the New Artifact tab (MPV-FR-ZQRE)", () => {
  it("MPV-FR-ZQRE: the sheet shows inside the draft pane", async () => {
    render(
      <InPlaceReviewScope>
        <DraftDocumentColumn
          drafts={{} as DraftSessionStore}
          docKey={null}
          selected={null}
          overlayOpen={false}
          imageError={null}
          imageStatus=""
          imageHost={undefined}
          announceImage={() => {}}
          viewing={null}
          viewText={null}
          viewError={null}
          entries={[]}
          readingMode="wysiwyg"
          onReadingMode={() => {}}
          onReturnToLive={() => {}}
          onRead={() => {}}
          writeReport=""
          dirty={false}
          readOnly={false}
          onEdit={() => {}}
          fragmentHost={undefined}
          review={undefined}
        />
        <div data-testid="discussion-column">
          <MapPlanEntry discussionId="t1" proposalId="p1" proposer="arch" />
        </div>
      </InPlaceReviewScope>,
    );
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: /^Review:/ }));
    const sheet = await screen.findByTestId("pp-sheet");
    expect(sheet.closest(".draft-editor")).not.toBeNull();
    expect(within(screen.getByTestId("discussion-column")).queryByTestId("pp-sheet")).toBeNull();
  });
});
