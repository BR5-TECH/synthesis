/**
 * New Artifact discussions: posting one, and what the margin does with it
 * (`../../specifications/ui/NAW-new-artifact.md` NAW-FR-31 … NAW-FR-34).
 */
import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
const listeners = new Map<string, Set<(e: { payload: unknown }) => void>>();
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(
    async (event: string, cb: (e: { payload: unknown }) => void) => {
      const set = listeners.get(event) ?? new Set();
      set.add(cb);
      listeners.set(event, set);
      return () => set.delete(cb);
    },
  ),
}));

import {
  makeStubs,
  discussionThread,
  renderWorkspace,
  showRail,
  versionRows,
  openActions,
  hideDiscussion,
  openComposer,
  postComposer,
  pngFile,
} from "../test/newArtifactFixtures";
import type { AgentTurn, ConversationOrigin, Participant } from "../types";
import { hunkCandidateBuffers } from "../state/candidateBuffers";
import { resetDraftDiscussions, restoreDraftRatios } from "../state/draftDiscussion";
import { resetLayoutPreferencesCache } from "../state/layoutPreferences";
import { clearAllDiscussionSessions } from "../state/discussionSession";
import { resetDiscussionFocus } from "../state/discussionFocus";
import {
  draftDiscussionOrigin,
} from "../test/origins";

const { stub, stubDiscussions } = makeStubs(invokeMock);

/** Every argument set one operation was invoked with. */
const callsTo = (cmd: string) =>
  invokeMock.mock.calls.filter((c) => c[0] === cmd).map((c) => c[1]);

/** A turn in flight, in the shape the backend reports it. */
function runningTurn(id: string, origin: ConversationOrigin): AgentTurn {
  return {
    id,
    agentId: "a1",
    nickname: "helga",
    origin,
    triggerCommentId: "c1",
    state: "running",
    failure: null,
    retryPermitted: false,
    imagesOmitted: false,
    activeToolCalls: [],
    startedAt: "2026-02-01T00:00:00Z",
    endedAt: null,
  } as unknown as AgentTurn;
}

beforeEach(() => {
  invokeMock.mockReset();
  listeners.clear();
  // DCR-FR-25: a candidate buffer outlives every surface by design, so it
  // carries from one test into the next unless a suite drops it.
  hunkCandidateBuffers.clear();
  // The per-draft view state is a module singleton keyed by draft id, and every
  // test in this file renders the same draft. A test that hid the discussion
  // column and failed before showing it again would otherwise leave every later
  // test looking at a column that is not there.
  resetDraftDiscussions();
  resetLayoutPreferencesCache();
  clearAllDiscussionSessions();
  resetDiscussionFocus();
  stub();
});

afterEach(cleanup);

// ---------------------------------------------------------------------------
// Discussions (NAW-FR-31, NAW-FR-32, ACT-FR-16, ACT-FR-19, CVP-FR-64, CMT-FR-53 … NAW-FR-33, CMT-FR-24, CMT-FR-26, CMT-FR-51, CMT-FR-52, CMT-FR-54, CMT-FR-55, CMT-FR-62 … CTA-FR-TTMS, CTA-FR-DGOC)
//
// A discussion is a comment thread about the draft as a WHOLE — no anchor, no
// file (CMS-FR-53). This tab begins one (NAW-FR-32); the comment margin renders
// and continues it (CMT-FR-53 … CTA-FR-QNBS).
// ---------------------------------------------------------------------------

describe("New Artifact discussions (NAW-FR-31 … NAW-FR-34, CMT-FR-52 … CMT-FR-62)", () => {
  it("NAW-FR-31, NAW-FR-32, ACT-FR-16, ACT-FR-19, CVP-FR-64, CMT-FR-53: posts a tagged message, dispatches once, and focuses the new card at the margin's head", async () => {
    stubDiscussions({ agents: ["arch", "sec"] });
    renderWorkspace();
    const composer = await openComposer();

    // NAW-FR-31: the mention picker, on the terms a comment composer offers it —
    // and opening it dismisses nothing, least of all the composer it is
    // completing a nickname in (NAW-FR-30, AGT-FR-30).
    await userEvent.type(composer, "Ask @");
    const picker = await screen.findByRole("listbox");
    expect(within(picker).getByText("@arch")).toBeInTheDocument();
    expect(within(picker).getByText("@sec")).toBeInTheDocument();
    expect(composer).toBeInTheDocument();

    fireEvent.change(composer, {
      target: { value: "@arch where should graduation live?" },
    });
    // NAW-FR-31: an attachment, on exactly the terms the rail's composers carry
    // one — dropped rather than picked, so the control need not be opened.
    await act(async () => {
      fireEvent.drop(composer.parentElement!, {
        dataTransfer: { files: [pngFile("chooser.png")] },
      });
    });
    await screen.findByText("chooser.png");

    await postComposer();

    // NAW-FR-32: the thread first, carrying the body and the strip's entries.
    await waitFor(() => expect(callsTo("open_discussion")).toHaveLength(1));
    const [opened] = callsTo("open_discussion") as [
      {
        target: { kind: string; draftId?: string };
        body: string;
        attachments: { filename?: string }[];
      },
    ];
    // CMS-FR-57: the target names what the discussion is about — this draft,
    // taken entire, which is what gives the turn its `draft_discussion` origin.
    expect(opened.target).toEqual({ kind: "draft", draftId: "d1" });
    expect(opened.body).toBe("@arch where should graduation live?");
    expect(opened.attachments).toHaveLength(1);
    expect(opened.attachments[0].filename).toBe("chooser.png");

    // Then exactly one turn, naming the returned thread with the discussion
    // origin and its opening comment as what addressed it.
    await waitFor(() => expect(callsTo("dispatch_agent_turn")).toHaveLength(1));
    expect(callsTo("dispatch_agent_turn")[0]).toEqual({
      nickname: "arch",
      origin: draftDiscussionOrigin("disc-1", "d1"),
      triggerCommentId: "c-1",
    });

    // NAW-FR-32 / DDS-FR-NWRL: the discussion is read in the column it was
    // posted from. The opening composer gives way to the conversation itself,
    // and no presentation instance is registered — there is nothing to attach,
    // detach, minimize or maximize, because the conversation is already where
    // it is read (CVP-FR-DKPW).
    const column = await screen.findByTestId("draft-discussion-column");
    await waitFor(() =>
      expect(within(column).getByTestId("comment-thread-disc-1")).toBeInTheDocument(),
    );
    expect(screen.queryByTestId("draft-open-discussion")).toBeNull();
    // DDS-FR-KTVW: no floating chat window anywhere in the tab.
    expect(document.querySelector(".action-control__panel")).toBeNull();
  });

  it("NAW-FR-32, CMT-FR-53: a message addressing nobody opens a discussion, and a second post opens a second one", async () => {
    stubDiscussions({ agents: ["arch"] });
    renderWorkspace();

    const first = await openComposer();
    fireEvent.change(first, { target: { value: "thinking aloud" } });
    await postComposer();
    await screen.findByTestId("comment-thread-disc-1");
    // NAW-FR-32: the message stands whether or not any agent answers it.
    expect(callsTo("dispatch_agent_turn")).toHaveLength(0);

    // NAW-FR-32: a second discussion is begun from the column's own control,
    // which puts the opening composer back in place of the conversation.
    await userEvent.click(screen.getByTestId("discussion-new"));
    const second = await screen.findByRole("textbox", {
      name: "Discuss this draft",
    });
    fireEvent.change(second, { target: { value: "a separate thought" } });
    await postComposer();

    // A second discussion rather than a comment added to the first, and the
    // column offers both in the order they were opened (CMT-FR-53).
    await waitFor(() =>
      expect(callsTo("open_discussion")).toHaveLength(2),
    );
    expect(callsTo("add_comment")).toHaveLength(0);
    const column = screen.getByTestId("draft-discussion-column");
    const tabs = within(column).getAllByRole("tab");
    expect(tabs).toHaveLength(2);
    // DDS-FR-NWRL: neither is a presentation instance.
  });

  it("ACT-FR-28: Cmd+Enter posts the opening composer, and does nothing while it is empty or posting", async () => {
    stubDiscussions({ agents: ["arch"] });
    renderWorkspace();
    const composer = await openComposer();

    // Empty: the accelerator does nothing.
    fireEvent.keyDown(composer, { key: "Enter", metaKey: true });
    expect(callsTo("open_discussion")).toHaveLength(0);

    fireEvent.change(composer, { target: { value: "is this ready?" } });
    fireEvent.keyDown(composer, { key: "Enter", metaKey: true });
    await waitFor(() =>
      expect(callsTo("open_discussion")).toHaveLength(1),
    );
    // A single press posted once, not twice.
    expect(callsTo("open_discussion")).toHaveLength(1);
  });

  it("NAW-FR-32, CMT-FR-55, CTA-FR-QUXJ: every later message is posted in the card rather than in the floating composer", async () => {
    stubDiscussions({
      agents: ["arch"],
      existing: [
        discussionThread("disc-1", [
          { id: "c1", author: { kind: "human", login: "raver119" } as Participant, body: "opening" },
        ]),
      ],
    });
    renderWorkspace();

    const card = await screen.findByTestId("comment-thread-disc-1");
    // CMT-FR-55: an ordinary card in every respect that does not depend on an
    // anchor — a composer, an attach control, a quote action, an overflow menu.
    const reply = within(card).getByLabelText("Reply to thread disc-1");
    expect(within(card).getByTestId("comment-attach-button")).toBeInTheDocument();
    expect(
      within(card).getByRole("button", { name: /^Quote comment 1/ }),
    ).toBeInTheDocument();
    expect(
      within(card).getByRole("button", { name: "Thread actions for disc-1" }),
    ).toBeInTheDocument();
    // …and none that does: no anchor quote, and nothing offering to reanchor it.
    expect(card.querySelector(".comment-card__quote")).toBeNull();

    fireEvent.change(reply, { target: { value: "a follow-up" } });
    await userEvent.click(within(card).getByRole("button", { name: "Post" }));

    await waitFor(() => expect(callsTo("add_comment")).toHaveLength(1));
    expect(callsTo("add_comment")[0]).toMatchObject({
      draftId: "d1",
      discussionId: "disc-1",
      body: "a follow-up",
    });
    // The floating composer was not involved at any point.
    expect(callsTo("open_discussion")).toHaveLength(0);
    expect(
      screen.queryByRole("textbox", { name: "Discuss this draft" }),
    ).not.toBeInTheDocument();
    expect(callsTo("reanchor_discussion_fragment")).toHaveLength(0);
  });

  it("NAW-FR-33, CMT-FR-24, CMT-FR-26, CMT-FR-51: is disabled while no identity resolves, and keeps body and strip when a post is refused", async () => {
    // NAW-FR-33: a discussion is a comment thread and cannot be attributed to
    // nobody, so the composer states the same reason the margin's composers
    // state and offers the same route (CMT-FR-24, CMT-FR-26).
    stubDiscussions({ identityError: "github_token_missing" });
    renderWorkspace();
    const blocked = await openComposer();
    expect(blocked).toBeDisabled();
    expect(
      screen.getByText(/needs a GitHub account to attribute comments to/i),
    ).toBeInTheDocument();
    expect(screen.getByText(/Global settings → GitHub/)).toBeInTheDocument();
    expect(
      within(screen.getByTestId("draft-open-discussion")).getByRole("button", {
        name: "Post",
      }),
    ).toBeDisabled();
    expect(callsTo("open_discussion")).toHaveLength(0);

    cleanup();
    invokeMock.mockReset();
    stubDiscussions({ openError: "unsupported_media_type" });
    renderWorkspace();
    const composer = await openComposer();
    fireEvent.change(composer, { target: { value: "look at this" } });
    await act(async () => {
      fireEvent.drop(composer.parentElement!, {
        dataTransfer: { files: [pngFile("shot.png")] },
      });
    });
    await screen.findByText("shot.png");
    await postComposer();

    // The typed error renders in the composer, no thread was opened, no turn was
    // dispatched, and the body and the strip are still there (CMT-FR-51).
    expect(await screen.findByRole("alert")).toHaveTextContent(
      /unsupported_media_type|media type/i,
    );
    expect(callsTo("dispatch_agent_turn")).toHaveLength(0);
    expect(
      screen.getByRole("textbox", { name: "Discuss this draft" }),
    ).toHaveValue("look at this");
    expect(screen.getByText("shot.png")).toBeInTheDocument();
    // Nothing was opened, so the column is still offering the opening composer.
    expect(screen.getByTestId("draft-open-discussion")).toBeInTheDocument();
    expect(screen.queryByTestId("comment-thread-disc-1")).toBeNull();
  });

  it("NAW-FR-05, NAW-FR-14, DDS-FR-KTVW, DDS-FR-KPSZ, DDS-FR-NWRL: the conversation is a column of the tab, and the field lends it no margin", async () => {
    // The discussion used to be a ~370px margin beside the page, which is why it
    // was pulled out into a floating window that covered the draft. It is a
    // column now: the field beside the page lends it nothing, the page keeps its
    // whole measure, and the anchored threads take the arrangement CMT-FR-64
    // defines below the page rather than beside it.
    stubDiscussions({ existing: [] });
    renderWorkspace();
    const field = () =>
      document.querySelector(".draft-editor") as HTMLElement | null;
    await waitFor(() => expect(field()).not.toBeNull());
    expect(document.querySelector(".comment-rail")).toBeNull();
    expect(field()!.dataset.rail).toBe("closed");
    // In jsdom nothing has a width, so the arrangement is the ordinary one.
    expect(field()!.dataset.comments).toBe("beside");
    // DDS-FR-KTVW: the column stands whether or not anything has been said.
    expect(screen.getByTestId("draft-discussion-column")).toBeInTheDocument();

    cleanup();
    stubDiscussions({
      existing: [
        discussionThread("disc-1", [
          {
            id: "c1",
            author: { kind: "human", login: "raver119" } as Participant,
            body: "the whole thing",
          },
        ]),
      ],
    });
    renderWorkspace();
    const column = await screen.findByTestId("draft-discussion-column");
    await within(column).findByTestId("comment-thread-disc-1");
    // A discussion costs the document column nothing: no margin opens beside
    // the page, and the page's field is exactly as it was.
    expect(document.querySelector(".comment-rail")).toBeNull();
    expect(field()!.dataset.rail).toBe("closed");
  });

  it("CMT-FR-52, CMT-FR-54, CMT-FR-55, DDS-FR-KTVW: the discussion is read once and survives what the page is showing", async () => {
    stubDiscussions({
      existing: [
        discussionThread("disc-1", [
          { id: "c1", author: { kind: "human", login: "raver119" } as Participant, body: "the whole thing" },
        ]),
      ],
    });
    renderWorkspace();

    const column = await screen.findByTestId("draft-discussion-column");
    const card = await within(column).findByTestId("comment-thread-disc-1");
    expect(callsTo("list_discussions")).toHaveLength(1);
    // CMT-FR-55: a discussion is about the draft as a whole, so it carries no
    // anchor quote and highlights nothing in the prompt.
    expect(card.querySelector(".comment-card__quote")).toBeNull();

    // CMT-FR-54 / DDS-FR-WCHN: showing the History rail and reading a past
    // version leaves the discussion exactly as it stands. It belongs to the
    // draft rather than to what the document column happens to be showing.
    await showRail();
    await userEvent.click(versionRows()[0]);
    await screen.findByTestId("draft-version-reading");
    expect(
      within(screen.getByTestId("draft-discussion-column")).getByTestId(
        "comment-thread-disc-1",
      ),
    ).toBeInTheDocument();
    expect(callsTo("list_discussions")).toHaveLength(1);
  });

  it("CMT-FR-17, CMT-FR-56: a resolved discussion leaves the column and comes back when it is reopened", async () => {
    stubDiscussions({
      existing: [
        discussionThread("disc-1", [
          { id: "c1", author: { kind: "human", login: "raver119" } as Participant, body: "one" },
        ]),
        discussionThread("disc-2", [
          { id: "c2", author: { kind: "human", login: "raver119" } as Participant, body: "two" },
        ]),
      ],
    });
    renderWorkspace();

    const column = await screen.findByTestId("draft-discussion-column");
    // Two discussions, so the column offers a chooser between them.
    expect(within(column).getAllByRole("tab")).toHaveLength(2);

    const card = await within(column).findByTestId("comment-thread-disc-1");
    await userEvent.click(
      within(card).getByRole("button", { name: "Thread actions for disc-1" }),
    );
    await userEvent.click(screen.getByRole("menuitem", { name: "Mark resolved" }));

    // CMT-FR-56: a resolved discussion is no longer one of the draft's open
    // conversations, so the column stops offering it and reads the other.
    await waitFor(() =>
      expect(
        within(screen.getByTestId("draft-discussion-column")).getAllByRole("tab"),
      ).toHaveLength(1),
    );
    await within(screen.getByTestId("draft-discussion-column")).findByTestId(
      "comment-thread-disc-2",
    );
  });

  it("CMT-FR-17, CMT-FR-56, DDS-FR-DPRJ: resolving the last discussion leaves it reachable and says nothing about following", async () => {
    // What the author hit. Resolving the only discussion used to take the
    // chooser off the surface with it — and the disclosure that reaches a
    // resolved discussion is pinned at that chooser's foot, so the
    // conversation became unreachable and the column claimed nothing had ever
    // been said about the draft.
    stubDiscussions({
      existing: [
        discussionThread("disc-1", [
          { id: "c1", author: { kind: "human", login: "raver119" } as Participant, body: "one" },
        ]),
      ],
    });
    renderWorkspace();

    const column = await screen.findByTestId("draft-discussion-column");
    const card = await within(column).findByTestId("comment-thread-disc-1");
    await userEvent.click(
      within(card).getByRole("button", { name: "Thread actions for disc-1" }),
    );
    await userEvent.click(screen.getByRole("menuitem", { name: "Mark resolved" }));

    const settled = await screen.findByTestId("draft-discussion-column");
    const disclosure = await within(settled).findByRole("button", {
      name: /1 resolved/,
    });
    // DDS-FR-DPRJ: no transcript stands, so the header says nothing about
    // following. Asserted here rather than after the disclosure is expanded —
    // expanding it puts the discussion back on screen, and the header is then
    // right to speak again.
    expect(
      within(settled).queryByTestId("discussion-follow-state"),
    ).toBeNull();

    await userEvent.click(disclosure);
    expect(
      within(screen.getByTestId("draft-discussion-column")).getByTestId(
        "discussion-follow-state",
      ),
    ).toHaveTextContent("following");
    expect(
      await within(settled).findByTestId("comment-thread-disc-1"),
    ).toBeInTheDocument();
  });

  it("DDS-FR-XQMF: one control hides the discussion column and gives the document the whole tab, and the same control brings it back", async () => {
    stubDiscussions({ existing: [] });
    renderWorkspace();

    const column = await screen.findByTestId("draft-discussion-column");
    expect(column).toBeVisible();
    const split = screen.getByTestId("draft-discussion-split");
    expect(split.style.gridTemplateColumns).not.toBe("1fr");
    expect(screen.getByRole("radiogroup", { name: /Split between/ })).toBeInTheDocument();

    const toggle = screen.getByTestId("draft-discussion-toggle");
    expect(toggle).toHaveAttribute("aria-pressed", "true");
    await userEvent.click(toggle);

    // The document takes the tab's full width, the column leaves the layout and
    // the accessibility tree, and the presets that arrange two columns stand
    // down — there is no split left to cycle.
    expect(screen.getByTestId("draft-discussion-column")).not.toBeVisible();
    expect(screen.queryByRole("region", { name: "Discussion" })).toBeNull();
    expect(screen.getByTestId("draft-discussion-split").style.gridTemplateColumns).toBe(
      "1fr",
    );
    expect(screen.getByTestId("draft-discussion-split")).toHaveAttribute(
      "data-discussion",
      "hidden",
    );
    expect(screen.queryByRole("radiogroup", { name: /Split between/ })).toBeNull();
    expect(
      screen.queryByRole("separator", { name: /Resize the document/ }),
    ).toBeNull();

    // The control stands in both states, so the conversation is never out of
    // reach.
    const back = screen.getByTestId("draft-discussion-toggle");
    expect(back).toHaveAttribute("aria-pressed", "false");
    expect(back).toHaveAccessibleName("Show the discussion");
    await userEvent.click(back);
    expect(screen.getByTestId("draft-discussion-column")).toBeVisible();
  });

  it("DDS-FR-XQMF: hiding the column keeps an unsent message, the discussion being read, and the reading position", async () => {
    // The column is hidden, not unmounted. An author who is part way through a
    // reply and hides the column to read the draft must find that reply where
    // they left it — losing it silently is the worst thing this control could
    // do, and it is invisible until it has already happened.
    stubDiscussions({
      existing: [
        discussionThread("disc-1", [
          { id: "c1", author: { kind: "human", login: "raver119" } as Participant, body: "one" },
        ]),
      ],
    });
    renderWorkspace();

    const card = await screen.findByTestId("comment-thread-disc-1");
    const reply = within(card).getByLabelText("Reply to thread disc-1");
    fireEvent.change(reply, { target: { value: "half a sentence" } });

    await userEvent.click(screen.getByTestId("draft-discussion-toggle"));
    await userEvent.click(screen.getByTestId("draft-discussion-toggle"));

    expect(
      within(await screen.findByTestId("comment-thread-disc-1")).getByLabelText(
        "Reply to thread disc-1",
      ),
    ).toHaveValue("half a sentence");
  });

  it("DDS-FR-XQMF, SNV-FR-08: a draft whose column was hidden opens with it hidden", async () => {
    // The clause the round trip alone does not establish: a record read back at
    // launch has to reach the tab that mounts for that draft. Two drafts are
    // stored so it is the draft's own entry that decides, not the map holding
    // any entry at all.
    stubDiscussions({ existing: [] });
    const base = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(async (cmd: string, args: unknown) =>
      cmd === "load_layout_preferences"
        ? { draftDiscussionHidden: { d1: true, d9: false } }
        : base(cmd, args),
    );
    await act(async () => {
      await restoreDraftRatios("/repo");
    });

    renderWorkspace();

    expect(await screen.findByTestId("draft-discussion-column")).not.toBeVisible();
    expect(screen.getByTestId("draft-discussion-split").style.gridTemplateColumns).toBe(
      "1fr",
    );
    expect(screen.getByTestId("draft-discussion-toggle")).toHaveAccessibleName(
      "Show the discussion",
    );
  });

  it("ACT-FR-23, SNV-FR-08, NAW-FR-34: a draft restored with its column hidden opens with Discuss live", async () => {
    // The state a returning author actually lands in. The hidden column is
    // restored per draft, so the action that brings it back has to be reachable
    // the moment the tab opens rather than only after a toggle in this sitting.
    stubDiscussions({ existing: [] });
    const base = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(async (cmd: string, args: unknown) =>
      cmd === "load_layout_preferences"
        ? { draftDiscussionHidden: { d1: true } }
        : base(cmd, args),
    );
    await act(async () => {
      await restoreDraftRatios("/repo");
    });
    renderWorkspace();
    expect(await screen.findByTestId("draft-discussion-column")).not.toBeVisible();

    await openActions();
    const discuss = screen.getByRole("menuitem", { name: "Discuss" });
    expect(discuss).toBeEnabled();
    expect(discuss).toHaveAttribute(
      "title",
      "Discuss this with the agents you address",
    );
  });

  it("ACT-FR-QWNP, DDS-FR-JWNC: Discuss focuses the composer the column is actually standing", async () => {
    // The column stands its composer in two different boxes, and which one is
    // at the foot depends on whether anything has been said. On a draft with a
    // transcript it is the reply of the card at the foot — so a focus effect
    // that reached for the opener by name would find nothing here.
    stubDiscussions({
      existing: [
        discussionThread("disc-1", [
          {
            id: "c1",
            author: { kind: "human", login: "raver119" } as Participant,
            body: "the opening line",
          },
        ]),
      ],
    });
    renderWorkspace();
    await screen.findByRole("button", { name: "Edit as Markdown source" });
    await hideDiscussion();
    await openActions();
    await userEvent.click(screen.getByRole("menuitem", { name: "Discuss" }));

    await waitFor(() =>
      expect(screen.getByTestId("draft-discussion-column")).toBeVisible(),
    );
    // The caret is in a composer of this column, whichever box holds it — and
    // not left on the control the author pressed.
    await waitFor(() => {
      const active = document.activeElement;
      expect(active?.tagName).toBe("TEXTAREA");
      expect(
        screen.getByTestId("draft-discussion-column").contains(active),
      ).toBe(true);
    });
  });

  it("DDS-FR-XQMF: every preset leaves both columns standing", async () => {
    // The splitter and the presets arrange two columns; neither of them hides
    // one. The narrowest either is dragged to is a column that can still be
    // read, which is what makes hiding a different thing from narrowing.
    stubDiscussions({ existing: [] });
    renderWorkspace();
    const column = await screen.findByTestId("draft-discussion-column");

    for (const preset of ["Document", "Even", "Discussion"]) {
      await userEvent.click(screen.getByRole("radio", { name: preset }));
      expect(column).toBeVisible();
      const tracks = screen
        .getByTestId("draft-discussion-split")
        .style.gridTemplateColumns.split(" ");
      expect(tracks).toHaveLength(3);
      // No track collapses to nothing, at either end of the cycle.
      expect(tracks[0]).not.toBe("0fr");
      expect(tracks[2]).not.toBe("0fr");
      expect(screen.getByTestId("draft-discussion-toggle")).toHaveAttribute(
        "aria-pressed",
        "true",
      );
    }
  });

  it("DDS-FR-PNXR, DDS-FR-XQMF: the accelerator cycles the split, and does nothing while there is no split to cycle", async () => {
    stubDiscussions({ existing: [] });
    renderWorkspace();
    await screen.findByTestId("draft-discussion-column");

    const cycle = () =>
      fireEvent.keyDown(window, { key: "\\", metaKey: true });
    const grid = () =>
      screen.getByTestId("draft-discussion-split").style.gridTemplateColumns;

    const opened = grid();
    act(() => cycle());
    const cycled = grid();
    expect(cycled).not.toBe(opened);

    // Hidden, the accelerator rearranges nothing — a keypress that silently
    // moved a split the author cannot see would land them on another one the
    // next time they showed the column.
    await userEvent.click(screen.getByTestId("draft-discussion-toggle"));
    act(() => cycle());
    await userEvent.click(screen.getByTestId("draft-discussion-toggle"));
    expect(grid()).toBe(cycled);
  });

  it("CTA-FR-TTMS, CTA-FR-DGOC: an agent once addressed answers every following message, and an explicit tag replaces the set", async () => {
    stubDiscussions({
      agents: ["arch", "sec"],
      existing: [
        discussionThread("disc-1", [
          {
            id: "c1",
            author: { kind: "human", login: "raver119" } as Participant,
            body: "@arch @sec where should graduation live?",
          },
        ]),
      ],
    });
    renderWorkspace();

    const card = await screen.findByTestId("comment-thread-disc-1");
    const reply = within(card).getByLabelText("Reply to thread disc-1");

    // CTA-FR-HCMJ: a comment carrying no tag reaches the conversation's active
    // agents — the two the newest naming comment named (CTA-FR-XBIN).
    fireEvent.change(reply, { target: { value: "and the archive part?" } });
    await userEvent.click(within(card).getByRole("button", { name: "Post" }));
    await waitFor(() => expect(callsTo("dispatch_agent_turn")).toHaveLength(2));
    expect(
      (callsTo("dispatch_agent_turn") as { nickname: string }[]).map(
        (c) => c.nickname,
      ),
    ).toEqual(["arch", "sec"]);
    for (const call of callsTo("dispatch_agent_turn") as {
      origin: unknown;
    }[]) {
      expect(call.origin).toEqual(draftDiscussionOrigin("disc-1", "d1"));
    }

    // …and one carrying a tag reaches exactly that agent, and by standing in the
    // conversation makes it the one active agent from then on — an explicit
    // mention replaces the set rather than joining it (CTA-FR-QUXJ).
    invokeMock.mockClear();
    fireEvent.change(reply, { target: { value: "@arch only, please" } });
    await userEvent.click(within(card).getByRole("button", { name: "Post" }));
    await waitFor(() => expect(callsTo("dispatch_agent_turn")).toHaveLength(1));
    expect(callsTo("dispatch_agent_turn")[0]).toMatchObject({ nickname: "arch" });
  });

  it("CTA-FR-UUXA: a mention the backend refuses shows the refusal at the foot of the discussion", async () => {
    stubDiscussions({
      agents: ["helga"],
      dispatchError: "agent_not_found",
      existing: [
        discussionThread("disc-1", [
          { id: "c1", author: { kind: "human", login: "raver119" } as Participant, body: "thinking aloud" },
        ]),
      ],
    });
    renderWorkspace();

    const card = await screen.findByTestId("comment-thread-disc-1");
    const reply = within(card).getByLabelText("Reply to thread disc-1");
    fireEvent.change(reply, { target: { value: "any more questions? @helga ?" } });
    await userEvent.click(within(card).getByRole("button", { name: "Post" }));

    await waitFor(() => expect(callsTo("dispatch_agent_turn")).toHaveLength(1));
    // AGC-FR-05: the origin the backend reads, not the old tagged one.
    expect(callsTo("dispatch_agent_turn")[0]).toMatchObject({
      nickname: "helga",
      origin: draftDiscussionOrigin("disc-1", "d1"),
    });
    // The author's message stands, and the refusal says what went wrong.
    expect(await within(card).findByTestId("comment-turn-error")).toHaveTextContent(
      "That agent is not enrolled in this project.",
    );
    expect(card).toHaveTextContent("any more questions?");
  });

  it("CTA-FR-UUXA: a refusal stands when another agent of the same message is accepted, and the next accepted message retires it", async () => {
    stubDiscussions({
      agents: ["helga", "arch"],
      existing: [
        discussionThread("disc-1", [
          { id: "c1", author: { kind: "human", login: "raver119" } as Participant, body: "thinking aloud" },
        ]),
      ],
    });
    // `arch` is refused while `helga` is accepted, then `arch` comes back.
    const served = invokeMock.getMockImplementation()!;
    let archRefused = true;
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (
        cmd === "dispatch_agent_turn" &&
        (args as { nickname: string }).nickname === "arch" &&
        archRefused
      ) {
        throw "agent_not_found";
      }
      return served(cmd, args);
    });
    renderWorkspace();

    const card = await screen.findByTestId("comment-thread-disc-1");
    const reply = within(card).getByLabelText("Reply to thread disc-1");
    fireEvent.change(reply, { target: { value: "@arch @helga ?" } });
    await userEvent.click(within(card).getByRole("button", { name: "Post" }));

    await waitFor(() => expect(callsTo("dispatch_agent_turn")).toHaveLength(2));
    expect(await within(card).findByTestId("comment-turn-error")).toHaveTextContent(
      "That agent is not enrolled in this project.",
    );

    archRefused = false;
    fireEvent.change(reply, { target: { value: "@arch again?" } });
    await userEvent.click(within(card).getByRole("button", { name: "Post" }));
    await waitFor(() => expect(callsTo("dispatch_agent_turn")).toHaveLength(3));
    await waitFor(() =>
      expect(within(card).queryByTestId("comment-turn-error")).toBeNull(),
    );
  });

  it("CTA-FR-MGVJ, AGC-FR-05: a recoverable failure the backend reports for this discussion offers Retry there, and one for another discussion does not", async () => {
    stubDiscussions({
      agents: ["helga"],
      existing: [
        discussionThread("disc-1", [
          { id: "c1", author: { kind: "human", login: "raver119" } as Participant, body: "@helga thoughts?" },
        ]),
        discussionThread("disc-2", [
          { id: "c2", author: { kind: "human", login: "raver119" } as Participant, body: "@helga and this?" },
        ]),
      ],
    });
    const failed = (id: string, discussionId: string) =>
      ({
        ...runningTurn(id, draftDiscussionOrigin(discussionId, "d1")),
        state: "failed",
        failure: "unreachable",
        retryPermitted: true,
        endedAt: "2026-02-01T00:01:00Z",
      }) as AgentTurn;
    const served = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) =>
      cmd === "list_recoverable_agent_turn_failures"
        ? [failed("turn-other", "disc-2"), failed("turn-failed", "disc-1")]
        : served(cmd, args),
    );
    renderWorkspace();

    // The column shows one discussion at a time: disc-1 carries its own offer
    // and not the one disc-2's failure makes.
    const card = await screen.findByTestId("comment-thread-disc-1");
    expect(
      await within(card).findByRole("button", { name: /retry/i }),
    ).toBeInTheDocument();
    expect(within(card).getAllByRole("button", { name: /retry/i })).toHaveLength(1);
  });

  it("CVP-FR-47, AGC-FR-05: a running turn the backend reports for this discussion shows that the agent is answering", async () => {
    stubDiscussions({
      agents: ["helga"],
      existing: [
        discussionThread("disc-1", [
          { id: "c1", author: { kind: "human", login: "raver119" } as Participant, body: "@helga thoughts?" },
        ]),
      ],
      // The shape `list_agent_turns` answers with: the origin names the
      // discussion by `discussionId`.
      turns: [runningTurn("turn-listed", draftDiscussionOrigin("disc-1", "d1"))],
    });
    renderWorkspace();

    const card = await screen.findByTestId("comment-thread-disc-1");
    expect(await within(card).findByText(/thinking/i)).toBeInTheDocument();
  });

  it("CVP-FR-47, AGC-FR-05: a turn the backend announces for this discussion shows that the agent is answering, and one for another discussion does not", async () => {
    stubDiscussions({
      agents: ["helga"],
      existing: [
        discussionThread("disc-1", [
          { id: "c1", author: { kind: "human", login: "raver119" } as Participant, body: "@helga thoughts?" },
        ]),
      ],
    });
    renderWorkspace();
    const card = await screen.findByTestId("comment-thread-disc-1");
    await waitFor(() =>
      expect(listeners.get("agent-turn-state-changed")?.size ?? 0).toBeGreaterThan(0),
    );

    const emit = (turn: AgentTurn) =>
      act(() => {
        for (const cb of listeners.get("agent-turn-state-changed") ?? []) {
          cb({ payload: turn });
        }
      });
    emit(runningTurn("turn-other", draftDiscussionOrigin("disc-2", "d1")));
    expect(within(card).queryByText(/thinking/i)).toBeNull();

    emit(runningTurn("turn-event", draftDiscussionOrigin("disc-1", "d1")));
    expect(await within(card).findByText(/thinking/i)).toBeInTheDocument();
  });

  it("CTA-FR-TTMS, CTA-FR-DGOC: a discussion no agent has been tagged in dispatches nothing", async () => {
    stubDiscussions({
      agents: ["arch"],
      existing: [
        discussionThread("disc-1", [
          { id: "c1", author: { kind: "human", login: "raver119" } as Participant, body: "thinking aloud" },
        ]),
      ],
    });
    renderWorkspace();

    const card = await screen.findByTestId("comment-thread-disc-1");
    const reply = within(card).getByLabelText("Reply to thread disc-1");
    fireEvent.change(reply, { target: { value: "still thinking" } });
    await userEvent.click(within(card).getByRole("button", { name: "Post" }));

    await waitFor(() => expect(callsTo("add_comment")).toHaveLength(1));
    expect(callsTo("dispatch_agent_turn")).toHaveLength(0);
  });

  it("CTA-FR-XTZC, CTA-FR-DWCK: an agent's answer enlists nobody and dispatches nothing", async () => {
    // CTA-FR-XTZC / CTA-FR-YWSU: only a comment a person posted starts a round of
    // turns, and a tag an agent writes adds no participant — so a following
    // untagged message reaches `@arch` alone.
    stubDiscussions({
      agents: ["arch", "sec"],
      existing: [
        discussionThread("disc-1", [
          { id: "c1", author: { kind: "human", login: "raver119" } as Participant, body: "@arch thoughts?" },
          {
            id: "c2",
            author: { kind: "agent", agentId: "a1", handle: "arch" } as Participant,
            body: "@sec should weigh in",
          },
        ]),
      ],
    });
    renderWorkspace();

    const card = await screen.findByTestId("comment-thread-disc-1");
    // The agent's arrival dispatched nothing of its own.
    expect(callsTo("dispatch_agent_turn")).toHaveLength(0);

    const reply = within(card).getByLabelText("Reply to thread disc-1");
    fireEvent.change(reply, { target: { value: "carry on" } });
    await userEvent.click(within(card).getByRole("button", { name: "Post" }));
    await waitFor(() => expect(callsTo("dispatch_agent_turn")).toHaveLength(1));
    expect(callsTo("dispatch_agent_turn")[0]).toMatchObject({ nickname: "arch" });
  });
});
