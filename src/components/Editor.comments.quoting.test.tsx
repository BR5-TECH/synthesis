import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  createEvent,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";

import { Editor } from "./Editor";
import { EditSessionStore } from "../state/editSessions";
import { GITHUB_TOKEN_ERRORS } from "../types";
import {
  BODY,
  human,
  makeThread,
  openRail,
  selectWithin,
  wireBackend,
  fragment,
} from "../test/editorCommentsFixtures";
import type { Backend } from "../test/editorCommentsFixtures";

/**
 * CMT-comments.md, driven through the real Editor.
 *
 * The rail's arithmetic — re-anchoring, tracking an edit, card stacking — lives
 * in `../state/commentAnchors` and is covered by its own unit tests. What is
 * exercised here is the wiring: which command each affordance invokes, what the
 * rail renders in each of its states, and the claims the Editor spec makes about
 * hosting it (EDT-FR-61).
 */
const invokeMock = vi.fn();
const unlistenMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

/**
 * The rail follows `"discussion changed"` (CMT-FR-04), so the mock keeps the
 * handlers rather than discarding them — a test emits on a channel exactly as
 * the backend would.
 */
const listeners = new Map<string, Set<(e: { payload: unknown }) => void>>();
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(
    async (event: string, cb: (e: { payload: unknown }) => void) => {
      const set = listeners.get(event) ?? new Set();
      set.add(cb);
      listeners.set(event, set);
      return () => {
        set.delete(cb);
        unlistenMock();
      };
    },
  ),
}));

beforeEach(() => {
  invokeMock.mockReset();
  unlistenMock.mockReset();
});

afterEach(() => {
  cleanup();
});

async function mount(over: Partial<Backend> = {}) {
  const sessions = new EditSessionStore();
  const backend: Backend = {
    load: { body: BODY, checksum: "ck1" },
    threads: [],
    calls: [],
    ...over,
  };
  // The anchor's `end` is derived so a hand-written fixture cannot drift.
  backend.threads = backend.threads.map((t) => ({
    ...t,
    fragmentTarget:
      t.fragmentTarget === null
        ? null
        : {
            ...t.fragmentTarget,
            end: t.fragmentTarget.start + t.fragmentTarget.quote.length,
          },
  }));
  wireBackend(invokeMock, backend);
  render(
    <Editor
      artifactId="a.md"
      artifactName="a.md"
      artifactType="skill"
      sessions={sessions}
    />,
  );
  await screen.findByLabelText("artifact body");
  return { sessions, backend };
}

/**
 * Selecting text in a jsdom contenteditable does not give ProseMirror a real
 * selection, so these drive the affordance through the same DOM selection the
 * component reads (`window.getSelection().toString()`), which is the part under
 * test — the mapping from a selection to a source anchor and the command it
 * produces.
 */
function selectInBody(text: string) {
  // Deliberately NOT inside `.editor__prose`: mutating ProseMirror's own DOM
  // makes its observer read a DOM change and call `coordsAtPos`, which jsdom
  // cannot serve. `captureSelection` reads `window.getSelection().toString()`
  // and takes its position hint from the editor's own selection state, so a
  // node parked elsewhere in the document exercises exactly the same path.
  const host = document.createElement("div");
  host.textContent = text;
  document.body.appendChild(host);
  selectionHosts.push(host);
  const range = document.createRange();
  range.selectNodeContents(host);
  const sel = window.getSelection()!;
  sel.removeAllRanges();
  sel.addRange(range);
  fireEvent.mouseUp(screen.getByLabelText("artifact body"));
}

const selectionHosts: HTMLElement[] = [];
afterEach(() => {
  window.getSelection()?.removeAllRanges();
  while (selectionHosts.length) selectionHosts.pop()!.remove();
});

describe("CMT-FR-12: quoting a message inside a thread", () => {
  it("does not let its own press collapse the selection", async () => {
    // The whole action reads the live selection. A button that takes focus
    // normally clears that selection before the click lands, which is why
    // activating Quote appeared to do nothing at all.
    await mount({ threads: [makeThread()] });
    await openRail();

    const quote = screen.getByRole("button", { name: "Quote comment 1 by raver119" });
    const press = createEvent.mouseDown(quote);
    fireEvent(quote, press);
    expect(press.defaultPrevented).toBe(true);
  });

  it("seeds the composer with the selected excerpt of that comment", async () => {
    // CMT-FR-12, CMT-FR-13.
    const { backend } = await mount({ threads: [makeThread()] });
    const rail = await openRail();

    selectWithin(screen.getByTestId("comment-thread-t1"), "session?");
    fireEvent.click(screen.getByRole("button", { name: "Quote comment 1 by raver119" }));

    const pending = rail.querySelector(".comment__quoted--pending");
    expect(pending?.textContent).toContain("session?");
    expect(pending?.textContent).toContain("raver119");

    fireEvent.change(screen.getByLabelText("Reply to thread t1"), {
      target: { value: "yes" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Post" }));
    await waitFor(() =>
      expect(backend.calls.some((c) => c.cmd === "add_comment")).toBe(true),
    );
    expect(backend.calls.find((c) => c.cmd === "add_comment")!.args).toMatchObject({
      quotes: [{ commentId: "c1", excerpt: "session?" }],
    });
  });

  it("quotes the whole message when nothing inside it is selected", async () => {
    // Activating an action must do the thing it names. Quoting a message with
    // no part of it picked out means the message.
    await mount({ threads: [makeThread()] });
    const rail = await openRail();

    window.getSelection()?.removeAllRanges();
    fireEvent.click(screen.getByRole("button", { name: "Quote comment 1 by raver119" }));

    expect(rail.querySelector(".comment__quoted--pending")?.textContent).toContain(
      "Which session?",
    );
  });

  it("ignores a selection made outside the comment being quoted", async () => {
    // Text highlighted in the artifact is not what the reader is quoting.
    await mount({ threads: [makeThread()] });
    const rail = await openRail();

    selectInBody("the first session");
    fireEvent.click(screen.getByRole("button", { name: "Quote comment 1 by raver119" }));

    const pending = rail.querySelector(".comment__quoted--pending");
    expect(pending?.textContent).not.toContain("the first session");
    expect(pending?.textContent).toContain("Which session?");
  });

  it("drops a quote the author removes before posting", async () => {
    const { backend } = await mount({ threads: [makeThread()] });
    const rail = await openRail();

    fireEvent.click(screen.getByRole("button", { name: "Quote comment 1 by raver119" }));
    expect(rail.querySelector(".comment__quoted--pending")).not.toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Remove quote" }));
    expect(rail.querySelector(".comment__quoted--pending")).toBeNull();

    fireEvent.change(screen.getByLabelText("Reply to thread t1"), {
      target: { value: "no quote" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Post" }));
    await waitFor(() =>
      expect(backend.calls.some((c) => c.cmd === "add_comment")).toBe(true),
    );
    expect(backend.calls.find((c) => c.cmd === "add_comment")!.args).toMatchObject({
      quotes: [],
    });
  });
});

describe("CMT-FR-12: quoting across a thread's several messages", () => {
  function threadOfThree() {
    return makeThread({
      comments: [
        {
          id: "c1",
          author: human("raver119"),
          body: "first message here",
          quotes: [],
          attachments: [],
          createdAt: "2026-01-01T00:00:00Z",
        },
        {
          id: "c2",
          author: human("raver119"),
          body: "second message here",
          quotes: [],
          attachments: [],
          createdAt: "2026-01-02T00:00:00Z",
        },
        {
          id: "c3",
          author: human("octocat"),
          body: "third message here",
          quotes: [],
          attachments: [],
          createdAt: "2026-01-03T00:00:00Z",
        },
      ],
    });
  }

  it("names each message's Quote distinctly even when one author wrote two", async () => {
    // CMT-FR-35: a control reachable by name is no use if two of them answer to
    // the same one — a keyboard user would have no way to say which they meant.
    await mount({ threads: [threadOfThree()] });
    await openRail();

    expect(screen.getByRole("button", { name: "Quote comment 1 by raver119" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Quote comment 2 by raver119" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Quote comment 3 by octocat" })).toBeInTheDocument();
  });

  it("quotes the message whose action was activated, not the one selected in", async () => {
    // A selection in a sibling message is not what this Quote is for.
    await mount({ threads: [threadOfThree()] });
    const rail = await openRail();

    const comments = screen.getByTestId("comment-thread-t1").querySelectorAll(".comment");
    selectWithin(comments[1], "second message");
    fireEvent.click(screen.getByRole("button", { name: "Quote comment 1 by raver119" }));

    const pending = rail.querySelector(".comment__quoted--pending");
    expect(pending?.textContent).toContain("first message here");
    expect(pending?.textContent).not.toContain("second message");
  });

  it("carries several quotes and drops the right one when one is removed", async () => {
    // CMT-FR-12 names this in so many words, and the removal is by index over a
    // list rendered by index — exactly the shape that goes wrong silently.
    const { backend } = await mount({ threads: [threadOfThree()] });
    await openRail();

    fireEvent.click(screen.getByRole("button", { name: "Quote comment 1 by raver119" }));
    fireEvent.click(screen.getByRole("button", { name: "Quote comment 2 by raver119" }));
    fireEvent.click(screen.getByRole("button", { name: "Quote comment 3 by octocat" }));
    expect(document.querySelectorAll(".comment__quoted--pending")).toHaveLength(3);

    // Remove the middle one.
    fireEvent.click(screen.getAllByRole("button", { name: "Remove quote" })[1]);

    fireEvent.change(screen.getByLabelText("Reply to thread t1"), {
      target: { value: "answering both" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Post" }));
    await waitFor(() =>
      expect(backend.calls.some((c) => c.cmd === "add_comment")).toBe(true),
    );
    expect(backend.calls.find((c) => c.cmd === "add_comment")!.args).toMatchObject({
      quotes: [
        { commentId: "c1", excerpt: "first message here" },
        { commentId: "c3", excerpt: "third message here" },
      ],
    });
  });

  it("keeps pending quotes when a post is refused and clears them on cancel", async () => {
    // CMT-FR-34's "leaving the composer's content intact" is about everything
    // the author assembled, not the textarea alone.
    await mount({ threads: [threadOfThree()], addCommentError: "discussion_locked" });
    await openRail();

    fireEvent.click(screen.getByRole("button", { name: "Quote comment 1 by raver119" }));
    fireEvent.change(screen.getByLabelText("Reply to thread t1"), {
      target: { value: "retry me" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Post" }));
    await screen.findByRole("alert");
    expect(document.querySelectorAll(".comment__quoted--pending")).toHaveLength(1);

    // CMT-FR-11: cancelling discards what the composer held, quotes included.
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(document.querySelectorAll(".comment__quoted--pending")).toHaveLength(0);
  });

  it("offers no Quote at all on a locked thread or while no identity resolves", async () => {
    // CMT-FR-12 is a property of an unlocked card; CMT-FR-24 disables the whole
    // conversation while nobody can be named as its author.
    await mount({ threads: [{ ...threadOfThree(), locked: true }] });
    await openRail();
    expect(screen.queryAllByRole("button", { name: /^Quote comment/ })).toHaveLength(0);

    cleanup();
    await mount({
      threads: [threadOfThree()],
      identityError: GITHUB_TOKEN_ERRORS.tokenMissing,
    });
    await openRail();
    expect(screen.queryAllByRole("button", { name: /^Quote comment/ })).toHaveLength(0);
  });
});
