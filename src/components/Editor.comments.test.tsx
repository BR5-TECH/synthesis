import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";

import { Editor } from "./Editor";
import { resetLogBufferForTest } from "../logging";
import { resetProjectIdentity } from "../state/projectIdentity";
import { EditSessionStore } from "../state/editSessions";
import { GITHUB_TOKEN_ERRORS } from "../types";
import {
  BODY,
  LOCAL_HUMAN,
  commentsToggle,
  human,
  makeThread,
  openRail,
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
  // The project login and the log buffer are module-level, so a login or a
  // flush timer from one test must not reach the next.
  resetProjectIdentity();
  resetLogBufferForTest();
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

describe("CMT-FR-04 / CMT-FR-29: loading and the action-cluster control", () => {
  it("loads the artifact's threads once and reads the unresolved count", async () => {
    const { backend } = await mount({ threads: [makeThread()] });
    await waitFor(() =>
      expect(
        backend.calls.filter((c) => c.cmd === "list_discussions"),
      ).toHaveLength(1),
    );
    expect(backend.calls[backend.calls.length - 1]).toBeDefined();
    await waitFor(() =>
      expect(commentsToggle()).toHaveAttribute(
        "title",
        "1 unresolved thread",
      ),
    );
  });

  it("counts only unresolved threads", async () => {
    await mount({
      threads: [
        makeThread({ id: "t1" }),
        makeThread({ id: "t2", resolved: true }),
        makeThread({ id: "t3" }),
      ],
    });
    await waitFor(() =>
      expect(commentsToggle()).toHaveAttribute("title", "2 unresolved threads"),
    );
  });

  it("keeps the control and its count in raw-text mode while hiding the rail", async () => {
    // CMT-FR-02 / CMT-FR-29 / EDT-FR-61: a switch to raw text never hides the
    // fact that threads exist.
    await mount({ threads: [makeThread()] });
    await openRail();
    expect(screen.getByRole("complementary", { name: "Comments" })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Edit as Markdown source" }));
    await screen.findByLabelText("Markdown source");
    expect(screen.queryByRole("complementary", { name: "Comments" })).toBeNull();
    expect(commentsToggle()).toHaveAttribute("title", "1 unresolved thread");

    // And toggling back renders the same thread against the same anchor.
    fireEvent.click(screen.getByRole("button", { name: "Edit as rich text" }));
    await screen.findByRole("complementary", { name: "Comments" });
    expect(screen.getByTestId("comment-thread-t1")).toBeInTheDocument();
  });
});

describe("CMT-FR-30: the rail's open state", () => {
  it("opens by default for an artifact carrying an unresolved thread", async () => {
    await mount({ threads: [makeThread()] });
    await waitFor(() =>
      expect(screen.getByRole("complementary", { name: "Comments" })).toBeInTheDocument(),
    );
  });

  it("stays closed for an artifact carrying none", async () => {
    await mount({ threads: [] });
    await waitFor(() => expect(commentsToggle()).toBeInTheDocument());
    expect(screen.queryByRole("complementary", { name: "Comments" })).toBeNull();
  });

  it("keeps the author's own choice in the artifact's retained edit state", async () => {
    // CMT-FR-30, EDT-FR-28: closing the rail survives the tab closing and reopening.
    const { sessions } = await mount({ threads: [makeThread()] });
    await screen.findByRole("complementary", { name: "Comments" });

    fireEvent.click(commentsToggle());
    expect(screen.queryByRole("complementary", { name: "Comments" })).toBeNull();
    expect(sessions.get("a.md")?.railOpen).toBe(false);

    // The record survives the close *because* the author made that choice — an
    // artifact merely opened and read is still dropped (EDT-FR-28).
    cleanup();
    act(() => sessions.closeTab("a.md"));
    expect(sessions.get("a.md")?.railOpen).toBe(false);
  });

  it("keeps the resolved disclosure expanded across a tab close and reopen", async () => {
    // CMT-FR-17 / CMT-FR-30: the disclosure's state belongs to the artifact.
    // The Editor is unmounted and remounted when a tab closes and reopens, so
    // component state would collapse it every time.
    const { sessions } = await mount({ threads: [makeThread({ resolved: true })] });
    fireEvent.click(commentsToggle());
    await screen.findByRole("complementary", { name: "Comments" });

    fireEvent.click(screen.getByRole("button", { name: /1 resolved thread/ }));
    await screen.findByTestId("comment-thread-t1");
    expect(sessions.get("a.md")?.resolvedOpen).toBe(true);

    // A mode toggle unmounts the rail but not the Editor…
    fireEvent.click(screen.getByRole("button", { name: "Edit as Markdown source" }));
    await screen.findByLabelText("Markdown source");
    fireEvent.click(screen.getByRole("button", { name: "Edit as rich text" }));
    await screen.findByTestId("comment-thread-t1");

    // …and closing the tab unmounts the Editor, which is the case component
    // state could not survive.
    cleanup();
    act(() => sessions.closeTab("a.md"));
    expect(sessions.get("a.md")?.resolvedOpen).toBe(
      true,
      );
  });

  it("does not retain a record for an artifact whose rail was only defaulted", async () => {
    const { sessions } = await mount({ threads: [makeThread()] });
    await screen.findByRole("complementary", { name: "Comments" });
    expect(sessions.get("a.md")?.railOpen).toBeNull();
    // The tab closing means the Editor is gone: a mounted one re-creates the
    // record through `ensure` on its very next render, so unmount first.
    cleanup();
    act(() => sessions.closeTab("a.md"));
    // Reopening is a plain first load, not a resumed session (EDT-FR-28).
    expect(sessions.get("a.md")).toBeUndefined();
  });
});

describe("CMT-FR-08 / CMT-FR-09 / CMT-FR-10: what a card renders", () => {
  it("renders a comment's Markdown as rich text and names its author", async () => {
    await mount({ threads: [makeThread()] });
    const rail = await openRail();
    expect(rail).toHaveTextContent("raver119");
    // `**which**` renders as emphasis rather than as literal asterisks.
    expect(rail.querySelector("strong")?.textContent).toBe("which");
    expect(rail.textContent).not.toContain("**which**");
  });

  it("typesets a card's body at the rail's scale, not the page's", async () => {
    // CMT-FR-08 layout note: a card is a narrow box in the page's margin, and
    // `.doc` alone is the Editor page's own type scale — a body set at it
    // renders half again the size of the card around it.
    //
    // This asserts only that the card asks for the panel treatment. That the
    // treatment still means anything is a fact about the stylesheet, which
    // jsdom never applies, and lives in src/test/style-invariants.test.ts; the
    // two together are what keep the defect fixed, and either alone lets it back
    // in from one side.
    await mount({ threads: [makeThread()] });
    const rail = await openRail();
    const body = rail.querySelector(".comment__body");
    expect(body).toHaveClass("doc");
    expect(body).toHaveClass("doc--ui");
  });

  it("distinguishes an agent's comment from a human's", async () => {
    // CMT-FR-10: the whole reason the participant is a tagged union.
    await mount({
      threads: [
        makeThread({
          comments: [
            {
              id: "c1",
              author: human("raver119"),
              body: "can you tighten this?",
              quotes: [],
              attachments: [],
              createdAt: "2026-01-01T00:00:00Z",
            },
            {
              id: "c2",
              author: { kind: "agent", agentId: "claude_code", handle: "claude" },
              body: "Shortened to two lines.",
              quotes: [],
              attachments: [],
              createdAt: "2026-01-02T00:00:00Z",
            },
          ],
        }),
      ],
    });
    const rail = await openRail();
    const authors = Array.from(
      rail.querySelectorAll<HTMLElement>(".comment__author"),
    );
    expect(authors.map((a) => a.dataset.agent)).toEqual(["false", "true"]);
    expect(authors[1].textContent).toContain("claude");
  });

  it("renders a quoted message as an attributed block", async () => {
    // CMT-FR-13.
    await mount({
      threads: [
        makeThread({
          comments: [
            {
              id: "c1",
              author: human("raver119"),
              body: "there are three",
              quotes: [],
              attachments: [],
              createdAt: "2026-01-01T00:00:00Z",
            },
            {
              id: "c2",
              author: human("octocat"),
              body: "Fixed.",
              quotes: [{ commentId: "c1", excerpt: "there are three" }],
              attachments: [],
              createdAt: "2026-01-02T00:00:00Z",
            },
          ],
        }),
      ],
    });
    const rail = await openRail();
    const quoted = rail.querySelector(".comment__quoted");
    expect(quoted?.textContent).toContain("raver119");
    expect(quoted?.textContent).toContain("there are three");
  });
});

describe("CMT-FR-11 / CMT-FR-14 / CMT-FR-15: replying, immutability, lock", () => {
  it("posts a reply through add_comment and appends it to the card", async () => {
    const { backend } = await mount({ threads: [makeThread()] });
    await openRail();

    const composer = screen.getByLabelText("Reply to thread t1");
    fireEvent.change(composer, { target: { value: "agreed" } });
    fireEvent.click(screen.getByRole("button", { name: "Post" }));

    await waitFor(() =>
      expect(backend.calls.some((c) => c.cmd === "add_comment")).toBe(true),
    );
    const call = backend.calls.find((c) => c.cmd === "add_comment")!;
    expect(call.args).toMatchObject({
      artifactId: "a.md",
      discussionId: "t1",
      body: "agreed",
      quotes: [],
    });
    await screen.findByText("agreed");
  });

  it("offers no way to edit or delete a comment or a thread anywhere", async () => {
    // CMT-FR-14: comments are immutable once posted.
    await mount({ threads: [makeThread()] });
    const rail = await openRail();
    fireEvent.click(screen.getByRole("button", { name: /Thread actions/ }));

    const labels = Array.from(rail.querySelectorAll("button")).map(
      (b) => `${b.textContent ?? ""} ${b.getAttribute("aria-label") ?? ""}`.toLowerCase(),
    );
    for (const label of labels) {
      expect(label).not.toContain("edit");
      expect(label).not.toContain("delete");
    }
    // And the menu holds exactly the lock and resolve entries (CMT-FR-15).
    const menuItems = Array.from(
      rail.querySelectorAll<HTMLElement>('[role="menuitem"]'),
    ).map((b) => b.textContent);
    expect(menuItems).toEqual(["Lock thread", "Mark resolved"]);
  });

  it("locks a thread, hides its composer, and keeps its comments readable", async () => {
    const { backend } = await mount({ threads: [makeThread()] });
    await openRail();
    fireEvent.click(screen.getByRole("button", { name: /Thread actions/ }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Lock thread" }));

    await waitFor(() =>
      expect(
        backend.calls.find((c) => c.cmd === "set_discussion_lock")?.args,
      ).toMatchObject({ discussionId: "t1", locked: true }),
    );
    await waitFor(() =>
      expect(screen.queryByLabelText("Reply to thread t1")).toBeNull(),
    );
    expect(screen.getByTestId("comment-thread-t1")).toHaveTextContent("Locked");
    // The comment itself is still there to read.
    expect(screen.getByTestId("comment-thread-t1")).toHaveTextContent("Which session?");

    // And unlocking brings the composer back (CMT-FR-15).
    fireEvent.click(screen.getByRole("button", { name: /Thread actions/ }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Unlock thread" }));
    await screen.findByLabelText("Reply to thread t1");
  });
});

describe("CMT-FR-16 / CMT-FR-17: resolving", () => {
  it("moves a resolved thread out of the aligned column into the disclosure", async () => {
    const { backend } = await mount({
      threads: [makeThread({ id: "t1" }), makeThread({ id: "t2" })],
    });
    await openRail();
    expect(screen.getByTestId("comment-thread-t1")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Thread actions for t1" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Mark resolved" }));

    await waitFor(() =>
      expect(
        backend.calls.find((c) => c.cmd === "set_discussion_resolution")?.args,
      ).toMatchObject({ discussionId: "t1", resolved: true }),
    );
    // Out of the column and behind a collapsed disclosure.
    await waitFor(() =>
      expect(screen.queryByTestId("comment-thread-t1")).toBeNull(),
    );
    const disclosure = screen.getByRole("button", { name: /1 resolved thread/ });
    expect(disclosure).toHaveAttribute("aria-expanded", "false");

    // Expanding it renders the card in full, and reopening returns it.
    fireEvent.click(disclosure);
    await screen.findByTestId("comment-thread-t1");
    fireEvent.click(screen.getByRole("button", { name: "Thread actions for t1" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Reopen thread" }));
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: /resolved thread/ })).toBeNull(),
    );
  });

  it("lock and resolve are independent", async () => {
    // CMT-FR-16 / CMS-FR-18: a locked thread can be resolved and reopened, and
    // stays locked throughout.
    await mount({ threads: [makeThread({ locked: true })] });
    await openRail();
    fireEvent.click(screen.getByRole("button", { name: /Thread actions/ }));
    // A locked thread's menu still offers the resolve entry.
    expect(screen.getByRole("menuitem", { name: "Mark resolved" })).toBeEnabled();
    expect(screen.getByRole("menuitem", { name: "Unlock thread" })).toBeEnabled();
  });
});

describe("CMT-FR-19: orphaned threads", () => {
  it("renders a thread whose passage is gone in the orphaned section, still repliable", async () => {
    // CMT-FR-18, CMT-FR-19.
    await mount({
      threads: [
        makeThread({
          id: "t-orphan",
          fragmentTarget: fragment({ start: 5, end: 5, quote: "a passage that is not in the file" }),
        }),
      ],
    });
    const rail = await openRail();
    expect(rail).toHaveTextContent("Orphaned");
    const card = screen.getByTestId("comment-thread-t-orphan");
    // It shows the quote it was attached to, in place of an alignment.
    expect(card).toHaveTextContent("a passage that is not in the file");
    expect(card.dataset.positioned).toBe("false");
    // Fully readable, still carries its menu, and still accepts a reply.
    expect(screen.getByRole("button", { name: /Thread actions/ })).toBeInTheDocument();
    expect(screen.getByLabelText("Reply to thread t-orphan")).toBeInTheDocument();
  });

  it("counts an orphaned thread as unresolved", async () => {
    // CMT-FR-29: it is the thread most at risk of being forgotten.
    await mount({
      threads: [
        makeThread({ id: "t-orphan", fragmentTarget: fragment({ start: 0, end: 0, quote: "nowhere at all" }) }),
      ],
    });
    await waitFor(() =>
      expect(commentsToggle()).toHaveAttribute("title", "1 unresolved thread"),
    );
  });
});

describe("CMT-FR-24 / CMT-FR-25 / CMT-FR-26: identity gating", () => {
  it("CMT-FR-24, CMT-FR-26: enables commenting as Me when no token is stored, with no missing-token reason", async () => {
    await mount({ threads: [makeThread()], identity: LOCAL_HUMAN });
    const rail = await openRail();
    await waitFor(() =>
      expect(screen.getByLabelText("Reply to thread t1")).toBeEnabled(),
    );
    expect(rail).not.toHaveTextContent(/GitHub account/i);
    expect(rail).not.toHaveTextContent("Global settings → GitHub");
  });

  it("gives a token that needs picking a different reason from one that needs adding", async () => {
    // The two call for different actions, so they must not collapse.
    await mount({
      threads: [makeThread()],
      identityError: GITHUB_TOKEN_ERRORS.selectionRequired,
    });
    const rail = await openRail();
    expect(rail).toHaveTextContent(/Choose which GitHub token/i);
    expect(rail).not.toHaveTextContent("Global settings → GitHub");
  });

  it("distinguishes an unreachable GitHub from a missing token", async () => {
    await mount({
      threads: [makeThread()],
      identityError: GITHUB_TOKEN_ERRORS.githubUnreachable,
    });
    const rail = await openRail();
    expect(rail).toHaveTextContent(/Could not reach GitHub/i);
  });

  it("enables commenting when an identity resolves", async () => {
    await mount({ threads: [makeThread()] });
    await openRail();
    await waitFor(() =>
      expect(screen.getByLabelText("Reply to thread t1")).toBeEnabled(),
    );
  });
});

describe("CMT-FR-ZCAE: the local participant's label", () => {
  const authorNames = () =>
    Array.from(document.querySelectorAll(".comment__author")).map(
      (el) => el.textContent,
    );
  const tokensChanged = () =>
    act(() => {
      for (const cb of listeners.get("github-tokens-changed") ?? []) {
        cb({ payload: null });
      }
    });
  const localThread = () =>
    makeThread({
      comments: [
        {
          id: "c1",
          author: LOCAL_HUMAN,
          body: "written without a token",
          quotes: [],
          attachments: [],
          createdAt: "2026-01-01T00:00:00Z",
        },
        {
          id: "c2",
          author: human("raver119"),
          body: "a saved github comment",
          quotes: [],
          attachments: [],
          createdAt: "2026-01-01T00:00:01Z",
        },
      ],
    });

  it("CMT-FR-ZCAE, CMT-FR-10: reads Me while no identity resolves and the project login once one does, leaving other authors as saved", async () => {
    const { backend } = await mount({ threads: [localThread()], identity: LOCAL_HUMAN });
    await openRail();
    await waitFor(() => expect(authorNames()).toEqual(["Me", "raver119"]));

    backend.identity = human("octocat");
    tokensChanged();
    await waitFor(() => expect(authorNames()).toEqual(["octocat", "raver119"]));
    // The stored snapshot is untouched: the comment is not read again.
    expect(backend.threads[0].comments[0].author).toEqual(LOCAL_HUMAN);
    expect(backend.calls.filter((c) => c.cmd === "list_discussions")).toHaveLength(1);
  });

  it("CMT-FR-ZCAE: reads Me again while a token binding is required", async () => {
    const { backend } = await mount({ threads: [localThread()], identity: human("octocat") });
    await openRail();
    await waitFor(() => expect(authorNames()[0]).toBe("octocat"));

    backend.identity = undefined;
    backend.identityError = GITHUB_TOKEN_ERRORS.selectionRequired;
    tokensChanged();
    await waitFor(() => expect(authorNames()[0]).toBe("Me"));
    expect(screen.getByLabelText("Reply to thread t1")).toBeDisabled();
  });

  it("CMT-FR-24, CMT-FR-ZCAE: posts nothing the author could mistake for another account", async () => {
    const { backend } = await mount({ threads: [localThread()], identity: LOCAL_HUMAN });
    await openRail();
    await waitFor(() =>
      expect(screen.getByLabelText("Reply to thread t1")).toBeEnabled(),
    );
    fireEvent.change(screen.getByLabelText("Reply to thread t1"), {
      target: { value: "no token needed" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Post" }));
    await waitFor(() =>
      expect(backend.calls.some((c) => c.cmd === "add_comment")).toBe(true),
    );
    const call = backend.calls.find((c) => c.cmd === "add_comment")!;
    expect(JSON.stringify(call.args)).not.toMatch(/login|participant|author/i);
  });
});

describe("CMT-FR-34: a failed operation", () => {
  it("re-renders the card as locked and keeps the composer's content", async () => {
    // CMT-FR-34, CMS-FR-17: someone else locked the thread since the rail last read it.
    // The fixture models exactly that — the refusal AND the state change behind
    // it — so the "re-renders as locked" clause is actually testable.
    const { backend } = await mount({
      threads: [makeThread()],
      addCommentError: "discussion_locked",
    });
    await openRail();
    backend.threads = backend.threads.map((t) => ({ ...t, locked: true }));

    const composer = screen.getByLabelText("Reply to thread t1") as HTMLTextAreaElement;
    fireEvent.change(composer, { target: { value: "my careful reply" } });
    fireEvent.click(screen.getByRole("button", { name: "Post" }));

    const alert = await screen.findByRole("alert");
    // Prose, not the wire slug: the author is told what happened and that it is
    // someone else's doing, rather than shown an implementation detail.
    expect(alert.textContent).toMatch(/locked/i);
    expect(alert.textContent).not.toBe("discussion_locked");

    // The card re-renders as locked, so the composer is gone rather than
    // inviting a second attempt at something that cannot succeed.
    await waitFor(() =>
      expect(screen.queryByLabelText("Reply to thread t1")).toBeNull(),
    );
    expect(screen.getByTestId("comment-thread-t1")).toHaveTextContent("Locked");
  });

  it("keeps the typed body when the refusal leaves the thread writable", async () => {
    // A transient failure must not cost the author what they wrote.
    const { backend } = await mount({
      threads: [makeThread()],
      addCommentError: "discussion_not_found",
    });
    await openRail();
    const composer = screen.getByLabelText("Reply to thread t1") as HTMLTextAreaElement;
    fireEvent.change(composer, { target: { value: "my careful reply" } });
    fireEvent.click(screen.getByRole("button", { name: "Post" }));

    await screen.findByRole("alert");
    expect(
      (screen.getByLabelText("Reply to thread t1") as HTMLTextAreaElement).value,
    ).toBe("my careful reply");
    // And the rail re-read, so a thread someone else changed is refreshed.
    await waitFor(() =>
      expect(
        backend.calls.filter((c) => c.cmd === "list_discussions").length,
      ).toBeGreaterThan(1),
    );
  });
});

describe("CMT-FR-31 / EDT-FR-61: the rail is not part of the editing surface", () => {
  it("contributes no match to the find panel", async () => {
    // CMT-FR-31: a word that appears only in a comment must not be findable.
    const { sessions } = await mount({
      threads: [
        makeThread({
          comments: [
            {
              id: "c1",
              author: human("raver119"),
              body: "zzzunique",
              quotes: [],
              attachments: [],
              createdAt: "2026-01-01T00:00:00Z",
            },
          ],
        }),
      ],
    });
    await openRail();
    await screen.findByText("zzzunique");

    act(() => {
      sessions.setFind("a.md", { form: "find", query: "zzzunique" });
    });
    // The panel's zero state, which is what "nothing matched" looks like.
    await waitFor(() =>
      expect(screen.getByTestId("find-count").textContent).toBe("No results"),
    );
  });

  it("adds no undo step when a thread is resolved", async () => {
    const { sessions } = await mount({ threads: [makeThread()] });
    await openRail();
    const before = sessions.get("a.md")?.history;
    const depthBefore = JSON.stringify(before);

    fireEvent.click(screen.getByRole("button", { name: /Thread actions/ }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Mark resolved" }));
    await waitFor(() =>
      expect(screen.queryByTestId("comment-thread-t1")).toBeNull(),
    );

    expect(JSON.stringify(sessions.get("a.md")?.history)).toBe(depthBefore);
    expect(sessions.get("a.md")?.dirty).toBe(false);
  });
});

describe("CMT-FR-32: the rail's own transient surfaces", () => {
  it("keeps at most one overflow menu open and dismisses on Escape", async () => {
    await mount({ threads: [makeThread({ id: "t1" }), makeThread({ id: "t2" })] });
    await openRail();
    const buttons = screen.getAllByRole("button", { name: /Thread actions/ });

    fireEvent.click(buttons[0]);
    expect(screen.getAllByRole("menuitem")).toHaveLength(2);

    fireEvent.click(buttons[1]);
    // Opening the second dismissed the first: still one menu, not two.
    expect(screen.getAllByRole("menuitem")).toHaveLength(2);
    expect(buttons[0]).toHaveAttribute("aria-expanded", "false");
    expect(buttons[1]).toHaveAttribute("aria-expanded", "true");

    fireEvent.keyDown(document, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("menuitem")).toBeNull());
  });
});

describe("CMT-FR-25: the token picker", () => {
  it("offers a way to resolve a selection-required identity and re-resolves on confirm", async () => {
    // CMT-FR-25, GHA-FR-16. Stating the reason without offering the action would leave the
    // author with nothing to do about it.
    let identityError: string | undefined = GITHUB_TOKEN_ERRORS.selectionRequired;
    const sessions = new EditSessionStore();
    const backend: Backend = {
      load: { body: BODY, checksum: "ck1" },
      threads: [{ ...makeThread(), fragmentTarget: fragment({ start: 0, end: 5, quote: BODY.slice(0, 5) }) }],
      calls: [],
    };
    invokeMock.mockImplementation(async (cmd: string, args: unknown) => {
      backend.calls.push({ cmd, args });
      if (cmd === "load_artifact_contents_by_id") return backend.load;
      if (cmd === "list_discussions") return backend.threads;
      if (cmd === "resolve_comment_author_identity") {
        if (identityError) throw identityError;
        return human("raver119");
      }
      if (cmd === "list_github_tokens") {
        return [
          {
            id: "tok-1",
            label: "work",
            accountLogin: "raver119",
            scopes: ["repo"],
            maskedHint: "a3f9",
            addedAt: "2026-01-01T00:00:00Z",
            lastVerifiedAt: null,
            state: "valid",
          },
        ];
      }
      if (cmd === "set_project_github_token_binding") return { tokenId: "tok-1", resolution: "bound" };
      throw new Error(`unexpected invoke ${cmd}`);
    });
    render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
        sessions={sessions}
      />,
    );
    await screen.findByLabelText("artifact body");
    const rail = await openRail();

    expect(rail).toHaveTextContent(/Choose which GitHub token/i);
    expect(screen.getByLabelText("Reply to thread t1")).toBeDisabled();

    // The action opens the picker.
    fireEvent.click(screen.getByRole("button", { name: "Choose a token…" }));
    await screen.findByText(/work/);

    // Confirming re-resolves the identity, which is what enables commenting.
    identityError = undefined;
    const use = screen.getByRole("button", { name: /^Use$/ });
    fireEvent.click(use);
    await waitFor(() =>
      expect(screen.getByLabelText("Reply to thread t1")).toBeEnabled(),
    );
  });

  it("CMT-FR-25, CMT-FR-26: offers no picker and no settings route when nothing is stored", async () => {
    await mount({ threads: [makeThread()], identity: LOCAL_HUMAN });
    await openRail();
    expect(screen.queryByRole("button", { name: "Choose a token…" })).toBeNull();
    expect(
      within(screen.getByRole("complementary", { name: "Comments" })).queryByText(
        /Global settings → GitHub/,
      ),
    ).toBeNull();
  });
});
