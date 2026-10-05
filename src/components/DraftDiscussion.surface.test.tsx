/**
 * The draft discussion column renders and opens discussions through the shared
 * surface and the shared composer
 * (`../../specifications/ui/DDS-draft-discussion.md` DDS-FR-KTVW, DDS-FR-NWRL,
 * DDS-FR-QMBC, DDS-FR-VHZN, DDS-FR-LPSC;
 * `../../specifications/ui/CVP-conversation-presentation.md`
 * CVP-FR-SDMQ, CVP-FR-47).
 *
 * `./DraftDiscussion.test.tsx` holds the column's layout and reading position.
 * This file holds what the column shares with every other owner: one surface,
 * one composer, and state that belongs to the discussion rather than to the
 * column.
 */
import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

import { DiscussionSurface } from "./discussion";
import { resetDraftDiscussions } from "../state/draftDiscussion";
import { resetLayoutPreferencesCache } from "../state/layoutPreferences";
import {
  getDiscussionSession,
  setComposerAttachments,
  setDiscussionTurns,
  setDiscussionScroll,
  noteDiscussionArrivals,
} from "../state/discussionSession";
import { forgetThreads, heldThreads, publishThread } from "../state/conversationThreads";
import { invalidateDraftDiscussions } from "../state/draftDiscussionInvalidation";
import {
  DRAFT,
  HUMAN,
  columnElement,
  discussion,
  discussionSaying,
  fragmentDiscussion,
} from "../test/draftDiscussionFixtures";
import type { AgentTurn } from "../types";
import {
  draftDiscussionOrigin,
} from "../test/origins";

const turn = {
  id: "turn-1",
  agentId: "a1",
  nickname: "helga",
  origin: draftDiscussionOrigin("disc-1", DRAFT),
  triggerCommentId: "disc-1-c0",
  state: "running",
  failure: null,
  retryPermitted: false,
  imagesOmitted: false,
  activeToolCalls: [],
  startedAt: "2026-01-01T00:00:00Z",
  endedAt: null,
} as unknown as AgentTurn;

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockResolvedValue([]);
  resetDraftDiscussions();
  resetLayoutPreferencesCache();
  for (const d of heldThreads()) forgetThreads([d.id]);
});

afterEach(cleanup);

describe("one surface for every discussion of the draft (DDS-FR-NWRL, DDS-FR-VHZN)", () => {
  it("DDS-FR-NWRL, DDS-FR-VHZN, CVP-FR-SDMQ: a whole-target and a fragment discussion are the same surface in the column", async () => {
    const whole = discussionSaying("disc-1", ["about the whole draft"]);
    const fragment = fragmentDiscussion("disc-2", "the ontology");
    const props = {
      discussions: [whole, fragment],
      selectedThreadId: "disc-1",
    };
    const { rerender } = render(columnElement(props));

    const first = await screen.findByTestId("comment-thread-disc-1");
    expect(first).toHaveAttribute("data-discussion-owner", "column");
    expect(first).toHaveAttribute("data-target", "whole");
    // CMT-FR-55: a whole-target discussion carries no fragment quote.
    expect(within(first).queryByTestId("discussion-fragment-quote")).toBeNull();

    // DDS-FR-VHZN: the chooser lists both, in the order they were opened.
    const tabs = screen.getAllByRole("tab");
    expect(tabs).toHaveLength(2);
    expect(tabs[0]).toHaveAttribute("data-target", "whole");
    expect(tabs[1]).toHaveAttribute("data-target", "fragment");

    rerender(columnElement({ ...props, selectedThreadId: "disc-2" }));
    const second = await screen.findByTestId("comment-thread-disc-2");
    // The same component, the same owner, the same markup.
    expect(second).toHaveAttribute("data-discussion-owner", "column");
    expect(second).toHaveAttribute("data-target", "fragment");
    expect(second.className).toBe(first.className);
    expect(within(second).getByTestId("discussion-fragment-quote")).toHaveTextContent(
      "the ontology",
    );
    // Only the discussion on screen is rendered, and it is rendered once.
    expect(screen.queryByTestId("comment-thread-disc-1")).toBeNull();
    expect(screen.getAllByTestId("comment-thread-disc-2")).toHaveLength(1);
  });

  it("DDS-FR-NWRL, CVP-FR-02: a second column of the draft renders nothing for a discussion the first one shows", async () => {
    // Duplicate prevention: one discussion has one surface, however many owners
    // would show it.
    const props = { discussions: [discussionSaying("disc-1", ["only once"])] };
    render(
      <>
        {columnElement(props)}
        {columnElement(props)}
      </>,
    );
    await screen.findByTestId("comment-thread-disc-1");
    expect(screen.getAllByTestId("comment-thread-disc-1")).toHaveLength(1);
    expect(screen.getAllByText("only once")).toHaveLength(1);
  });

  it("CMT-FR-19, DDS-FR-QMBC: a fragment the prompt no longer holds is kept and shown as orphaned in the column", async () => {
    const fragment = fragmentDiscussion("disc-2", "a removed passage");
    render(
      columnElement({
        discussions: [fragment],
        selectedThreadId: "disc-2",
        availabilityOf: () => "orphaned",
      }),
    );
    const card = await screen.findByTestId("comment-thread-disc-2");
    expect(card).toHaveAttribute("data-availability", "orphaned");
    // Kept, readable, and still carrying its quote.
    expect(within(card).getByText("about a removed passage")).toBeInTheDocument();
    expect(within(card).getByTestId("discussion-fragment-quote")).toBeInTheDocument();
  });

  it("DDS-FR-QMBC, CMT-FR-28: activating the quote of a fragment discussion reports it for the prompt to reveal", async () => {
    const fragment = fragmentDiscussion("disc-2", "the ontology");
    const onFocusFragment = vi.fn();
    render(
      columnElement({
        discussions: [fragment],
        selectedThreadId: "disc-2",
        onFocusFragment,
      }),
    );
    await userEvent.click(await screen.findByTestId("discussion-fragment-quote"));
    expect(onFocusFragment).toHaveBeenCalledWith(
      expect.objectContaining({ quote: "the ontology" }),
      fragment,
    );
  });
});

describe("the opening composer (DDS-FR-VHZN, DDS-FR-LPSC, NAW-FR-32)", () => {
  async function typeOpening(text: string) {
    const field = await screen.findByRole("textbox", { name: "Discuss this draft" });
    fireEvent.change(field, { target: { value: text } });
    return field;
  }

  it("DDS-FR-VHZN, NAW-FR-32: opens a whole-target discussion of the draft, carrying no fragment", async () => {
    const onOpenDiscussion = vi.fn(async () => discussionSaying("disc-9", ["x"]));
    render(columnElement({ discussions: [], onOpenDiscussion }));
    await typeOpening("is this ready?");
    await userEvent.click(screen.getByRole("button", { name: "Post" }));

    await waitFor(() => expect(onOpenDiscussion).toHaveBeenCalledTimes(1));
    expect(onOpenDiscussion).toHaveBeenCalledWith({
      target: { kind: "draft", draftId: DRAFT },
      fragmentTarget: null,
      body: "is this ready?",
      attachments: [],
    });
  });

  it.each([
    ["Ctrl", { ctrlKey: true }],
    ["Cmd", { metaKey: true }],
  ])("DDS-FR-LPSC, CVP-FR-40: %s+Enter posts the opening composer once", async (_name, modifier) => {
    const onOpenDiscussion = vi.fn(async () => discussionSaying("disc-9", ["x"]));
    render(columnElement({ discussions: [], onOpenDiscussion }));
    const field = await typeOpening("with the keyboard");

    fireEvent.keyDown(field, { key: "Enter", ...modifier });
    // A second press while the first is in flight does not post again.
    fireEvent.keyDown(field, { key: "Enter", ...modifier });

    await waitFor(() => expect(onOpenDiscussion).toHaveBeenCalledTimes(1));
  });

  it("DDS-FR-LPSC: the accelerator does nothing on an empty composer, and a plain Enter adds a line rather than posting", async () => {
    const onOpenDiscussion = vi.fn(async () => discussionSaying("disc-9", ["x"]));
    render(columnElement({ discussions: [], onOpenDiscussion }));
    const field = await screen.findByRole("textbox", { name: "Discuss this draft" });

    fireEvent.keyDown(field, { key: "Enter", ctrlKey: true });
    fireEvent.change(field, { target: { value: "text" } });
    fireEvent.keyDown(field, { key: "Enter" });

    expect(onOpenDiscussion).not.toHaveBeenCalled();
  });

  it("CMT-FR-34, CVP-FR-41: a refused opening keeps the text and the strip and says why", async () => {
    const onOpenDiscussion = vi.fn(async () => {
      throw "unsupported_media_type";
    });
    render(columnElement({ discussions: [], onOpenDiscussion }));
    const field = await typeOpening("keep me");
    act(() =>
      setComposerAttachments(`draft:${DRAFT}`, [
        {
          input: { kind: "url", url: "https://example.com/a.png", mediaType: "image/png" },
          name: "a.png",
        },
      ]),
    );
    await userEvent.click(screen.getByRole("button", { name: "Post" }));

    expect(await screen.findByTestId("discussion-composer-error")).toBeInTheDocument();
    expect(field).toHaveValue("keep me");
    expect(screen.getByText("a.png")).toBeInTheDocument();
  });

  it("DDS-FR-QMBC: a passage chosen in the prompt puts the column on an opening composer that carries it", async () => {
    const onOpenDiscussion = vi.fn(async () => fragmentDiscussion("disc-9", "the ontology"));
    const onCancelFragment = vi.fn();
    const openingFragment = {
      owner: { kind: "draft", draftId: DRAFT } as const,
      path: "prompt.md",
      start: 4,
      end: 16,
      quote: "the ontology",
    };
    render(
      columnElement({
        discussions: [discussionSaying("disc-1", ["opening"])],
        openingFragment,
        onOpenDiscussion,
        onCancelFragment,
      }),
    );
    // The reply of the discussion on screen gives way to the opening composer.
    expect(await screen.findByTestId("discussion-opening-fragment")).toHaveTextContent(
      "the ontology",
    );
    expect(screen.queryByTestId("comment-thread-disc-1")).toBeNull();

    const field = await screen.findByRole("textbox", { name: "Discuss this draft" });
    await waitFor(() => expect(field).toHaveFocus());
    fireEvent.change(field, { target: { value: "split this" } });
    fireEvent.keyDown(field, { key: "Enter", ctrlKey: true });

    await waitFor(() =>
      expect(onOpenDiscussion).toHaveBeenCalledWith(
        expect.objectContaining({ fragmentTarget: openingFragment, body: "split this" }),
      ),
    );
  });

  it("CMT-FR-07, DDS-FR-QMBC: discarding the opening composer of a passage abandons it and returns to the discussion", async () => {
    const onCancelFragment = vi.fn();
    const openingFragment = {
      owner: { kind: "draft", draftId: DRAFT } as const,
      path: "prompt.md",
      start: 0,
      end: 3,
      quote: "abc",
    };
    const { rerender } = render(
      columnElement({ openingFragment, onCancelFragment }),
    );
    await userEvent.click(await screen.findByRole("button", { name: "Cancel" }));
    expect(onCancelFragment).toHaveBeenCalledTimes(1);

    rerender(columnElement({ openingFragment: null, onCancelFragment }));
    expect(await screen.findByTestId("comment-thread-disc-1")).toBeInTheDocument();
  });
});

describe("state that belongs to the discussion (CVP-FR-47, DDS-FR-LPSC)", () => {
  it("CVP-FR-47: an owner change keeps the unsent text, the attachments, and the pending placeholder", async () => {
    const d = discussionSaying("disc-1", ["opening"]);
    setDiscussionTurns("disc-1", [turn]);
    const view = render(columnElement({ discussions: [d] }));

    const reply = await screen.findByLabelText("Reply to thread disc-1");
    fireEvent.change(reply, { target: { value: "half a sentence" } });
    act(() =>
      setComposerAttachments("disc-1", [
        {
          input: { kind: "url", url: "https://example.com/b.png", mediaType: "image/png" },
          name: "b.png",
        },
      ]),
    );
    expect(await screen.findByTestId("dds-transient")).toBeInTheDocument();

    // The column goes, and another owner shows the same discussion.
    view.unmount();
    render(
      <DiscussionSurface
        discussion={d}
        owner="tab"
        variant="embedded"
        agents={[]}
        identity={HUMAN}
        onReply={vi.fn(async () => {})}
        onSetLock={vi.fn(async () => {})}
        onSetResolved={vi.fn(async () => {})}
      />,
    );

    const card = await screen.findByTestId("comment-thread-disc-1");
    expect(card).toHaveAttribute("data-discussion-owner", "tab");
    expect(within(card).getByLabelText("Reply to thread disc-1")).toHaveValue(
      "half a sentence",
    );
    expect(within(card).getByText("b.png")).toBeInTheDocument();
    // The same outstanding turn, drawn in the other owner's form.
    expect(within(card).getByTestId("comment-pending")).toBeInTheDocument();
    expect(getDiscussionSession("disc-1").turns.map((t) => t.id)).toEqual(["turn-1"]);
  });

  it("DDS-FR-XQMF, DDS-FR-LPSC: hiding the column keeps the unsent text where it was", async () => {
    const props = { discussions: [discussionSaying("disc-1", ["opening"])] };
    const { rerender } = render(columnElement(props));
    const reply = await screen.findByLabelText("Reply to thread disc-1");
    fireEvent.change(reply, { target: { value: "still here" } });

    rerender(columnElement({ ...props, hidden: true }));
    expect(screen.getByTestId("draft-discussion-column")).not.toBeVisible();
    rerender(columnElement({ ...props, hidden: false }));

    expect(screen.getByLabelText("Reply to thread disc-1")).toHaveValue("still here");
  });

  it("CVP-FR-TWRL, DDS-FR-GKMT: arrivals while the author reads back show the shared indicator and divider in the column, and no second one", async () => {
    const { rerender } = render(
      columnElement({ discussions: [discussionSaying("disc-1", ["one", "two"])] }),
    );
    await screen.findByTestId("comment-thread-disc-1");

    act(() => {
      setDiscussionScroll("disc-1", 0, false);
      noteDiscussionArrivals("disc-1", ["disc-1-c1"]);
    });
    rerender(
      columnElement({ discussions: [discussionSaying("disc-1", ["one", "two"])] }),
    );

    const column = screen.getByTestId("draft-discussion-column");
    const indicators = within(column).getAllByTestId("discussion-unread-indicator");
    expect(indicators).toHaveLength(1);
    expect(indicators[0]).toHaveTextContent("1 new message");
    expect(within(column).getAllByRole("separator", { name: "1 unread" })).toHaveLength(1);
    // The column draws no unread or jump control of its own any more.
    expect(screen.queryByTestId("discussion-jump-to-latest")).toBeNull();
  });
});

describe("a deleted draft takes its discussions with it (CMS-FR-39, DRS-FR-21)", () => {
  it("CMS-FR-39: forgets the unsent text, the position, the turns and the cached threads of that draft alone", () => {
    const mine = discussionSaying("disc-1", ["mine"]);
    const mineFragment = fragmentDiscussion("disc-2", "a passage");
    const other = discussion("disc-3", [], {
      target: { kind: "draft", draftId: "d2" },
    });
    for (const d of [mine, mineFragment, other]) publishThread(d);
    for (const id of ["disc-1", "disc-2", "disc-3", `draft:${DRAFT}`]) {
      act(() => {
        setDiscussionScroll(id, 90, false);
        setComposerAttachments(id, []);
      });
    }
    setDiscussionTurns("disc-1", [turn]);

    invalidateDraftDiscussions(DRAFT);

    expect(getDiscussionSession("disc-1").turns).toEqual([]);
    expect(getDiscussionSession("disc-1").scrollTop).toBe(0);
    expect(getDiscussionSession("disc-2").scrollTop).toBe(0);
    expect(getDiscussionSession(`draft:${DRAFT}`).scrollTop).toBe(0);
    expect(heldThreads().map((d) => d.id)).toEqual(["disc-3"]);
    // Another draft's discussion is untouched.
    expect(getDiscussionSession("disc-3").scrollTop).toBe(90);
  });

  it("CMS-FR-39, NAW-FR-20: graduating a draft invalidates nothing, so the column keeps its discussions", async () => {
    // Only deleting ends a draft's discussions. Nothing in the column's own life
    // calls the invalidation, so a graduated draft reads as it did.
    const mine = discussionSaying("disc-1", ["kept"]);
    publishThread(mine);
    render(columnElement({ discussions: [mine] }));
    expect(await screen.findByText("kept")).toBeInTheDocument();
    expect(heldThreads().map((d) => d.id)).toContain("disc-1");
  });
});
