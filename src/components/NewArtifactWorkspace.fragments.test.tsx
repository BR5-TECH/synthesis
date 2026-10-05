/**
 * The fragment discussions of a draft's prompt
 * (`../../specifications/ui/DDS-draft-discussion.md` DDS-FR-QMBC, DDS-FR-VHZN,
 * DDS-FR-NWRL; `../../specifications/ui/NAW-new-artifact.md` NAW-FR-14,
 * NAW-FR-32).
 *
 * A selection in the prompt offers Comment, the passage is marked while its
 * discussion is open, and the discussion itself is the shared surface in the
 * column. The column and the prompt reach each other through the focus
 * registry rather than through a second rendering.
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
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

import {
  PROMPT,
  discussionThread,
  makeStubs,
  renderWorkspace,
} from "../test/newArtifactFixtures";
import { hunkCandidateBuffers } from "../state/candidateBuffers";
import { resetDraftDiscussions } from "../state/draftDiscussion";
import { resetLayoutPreferencesCache } from "../state/layoutPreferences";
import { clearAllDiscussionSessions } from "../state/discussionSession";
import { resetDiscussionFocus } from "../state/discussionFocus";
import type { Discussion, Participant } from "../types";

const { stub, stubDiscussions } = makeStubs(invokeMock);

const callsTo = (cmd: string) =>
  invokeMock.mock.calls.filter((c) => c[0] === cmd).map((c) => c[1]);

const ME = { kind: "human", login: "raver119" } as Participant;

/** The stub's prompt is `body text`: `body` is [0, 4) and `text` is [5, 9). */
function fragmentOf(id: string, quote: string, start: number): Discussion {
  return discussionThread(
    id,
    [{ id: `${id}-c0`, author: ME, body: `about ${quote}` }],
    {
      fragmentTarget: {
        owner: { kind: "draft", draftId: "d1" },
        path: PROMPT,
        start,
        end: start + quote.length,
        quote,
      },
    },
  );
}

beforeEach(() => {
  invokeMock.mockReset();
  hunkCandidateBuffers.clear();
  clearAllDiscussionSessions();
  resetDiscussionFocus();
  resetDraftDiscussions();
  resetLayoutPreferencesCache();
  stub();
});

afterEach(() => {
  window.getSelection()?.removeAllRanges();
  cleanup();
});

/**
 * Selecting text in a jsdom contenteditable gives ProseMirror no real
 * selection, so the selection is parked in a node of its own and the surface
 * reads it through `window.getSelection()`, which is the part under test.
 */
function selectInPrompt(text: string) {
  const host = document.createElement("div");
  host.textContent = text;
  document.body.appendChild(host);
  const range = document.createRange();
  range.selectNodeContents(host);
  const sel = window.getSelection()!;
  sel.removeAllRanges();
  sel.addRange(range);
  fireEvent.mouseUp(screen.getByLabelText("artifact body"));
  return () => host.remove();
}

describe("a comment on a passage of the prompt (DDS-FR-QMBC, NAW-FR-32)", () => {
  it("DDS-FR-QMBC, NAW-FR-32, CMT-FR-05: Comment on a selection opens the column's composer for that passage, and posting opens a fragment discussion of the draft", async () => {
    stubDiscussions({});
    renderWorkspace();
    await screen.findByLabelText("artifact body");
    await screen.findByText("body text");

    const cleanupHost = selectInPrompt("body");
    const comment = await screen.findByRole("button", { name: "Comment on selection" });
    await userEvent.click(comment);

    // The column stands on the opening composer carrying the passage.
    const column = screen.getByTestId("draft-discussion-column");
    expect(within(column).getByTestId("discussion-opening-fragment")).toHaveTextContent(
      "body",
    );
    const field = within(column).getByRole("textbox", { name: "Discuss this draft" });
    await waitFor(() => expect(field).toHaveFocus());

    fireEvent.change(field, { target: { value: "tighten this" } });
    fireEvent.keyDown(field, { key: "Enter", ctrlKey: true });

    await waitFor(() => expect(callsTo("open_discussion")).toHaveLength(1));
    expect(callsTo("open_discussion")[0]).toMatchObject({
      target: { kind: "draft", draftId: "d1" },
      fragmentTarget: {
        owner: { kind: "draft", draftId: "d1" },
        path: PROMPT,
        start: 0,
        end: 4,
        quote: "body",
      },
      body: "tighten this",
    });
    // A fragment discussion of the draft is opened where the column reads it.
    await waitFor(() =>
      expect(screen.queryByTestId("discussion-opening-fragment")).toBeNull(),
    );
    expect(await screen.findByTestId("comment-thread-disc-1")).toBeInTheDocument();
    cleanupHost();
  });

  it("DDS-FR-QMBC, CMT-FR-07: discarding the composer of a passage opens no discussion", async () => {
    stubDiscussions({});
    renderWorkspace();
    await screen.findByText("body text");
    const cleanupHost = selectInPrompt("text");
    await userEvent.click(
      await screen.findByRole("button", { name: "Comment on selection" }),
    );
    await screen.findByTestId("discussion-opening-fragment");

    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));

    expect(screen.queryByTestId("discussion-opening-fragment")).toBeNull();
    expect(callsTo("open_discussion")).toHaveLength(0);
    cleanupHost();
  });
});

describe("the passages of open fragment discussions (DDS-FR-QMBC, DDS-FR-NWRL)", () => {
  it("DDS-FR-QMBC, DDS-FR-VHZN: marks the passage in the prompt, and lists whole-target and fragment discussions together in the column", async () => {
    stubDiscussions({
      existing: [
        discussionThread("disc-1", [{ id: "w1", author: ME, body: "about the draft" }]),
        fragmentOf("disc-2", "text", 5),
      ],
    });
    renderWorkspace();

    const column = await screen.findByTestId("draft-discussion-column");
    await within(column).findByTestId("comment-thread-disc-1");
    expect(within(column).getAllByRole("tab")).toHaveLength(2);

    // The mark carries the discussion's id, so it is reachable from the prompt.
    await waitFor(() =>
      expect(document.querySelector('[data-comment-thread="disc-2"]')).not.toBeNull(),
    );
    expect(document.querySelector('[data-comment-thread="disc-2"]')).toHaveTextContent(
      "text",
    );
  });

  it("DDS-FR-QMBC, CVP-FR-06: activating a mark focuses its discussion in the column", async () => {
    stubDiscussions({
      existing: [
        discussionThread("disc-1", [{ id: "w1", author: ME, body: "about the draft" }]),
        fragmentOf("disc-2", "text", 5),
      ],
    });
    renderWorkspace();
    const column = await screen.findByTestId("draft-discussion-column");
    await within(column).findByTestId("comment-thread-disc-1");
    const mark = await waitFor(() => {
      const found = document.querySelector<HTMLElement>('[data-comment-thread="disc-2"]');
      expect(found).not.toBeNull();
      return found!;
    });

    await act(async () => {
      fireEvent.click(mark);
    });

    // The column reads the fragment discussion now, and focus is inside it.
    const card = await within(column).findByTestId("comment-thread-disc-2");
    expect(card).toHaveAttribute("data-discussion-owner", "column");
    await waitFor(() => expect(card.contains(document.activeElement)).toBe(true));
    expect(within(card).getByTestId("discussion-fragment-quote")).toHaveTextContent("text");
    // One surface for the discussion, however it was reached.
    expect(screen.getAllByTestId("comment-thread-disc-2")).toHaveLength(1);
  });

  it("CMT-FR-19, DDS-FR-QMBC: a fragment the prompt does not hold is kept in the column as orphaned and marks nothing", async () => {
    stubDiscussions({
      existing: [fragmentOf("disc-2", "a sentence that is gone", 0)],
    });
    renderWorkspace();
    const card = await screen.findByTestId("comment-thread-disc-2");
    expect(card).toHaveAttribute("data-availability", "orphaned");
    expect(within(card).getByText("about a sentence that is gone")).toBeInTheDocument();
    expect(document.querySelector('[data-comment-thread="disc-2"]')).toBeNull();
  });
});
