/**
 * The draft column's part in a discussion about one change, and the menu its
 * header holds (`../../specifications/ui/DDS-draft-discussion.md` DDS-FR-RHKW,
 * DDS-FR-WKTQ, DDS-FR-YLRM, DDS-FR-MZQJ; `DCR-draft-change-review.md`
 * DCR-FR-MTFD).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { useState } from "react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

import { resetDraftDiscussions } from "../state/draftDiscussion";
import { resetLayoutPreferencesCache } from "../state/layoutPreferences";
import {
  clearAllDiscussionSessions,
  getDiscussionSession,
  setComposerBody,
} from "../state/discussionSession";
import { forgetThreads, heldThreads } from "../state/conversationThreads";
import { noteProposalChanged, resetDraftProposals } from "../state/draftProposals";
import { resetProposalHunks } from "../state/proposalHunks";
import { resetLogBufferForTest } from "../logging";
import type { HunkOpening } from "./DraftDiscussion/OpenDiscussion";
import {
  DRAFT,
  HUMAN,
  type ColumnProps,
  columnElement,
  comment,
  discussion,
  discussionSaying,
  fragmentDiscussion,
  hunkReading,
  hunkReference,
  proposalHunk,
  proposalOf,
} from "../test/draftDiscussionFixtures";
import { discussionTargetKey, type Discussion, type ProposalHunkReference } from "../types";

const reference: ProposalHunkReference = {
  kind: "proposalHunk",
  proposalId: "p1",
  hunkId: "h2",
  draftId: DRAFT,
  path: "Spec.md",
};

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "append_log_records") return Promise.resolve();
    if (cmd === "load_draft_change_proposal_hunks") {
      return Promise.resolve(
        hunkReading([proposalHunk("h1"), proposalHunk("h2"), proposalHunk("h3")]),
      );
    }
    if (cmd === "list_draft_change_proposals") return Promise.resolve([]);
    return Promise.reject(new Error(`unexpected command ${cmd}`));
  });
  resetDraftDiscussions();
  resetDraftProposals();
  resetProposalHunks();
  resetLayoutPreferencesCache();
  clearAllDiscussionSessions();
  resetLogBufferForTest();
  for (const d of heldThreads()) forgetThreads([d.id]);
});

afterEach(cleanup);

describe("DDS-FR-RHKW: the composer of a discussion about one change", () => {
  it("DDS-FR-RHKW, DCR-FR-MTFD: names the change and posts a new discussion carrying its reference", async () => {
    const onOpenDiscussion = vi.fn(async () => discussionSaying("disc-9", ["x"]));
    render(
      columnElement({
        onOpenDiscussion,
        openingHunk: { reference, position: 2, total: 3 },
      }),
    );

    // DDS-FR-RHKW: the composer names its change, whichever discussion the
    // column was reading.
    expect(screen.getByTestId("draft-open-discussion")).toHaveTextContent(
      "This begins a discussion about change 2 of 3.",
    );
    const field = await screen.findByRole("textbox", { name: "Discuss this draft" });
    fireEvent.change(field, { target: { value: "@arch why this word?" } });
    await userEvent.click(screen.getByRole("button", { name: "Post" }));

    // DCR-FR-MTFD: a new discussion record, whole-target, with one
    // `proposal_hunk` attachment naming the change.
    await waitFor(() => expect(onOpenDiscussion).toHaveBeenCalledTimes(1));
    expect(onOpenDiscussion).toHaveBeenCalledWith({
      target: { kind: "draft", draftId: DRAFT },
      fragmentTarget: null,
      body: "@arch why this word?",
      attachments: [reference],
    });
  });

  it("DDS-FR-RHKW: the caret goes after the text the composer holds", async () => {
    setComposerBody(discussionTargetKey({ kind: "draft", draftId: DRAFT }), "@arch ");
    render(columnElement({ openingHunk: { reference, position: 2, total: 3 } }));
    const field = await screen.findByRole<HTMLTextAreaElement>("textbox", {
      name: "Discuss this draft",
    });
    await waitFor(() => expect(field).toHaveFocus());
    // Every read the strip asks for has answered, so nothing renders after the
    // caret is placed.
    await screen.findByTestId("discussion-subject-after");
    expect(await screen.findByTestId("discussion-subject-state")).toHaveTextContent(
      "no longer available",
    );
    expect(field).toHaveValue("@arch ");
    expect(field.selectionStart).toBe("@arch ".length);
    expect(field.selectionEnd).toBe("@arch ".length);
  });

  it("DDS-FR-RHKW: discarding the composer gives the opening back", async () => {
    const onCancelHunk = vi.fn();
    const onCancelFragment = vi.fn();
    render(
      columnElement({
        openingHunk: { reference, position: 2, total: 3 },
        onCancelHunk,
        onCancelFragment,
      }),
    );
    await userEvent.click(await screen.findByRole("button", { name: "Cancel" }));
    expect(onCancelHunk).toHaveBeenCalledTimes(1);
    expect(onCancelFragment).not.toHaveBeenCalled();
  });
});

/** A discussion whose opening comment carries this change's reference. */
function hunkDiscussion(id: string, ref: ProposalHunkReference): Discussion {
  return discussion(id, [comment(`${id}-c0`, HUMAN, "about the change", { attachments: [ref] })]);
}

const seedProposal = () =>
  act(() =>
    noteProposalChanged({
      draftId: DRAFT,
      proposal: proposalOf(["pending", "pending", "pending"]),
    }),
  );

describe("DDS-FR-WKTQ: the subject strip in the column", () => {
  it("DDS-FR-WKTQ, DDS-FR-RHKW: stands for the composer of a hunk discussion", async () => {
    render(columnElement({ openingHunk: { reference, position: 2, total: 3 } }));
    expect(screen.getByTestId("discussion-subject")).toHaveTextContent("Change 2 of 3");
    await screen.findByTestId("discussion-subject-after");
  });

  it("DDS-FR-WKTQ: stands for a hunk discussion, below the header and outside the scroller", async () => {
    seedProposal();
    render(
      columnElement({
        discussions: [hunkDiscussion("disc-h", hunkReference("h1"))],
        selectedThreadId: "disc-h",
      }),
    );
    await screen.findByTestId("comment-thread-disc-h");
    const strip = screen.getByTestId("discussion-subject");
    expect(strip).toHaveTextContent("Change 1 of 3");
    expect(strip.closest(".comment-card__messages")).toBeNull();
    expect(strip.previousElementSibling?.tagName).toBe("HEADER");
    // The tab names what the discussion is about.
    expect(screen.getByRole("tab", { name: /1/ })).toHaveAttribute(
      "title",
      "Discussion 1: a change to Spec.md",
    );
    await screen.findByTestId("discussion-subject-after");
  });

  it.each([
    ["a whole-target discussion", discussionSaying("disc-x", ["opening"])],
    ["a fragment discussion", fragmentDiscussion("disc-x", "the ontology")],
    [
      "a discussion where only a later comment names a change",
      discussion("disc-x", [
        comment("c0", HUMAN, "opening"),
        comment("c1", HUMAN, "see this", { attachments: [hunkReference("h1")] }),
      ]),
    ],
  ])("DDS-FR-WKTQ: no strip stands for %s", async (_name, d) => {
    render(columnElement({ discussions: [d], selectedThreadId: "disc-x" }));
    await screen.findByTestId("comment-thread-disc-x");
    expect(screen.queryByTestId("discussion-subject")).toBeNull();
  });

  it("DDS-FR-WKTQ, DDS-FR-RHKW: the composer's change is the subject, whichever discussion was being read", async () => {
    seedProposal();
    render(
      columnElement({
        discussions: [hunkDiscussion("disc-h", hunkReference("h1"))],
        selectedThreadId: "disc-h",
        openingHunk: { reference: hunkReference("h2"), position: 2, total: 3 },
      }),
    );
    expect(screen.getByTestId("discussion-subject")).toHaveTextContent("Change 2 of 3");
    await screen.findByTestId("discussion-subject-after");
  });
});

/**
 * The column as the workspace holds it: the opening of a hunk discussion and
 * the selected discussion are the workspace's state, and the column's
 * callbacks are what change them.
 */
function Hosted({
  discussions,
  spies,
}: {
  discussions: Discussion[];
  spies: { cancelHunk: () => void; cancelFragment: () => void; select: (id: string) => void };
}) {
  const [opening, setOpening] = useState<HunkOpening | null>({
    reference,
    position: 2,
    total: 3,
  });
  const [selected, setSelected] = useState<string | null>(null);
  const props: Partial<ColumnProps> = {
    discussions,
    selectedThreadId: selected,
    onSelectThread: (id) => {
      spies.select(id);
      setSelected(id);
    },
    openingHunk: opening,
    onCancelHunk: () => {
      spies.cancelHunk();
      setOpening(null);
    },
    onCancelFragment: spies.cancelFragment,
  };
  return columnElement(props);
}

describe("DDS-FR-YLRM: leaving the composer of a hunk discussion", () => {
  const spies = () => ({ cancelHunk: vi.fn(), cancelFragment: vi.fn(), select: vi.fn() });
  const draftKey = discussionTargetKey({ kind: "draft", draftId: DRAFT });

  it("DDS-FR-YLRM: a tab leaves it for the discussion chosen, and the typed text stays", async () => {
    const s = spies();
    render(<Hosted discussions={[discussionSaying("disc-1", ["opening"])]} spies={s} />);
    const field = await screen.findByRole("textbox", { name: "Discuss this draft" });
    await userEvent.type(field, "draft text");

    await userEvent.click(screen.getByRole("tab", { name: /1/ }));

    expect(s.cancelHunk).toHaveBeenCalledTimes(1);
    expect(s.select).toHaveBeenCalledWith("disc-1");
    expect(await screen.findByTestId("comment-thread-disc-1")).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /1/ })).toHaveAttribute("aria-selected", "true");
    expect(screen.queryByTestId("discussion-subject")).toBeNull();
    expect(screen.queryByText(/This begins a discussion about change/)).toBeNull();
    expect(getDiscussionSession(draftKey).body).toBe("draft text");

    // The text waits in the draft's composer for the next discussion.
    await userEvent.click(screen.getByTestId("discussion-new"));
    expect(screen.getByRole("textbox", { name: "Discuss this draft" })).toHaveValue("draft text");
  });

  it("DDS-FR-YLRM: + leaves it for the composer of another discussion, and the typed text stays", async () => {
    const s = spies();
    render(<Hosted discussions={[discussionSaying("disc-1", ["opening"])]} spies={s} />);
    const field = await screen.findByRole("textbox", { name: "Discuss this draft" });
    await userEvent.type(field, "draft text");

    await userEvent.click(screen.getByTestId("discussion-new"));

    expect(s.cancelHunk).toHaveBeenCalledTimes(1);
    expect(s.cancelFragment).toHaveBeenCalledTimes(1);
    expect(screen.getByTestId("draft-open-discussion")).toHaveTextContent(
      "This begins another discussion about this draft.",
    );
    expect(screen.queryByTestId("discussion-subject")).toBeNull();
    expect(screen.getByRole("textbox", { name: "Discuss this draft" })).toHaveValue("draft text");
  });
});

describe("DDS-FR-MZQJ: the discussion's menu in the column header", () => {
  it("DDS-FR-MZQJ, CMT-FR-15: the header holds the one entry, and no message holds a menu", async () => {
    const onSetResolved = vi.fn(async () => {});
    render(columnElement({ onSetResolved }));
    const header = screen.getByTestId("discussion-header-actions");
    const menu = await within(header).findByRole("button", { name: "Thread actions for disc-1" });
    expect(screen.getAllByRole("button", { name: "Thread actions for disc-1" })).toHaveLength(1);

    await userEvent.click(menu);
    await userEvent.click(screen.getByRole("menuitem", { name: "Mark resolved" }));
    expect(onSetResolved).toHaveBeenCalledWith("disc-1", true);
  });

  it("DDS-FR-MZQJ, CMT-FR-16: a resolved discussion says so in the header, offers Reopen, and has no composer", async () => {
    const settled = { ...discussionSaying("disc-1", ["opening"]), resolved: true };
    render(columnElement({ discussions: [settled], selectedThreadId: "disc-1" }));

    expect(await screen.findByTestId("discussion-resolved-state")).toHaveTextContent("resolved");
    expect(screen.queryByRole("textbox", { name: "Reply to thread disc-1" })).toBeNull();
    const header = screen.getByTestId("discussion-header-actions");
    await userEvent.click(
      await within(header).findByRole("button", { name: "Thread actions for disc-1" }),
    );
    expect(screen.getAllByRole("menuitem").map((i) => i.textContent)).toEqual([
      "Reopen discussion",
    ]);
  });
});
