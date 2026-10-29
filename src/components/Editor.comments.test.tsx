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
import { CONTEXT_ESTIMATE } from "../test/invokeMocks";
import { resetProjectIdentity } from "../state/projectIdentity";
import { EditSessionStore } from "../state/editSessions";
import { restoreDiscussionColumns } from "../state/discussionColumn";
import { resetLayoutPreferencesCache } from "../state/layoutPreferences";
import { GITHUB_TOKEN_ERRORS } from "../types";
import type { LayoutPreferences } from "../types";
import {
  BODY,
  LOCAL_HUMAN,
  discussionToggle,
  human,
  makeThread,
  openColumn,
  wireBackend,
  fragment,
} from "../test/editorCommentsFixtures";
import type { Backend } from "../test/editorCommentsFixtures";

/**
 * CMT-comments.md, driven through the real Editor.
 *
 * The anchor arithmetic — re-anchoring and tracking an edit — lives in
 * `../state/commentAnchors` and is covered by its own unit tests. What is
 * exercised here is the wiring: which command each affordance invokes, what the
 * discussion column renders in each of its states, and the claims the Editor
 * spec makes about hosting it (EDT-FR-61).
 */
const invokeMock = vi.fn();
const unlistenMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

/**
 * The column follows `"discussion changed"` (CMT-FR-04), so the mock keeps the
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
  resetLayoutPreferencesCache();
});

afterEach(() => {
  cleanup();
});

/** CMT-FR-29 / CMT-FR-53: the column's chooser tabs, one per unresolved discussion. */
const chooserTabs = (column: HTMLElement) => within(column).queryAllByRole("tab");
/** CMT-FR-17: the disclosure that holds the resolved discussions. */
const resolvedDisclosure = (column: HTMLElement) =>
  within(column).getByRole("button", { name: /^\d+ resolved$/ });
/** EDT-FR-16: the Editor's action cluster, which holds the column's controls. */
const actionCluster = () => document.querySelector<HTMLElement>(".editor__actions")!;

/**
 * DDS-FR-PNXR / CMT-FR-30: the per-project layout record. `stored` is what the
 * backend reads back, and `saved` is every record the frontend writes.
 */
interface Layout {
  stored: LayoutPreferences;
  saved: LayoutPreferences[];
}

async function mount(
  over: Partial<Backend> = {},
  opts: { artifactId?: string; sessions?: EditSessionStore; layout?: Layout } = {},
) {
  const sessions = opts.sessions ?? new EditSessionStore();
  const artifactId = opts.artifactId ?? "a.md";
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
  const layout = opts.layout;
  if (layout) {
    // The column's hidden state is written to the layout record, which the
    // shared fixture does not answer.
    const base = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(async (cmd: string, args: unknown) => {
      if (cmd === "load_layout_preferences") return layout.stored;
      if (cmd === "save_layout_preferences") {
        backend.calls.push({ cmd, args });
        layout.saved.push((args as { preferences: LayoutPreferences }).preferences);
        return null;
      }
      return base(cmd, args);
    });
    await act(async () => {
      await restoreDiscussionColumns("/repo");
    });
  }
  render(
    <Editor
      artifactId={artifactId}
      artifactName={artifactId}
      artifactType="skill"
      sessions={sessions}
    />,
  );
  await screen.findByLabelText("artifact body");
  return { sessions, backend };
}

describe("CMT-FR-04 / CMT-FR-29: loading and the chooser", () => {
  it("CMT-FR-04, CMT-FR-29, CMT-FR-53: loads the file's discussions once and gives the unresolved one a chooser tab", async () => {
    const { backend } = await mount({ threads: [makeThread()] });
    const column = await openColumn();
    await waitFor(() => expect(chooserTabs(column)).toHaveLength(1));
    expect(
      backend.calls.filter((c) => c.cmd === "list_discussions"),
    ).toHaveLength(1);
    expect(screen.getByTestId("comment-thread-t1")).toBeInTheDocument();
    // The toolbar carries no count: the chooser is where the discussions are
    // told apart.
    expect(actionCluster()).not.toHaveTextContent(/unresolved|\d/);
    expect(discussionToggle()).toHaveAccessibleName("Hide the discussion");
  });

  it("CMT-FR-29, CMT-FR-17, CMT-FR-56: gives a tab to each unresolved discussion only, and puts a resolved one in the disclosure", async () => {
    await mount({
      threads: [
        makeThread({ id: "t1" }),
        makeThread({ id: "t2", resolved: true }),
        makeThread({ id: "t3" }),
      ],
    });
    const column = await openColumn();
    await waitFor(() => expect(chooserTabs(column)).toHaveLength(2));
    const disclosure = resolvedDisclosure(column);
    expect(disclosure).toHaveTextContent("1 resolved");
    expect(disclosure).toHaveAttribute("aria-expanded", "false");
  });

  it("CMT-FR-02, CMT-FR-29, EDT-FR-61: keeps the column, its tab and its toggle in raw-text mode", async () => {
    // A switch to raw text never hides the fact that discussions exist.
    await mount({ threads: [makeThread()] });
    const column = await openColumn();
    await screen.findByTestId("comment-thread-t1");

    fireEvent.click(screen.getByRole("button", { name: "Edit as Markdown source" }));
    await screen.findByLabelText("Markdown source");
    expect(column).toBeVisible();
    expect(chooserTabs(column)).toHaveLength(1);
    expect(screen.getByTestId("comment-thread-t1")).toBeInTheDocument();
    expect(discussionToggle()).toBeInTheDocument();

    // And toggling back renders the same discussion.
    fireEvent.click(screen.getByRole("button", { name: "Edit as rich text" }));
    await screen.findByLabelText("artifact body");
    expect(screen.getByTestId("comment-thread-t1")).toBeInTheDocument();
  });
});

describe("CMT-FR-30 / DDS-FR-XQMF: whether the column shows", () => {
  it("CMT-FR-30, DDS-FR-XQMF: shows the column by default for a file with no discussions, on its opening composer", async () => {
    await mount({ threads: [] });
    const column = await screen.findByTestId("editor-discussion-column");
    expect(column).toBeVisible();
    expect(screen.getByTestId("editor-discussion-split")).toHaveAttribute(
      "data-discussion",
      "shown",
    );
    expect(
      within(column).getByText("Nothing has been said about this file yet."),
    ).toBeInTheDocument();
    const composer = within(column).getByRole("textbox", { name: "Discuss this file" });
    expect(composer).toHaveAttribute("placeholder", "Ask an agent about this file…");
    // No discussion, so no chooser.
    expect(within(column).queryByRole("tablist")).toBeNull();
    expect(discussionToggle()).toHaveAttribute("aria-pressed", "true");
  });

  it("CMT-FR-30: shows the column by default for a file carrying a discussion", async () => {
    await mount({ threads: [makeThread()] });
    const column = await screen.findByTestId("editor-discussion-column");
    expect(column).toBeVisible();
    expect(await within(column).findByTestId("comment-thread-t1")).toBeInTheDocument();
  });

  it("CMT-FR-30, DDS-FR-XQMF: hides the column without unmounting it, and keeps the choice for this file", async () => {
    const layout: Layout = { stored: {}, saved: [] };
    await mount({ threads: [makeThread()] }, { layout });
    const lastSaved = () => layout.saved[layout.saved.length - 1];
    const column = await openColumn();
    fireEvent.change(within(column).getByLabelText("Reply to thread t1"), {
      target: { value: "half written" },
    });

    fireEvent.click(discussionToggle());
    expect(column.hidden).toBe(true);
    expect(column).toBeInTheDocument();
    expect(screen.getByTestId("editor-discussion-split")).toHaveAttribute(
      "data-discussion",
      "hidden",
    );
    expect(discussionToggle()).toHaveAccessibleName("Show the discussion");
    expect(discussionToggle()).toHaveAttribute("aria-pressed", "false");
    await waitFor(() =>
      expect(lastSaved()?.fileDiscussionHidden).toEqual({ "a.md": true }),
    );

    // The column comes back with the reply it held.
    fireEvent.click(discussionToggle());
    expect(column.hidden).toBe(false);
    expect(within(column).getByLabelText("Reply to thread t1")).toHaveValue(
      "half written",
    );
    await waitFor(() =>
      expect(lastSaved()?.fileDiscussionHidden).toEqual({ "a.md": false }),
    );
  });

  it("CMT-FR-30, DDS-FR-XQMF, DDS-FR-PNXR: restores a hidden column and its split for that file alone", async () => {
    const stored = {
      fileDiscussionHidden: { "a.md": true },
      fileDiscussionRatios: { "a.md": "doc70" },
    };
    await mount({ threads: [makeThread()] }, { layout: { stored, saved: [] } });
    expect(await screen.findByTestId("editor-discussion-column")).not.toBeVisible();
    expect(discussionToggle()).toHaveAccessibleName("Show the discussion");
    fireEvent.click(discussionToggle());
    expect(
      within(actionCluster()).getByRole("radio", { name: "Document" }),
    ).toHaveAttribute("aria-checked", "true");

    // Another file, never hidden or split, shows its column at the default.
    cleanup();
    await mount({ threads: [] }, { artifactId: "b.md" });
    expect(await screen.findByTestId("editor-discussion-column")).toBeVisible();
    expect(discussionToggle()).toHaveAccessibleName("Hide the discussion");
    expect(
      within(actionCluster()).getByRole("radio", { name: "Even" }),
    ).toHaveAttribute("aria-checked", "true");
  });

  it("CMT-FR-17, CMT-FR-30: keeps the resolved disclosure expanded across a tab close and reopen", async () => {
    // The disclosure's state belongs to the artifact. The Editor is unmounted
    // and remounted when a tab closes and reopens, so component state would
    // collapse it every time.
    const { sessions } = await mount({ threads: [makeThread({ resolved: true })] });
    const column = await openColumn();

    fireEvent.click(resolvedDisclosure(column));
    await screen.findByTestId("comment-thread-t1");
    expect(resolvedDisclosure(column)).toHaveAttribute("aria-expanded", "true");

    // A mode toggle does not unmount the Editor…
    fireEvent.click(screen.getByRole("button", { name: "Edit as Markdown source" }));
    await screen.findByLabelText("Markdown source");
    fireEvent.click(screen.getByRole("button", { name: "Edit as rich text" }));
    await screen.findByTestId("comment-thread-t1");

    // …and closing the tab unmounts the Editor, which is the case component
    // state could not survive.
    cleanup();
    act(() => sessions.closeTab("a.md"));
    await mount({ threads: [makeThread({ resolved: true })] }, { sessions });
    const reopened = await openColumn();
    expect(resolvedDisclosure(reopened)).toHaveAttribute("aria-expanded", "true");
    expect(await screen.findByTestId("comment-thread-t1")).toBeInTheDocument();
  });

  it("EDT-FR-28, CMT-FR-30: keeps no retained record for a file whose column was only defaulted", async () => {
    const { sessions } = await mount({ threads: [makeThread()] });
    await openColumn();
    // The tab closing means the Editor is gone: a mounted one re-creates the
    // record through `ensure` on its very next render, so unmount first.
    cleanup();
    act(() => sessions.closeTab("a.md"));
    // Reopening is a plain first load, not a resumed session (EDT-FR-28).
    expect(sessions.get("a.md")).toBeUndefined();
  });
});

describe("DDS-FR-PNXR: the split presets and the accelerator", () => {
  it("DDS-FR-PNXR, CMT-FR-29, EDT-FR-16: sets the presets and the hint before the toggle, only while the column shows", async () => {
    await mount({ threads: [makeThread()] });
    await openColumn();
    const cluster = actionCluster();
    const radios = within(cluster).getAllByRole("radio");
    expect(radios.map((r) => r.textContent)).toEqual(["Document", "Even", "Discussion"]);
    expect(within(cluster).getByRole("radio", { name: "Even" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
    const hint = within(cluster).getByText("⌘\\");
    // The toggle stands at the trailing end, with the presets leading of it.
    expect(cluster.lastElementChild?.lastElementChild).toBe(discussionToggle());
    expect(
      radios[0].compareDocumentPosition(discussionToggle()) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(
      hint.compareDocumentPosition(discussionToggle()) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();

    fireEvent.click(discussionToggle());
    expect(within(cluster).queryAllByRole("radio")).toHaveLength(0);
    expect(within(cluster).queryByText("⌘\\")).toBeNull();
    expect(discussionToggle()).toBeInTheDocument();
  });

  it("DDS-FR-PNXR, DDS-FR-XQMF, CMT-FR-30: cycles the presets on Cmd+\\ or Ctrl+\\, keeps the split for this file, and not while the column is hidden", async () => {
    const layout: Layout = { stored: {}, saved: [] };
    await mount({ threads: [makeThread()] }, { layout });
    await openColumn();
    const checked = () =>
      within(actionCluster())
        .getAllByRole("radio")
        .find((r) => r.getAttribute("aria-checked") === "true")?.textContent;
    expect(checked()).toBe("Even");

    fireEvent.keyDown(window, { key: "\\", metaKey: true });
    expect(checked()).toBe("Discussion");
    fireEvent.keyDown(window, { key: "\\", ctrlKey: true });
    expect(checked()).toBe("Document");
    await waitFor(() =>
      expect(layout.saved[layout.saved.length - 1]?.fileDiscussionRatios).toEqual({
        "a.md": "doc70",
      }),
    );

    fireEvent.click(discussionToggle());
    fireEvent.keyDown(window, { key: "\\", metaKey: true });
    fireEvent.click(discussionToggle());
    expect(checked()).toBe("Document");
  });
});

describe("CMT-FR-08 / CMT-FR-09 / CMT-FR-10: what a card renders", () => {
  it("renders a comment's Markdown as rich text and names its author", async () => {
    await mount({ threads: [makeThread()] });
    const column = await openColumn();
    expect(column).toHaveTextContent("raver119");
    // `**which**` renders as emphasis rather than as literal asterisks.
    expect(column.querySelector("strong")?.textContent).toBe("which");
    expect(column.textContent).not.toContain("**which**");
  });

  it("CMT-FR-08: typesets a message body at the UI scale, not the page's", async () => {
    // `.doc` alone is the Editor page's own type scale — a body set at it
    // renders half again the size of the column around it.
    //
    // This asserts only that the body asks for the panel treatment. That the
    // treatment still means anything is a fact about the stylesheet, which
    // jsdom never applies, and lives in src/test/style-invariants.test.ts; the
    // two together are what keep the defect fixed, and either alone lets it back
    // in from one side.
    await mount({ threads: [makeThread()] });
    const column = await openColumn();
    const body = column.querySelector(".dds-message__body");
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
    const column = await openColumn();
    const authors = Array.from(
      column.querySelectorAll<HTMLElement>(".dds-message__author"),
    );
    expect(authors.map((a) => a.dataset.agent === "true")).toEqual([false, true]);
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
    const column = await openColumn();
    const quoted = column.querySelector(".dds-message__quote");
    expect(quoted?.textContent).toContain("raver119");
    expect(quoted?.textContent).toContain("there are three");
  });
});

describe("CMT-FR-11 / CMT-FR-14 / CMT-FR-15: replying, immutability, the menu", () => {
  it("posts a reply through add_comment and appends it to the card", async () => {
    const { backend } = await mount({ threads: [makeThread()] });
    await openColumn();

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
    const column = await openColumn();
    fireEvent.click(screen.getByRole("button", { name: /Thread actions/ }));

    const labels = Array.from(column.querySelectorAll("button")).map(
      (b) => `${b.textContent ?? ""} ${b.getAttribute("aria-label") ?? ""}`.toLowerCase(),
    );
    for (const label of labels) {
      expect(label).not.toContain("edit");
      expect(label).not.toContain("delete");
    }
    // And the menu holds exactly its one entry (CMT-FR-15).
    const menuItems = Array.from(
      column.querySelectorAll<HTMLElement>('[role="menuitem"]'),
    ).map((b) => b.textContent);
    expect(menuItems).toEqual(["Mark resolved"]);
  });

  it("CMT-FR-15, CMT-FR-16, CTA-FR-CSZT: resolving hides the composer, and reopening restores it and dispatches nothing", async () => {
    const { backend } = await mount({ threads: [makeThread()] });
    await openColumn();
    fireEvent.click(screen.getByRole("button", { name: /Thread actions/ }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Mark resolved" }));

    await waitFor(() =>
      expect(
        backend.calls.find((c) => c.cmd === "set_discussion_resolution")?.args,
      ).toMatchObject({ discussionId: "t1", resolved: true }),
    );
    const column = screen.getByTestId("editor-discussion-column");
    await waitFor(() => expect(chooserTabs(column)).toHaveLength(0));
    fireEvent.click(resolvedDisclosure(column));
    const card = await screen.findByTestId("comment-thread-t1");
    expect(screen.queryByLabelText("Reply to thread t1")).toBeNull();
    expect(card).toHaveTextContent("Resolved");
    // The comment itself is still there to read.
    expect(card).toHaveTextContent("Which session?");

    // Reopening brings the composer back and starts no agent turn.
    fireEvent.click(screen.getByRole("button", { name: /Thread actions/ }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Reopen discussion" }));
    await screen.findByLabelText("Reply to thread t1");
    expect(backend.calls.some((c) => c.cmd === "dispatch_agent_turn")).toBe(false);
  });
});

describe("CMT-FR-16 / CMT-FR-17: resolving", () => {
  it("CMT-FR-16, CMT-FR-17, CMT-FR-56: moves a resolved discussion out of the chooser into the disclosure, and reopening returns it", async () => {
    const { backend } = await mount({
      threads: [
        makeThread({ id: "t1" }),
        makeThread({ id: "t2", createdAt: "2026-01-01T00:00:01Z" }),
      ],
    });
    const column = await openColumn();
    await waitFor(() => expect(chooserTabs(column)).toHaveLength(2));
    // The column reads the first discussion where none is chosen.
    expect(screen.getByTestId("comment-thread-t1")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Thread actions for t1" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Mark resolved" }));

    await waitFor(() =>
      expect(
        backend.calls.find((c) => c.cmd === "set_discussion_resolution")?.args,
      ).toMatchObject({ discussionId: "t1", resolved: true }),
    );
    // Out of the chooser and behind a collapsed disclosure.
    await waitFor(() => expect(chooserTabs(column)).toHaveLength(1));
    expect(screen.queryByTestId("comment-thread-t1")).toBeNull();
    const disclosure = resolvedDisclosure(column);
    expect(disclosure).toHaveTextContent("1 resolved");
    expect(disclosure).toHaveAttribute("aria-expanded", "false");

    // Expanding it offers the discussion again, and it reads in full.
    fireEvent.click(disclosure);
    expect(disclosure).toHaveAttribute("aria-expanded", "true");
    const tabs = chooserTabs(column);
    expect(tabs).toHaveLength(2);
    fireEvent.click(tabs[1]);
    expect(await screen.findByTestId("comment-thread-t1")).toHaveTextContent(
      "Which session?",
    );
    fireEvent.click(screen.getByRole("button", { name: "Thread actions for t1" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Reopen discussion" }));
    await waitFor(() =>
      expect(within(column).queryByRole("button", { name: /^\d+ resolved$/ })).toBeNull(),
    );
    expect(chooserTabs(column)).toHaveLength(2);
  });

  it("CMT-FR-15: a resolved discussion's menu holds the one entry that reopens it", async () => {
    await mount({ threads: [makeThread({ resolved: true })] });
    const column = await openColumn();
    // With every discussion resolved the column says so, over its composer.
    expect(
      within(column).getByText("The discussion about this file is resolved."),
    ).toBeInTheDocument();
    fireEvent.click(resolvedDisclosure(column));
    fireEvent.click(await screen.findByRole("button", { name: /Thread actions/ }));
    const items = screen.getAllByRole("menuitem").map((i) => i.textContent);
    expect(items).toEqual(["Reopen discussion"]);
  });
});

describe("CMT-FR-19: orphaned threads", () => {
  it("CMT-FR-18, CMT-FR-19: renders a discussion whose passage is gone as orphaned, still repliable", async () => {
    await mount({
      threads: [
        makeThread({
          id: "t-orphan",
          fragmentTarget: fragment({ start: 5, end: 5, quote: "a passage that is not in the file" }),
        }),
      ],
    });
    const column = await openColumn();
    const card = await screen.findByTestId("comment-thread-t-orphan");
    await waitFor(() => expect(column).toHaveTextContent("Orphaned"));
    // It shows the quote it was attached to.
    expect(card).toHaveTextContent("a passage that is not in the file");
    expect(card).toHaveAttribute("data-availability", "orphaned");
    // Fully readable, still carries its menu, and still accepts a reply.
    expect(screen.getByRole("button", { name: /Thread actions/ })).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.getByLabelText("Reply to thread t-orphan")).toBeEnabled(),
    );
  });

  it("CMT-FR-29: gives an orphaned discussion its own chooser tab", async () => {
    // It is the discussion most at risk of being forgotten.
    await mount({
      threads: [
        makeThread({ id: "t-orphan", fragmentTarget: fragment({ start: 0, end: 0, quote: "nowhere at all" }) }),
      ],
    });
    const column = await openColumn();
    await waitFor(() => expect(chooserTabs(column)).toHaveLength(1));
  });
});

describe("CMT-FR-24 / CMT-FR-25 / CMT-FR-26: identity gating", () => {
  it("CMT-FR-24, CMT-FR-26: enables commenting as Me when no token is stored, with no missing-token reason", async () => {
    await mount({ threads: [makeThread()], identity: LOCAL_HUMAN });
    const column = await openColumn();
    await waitFor(() =>
      expect(screen.getByLabelText("Reply to thread t1")).toBeEnabled(),
    );
    expect(column).not.toHaveTextContent(/GitHub account/i);
    expect(column).not.toHaveTextContent("Global settings → GitHub");
  });

  it("gives a token that needs picking a different reason from one that needs adding", async () => {
    // The two call for different actions, so they must not collapse.
    await mount({
      threads: [makeThread()],
      identityError: GITHUB_TOKEN_ERRORS.selectionRequired,
    });
    const column = await openColumn();
    expect(column).toHaveTextContent(/Choose which GitHub token/i);
    expect(column).not.toHaveTextContent("Global settings → GitHub");
  });

  it("distinguishes an unreachable GitHub from a missing token", async () => {
    await mount({
      threads: [makeThread()],
      identityError: GITHUB_TOKEN_ERRORS.githubUnreachable,
    });
    const column = await openColumn();
    expect(column).toHaveTextContent(/Could not reach GitHub/i);
  });

  it("enables commenting when an identity resolves", async () => {
    await mount({ threads: [makeThread()] });
    await openColumn();
    await waitFor(() =>
      expect(screen.getByLabelText("Reply to thread t1")).toBeEnabled(),
    );
  });
});

describe("CMT-FR-ZCAE: the local participant's label", () => {
  const authorNames = () =>
    Array.from(document.querySelectorAll(".dds-message__author")).map(
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
    await openColumn();
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
    await openColumn();
    await waitFor(() => expect(authorNames()[0]).toBe("octocat"));

    backend.identity = undefined;
    backend.identityError = GITHUB_TOKEN_ERRORS.selectionRequired;
    tokensChanged();
    await waitFor(() => expect(authorNames()[0]).toBe("Me"));
    expect(screen.getByLabelText("Reply to thread t1")).toBeDisabled();
  });

  it("CMT-FR-24, CMT-FR-ZCAE: posts nothing the author could mistake for another account", async () => {
    const { backend } = await mount({ threads: [localThread()], identity: LOCAL_HUMAN });
    await openColumn();
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
  it("CMT-FR-34, CMS-FR-17: re-renders the card as resolved and states the refusal in words", async () => {
    // Someone else resolved the thread since the column last read it. The fixture
    // models exactly that — the refusal AND the state change behind it — so the
    // "re-renders as resolved" clause is actually testable.
    const { backend } = await mount({
      threads: [makeThread()],
      addCommentError: "discussion_resolved",
    });
    await openColumn();
    backend.threads = backend.threads.map((t) => ({ ...t, resolved: true }));

    const composer = screen.getByLabelText("Reply to thread t1") as HTMLTextAreaElement;
    fireEvent.change(composer, { target: { value: "my careful reply" } });
    fireEvent.click(screen.getByRole("button", { name: "Post" }));

    // The discussion re-renders as resolved: it leaves the chooser for the
    // disclosure, and it carries no composer to invite a second attempt.
    const column = screen.getByTestId("editor-discussion-column");
    await waitFor(() => expect(chooserTabs(column)).toHaveLength(0));
    fireEvent.click(resolvedDisclosure(column));
    const card = await screen.findByTestId("comment-thread-t1");
    expect(screen.queryByLabelText("Reply to thread t1")).toBeNull();
    // Prose, not the wire slug.
    const alert = await within(card).findByRole("alert");
    expect(alert.textContent).toMatch(/resolved/i);
    expect(alert.textContent).not.toBe("discussion_resolved");
  });

  it("keeps the typed body when the refusal leaves the thread writable", async () => {
    // A transient failure must not cost the author what they wrote.
    const { backend } = await mount({
      threads: [makeThread()],
      addCommentError: "discussion_not_found",
    });
    await openColumn();
    const composer = screen.getByLabelText("Reply to thread t1") as HTMLTextAreaElement;
    fireEvent.change(composer, { target: { value: "my careful reply" } });
    fireEvent.click(screen.getByRole("button", { name: "Post" }));

    await screen.findByRole("alert");
    expect(
      (screen.getByLabelText("Reply to thread t1") as HTMLTextAreaElement).value,
    ).toBe("my careful reply");
    // And the column re-read, so a thread someone else changed is refreshed.
    await waitFor(() =>
      expect(
        backend.calls.filter((c) => c.cmd === "list_discussions").length,
      ).toBeGreaterThan(1),
    );
  });
});

describe("CMT-FR-31 / EDT-FR-61: the column is not part of the editing surface", () => {
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
    await openColumn();
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
    await openColumn();
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

describe("CMT-FR-32: the column's own transient surfaces", () => {
  it("CMT-FR-32: keeps one overflow menu, for the discussion on screen, and dismisses it on Escape", async () => {
    const { backend } = await mount({
      threads: [makeThread({ id: "t1" }), makeThread({ id: "t2" })],
    });
    const column = await openColumn();
    await waitFor(() => expect(chooserTabs(column)).toHaveLength(2));
    // One discussion reads at a time, so one menu control stands.
    expect(screen.getAllByRole("button", { name: /Thread actions/ })).toHaveLength(1);

    fireEvent.click(screen.getByRole("button", { name: "Thread actions for t1" }));
    expect(screen.getAllByRole("menuitem")).toHaveLength(1);

    // Choosing the other discussion with the pointer leaves no menu open: the
    // pointer-down outside the menu dismisses it.
    const tab = chooserTabs(column)[1];
    fireEvent.mouseDown(tab);
    fireEvent.click(tab);
    await screen.findByTestId("comment-thread-t2");
    expect(screen.queryByRole("button", { name: "Thread actions for t1" })).toBeNull();
    const second = screen.getByRole("button", { name: "Thread actions for t2" });
    expect(screen.queryAllByRole("menuitem")).toHaveLength(0);

    fireEvent.click(second);
    expect(screen.getAllByRole("menuitem")).toHaveLength(1);
    expect(second).toHaveAttribute("aria-expanded", "true");
    fireEvent.keyDown(document, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("menuitem")).toBeNull());
    // A dismissal invokes nothing.
    expect(backend.calls.some((c) => c.cmd === "set_discussion_resolution")).toBe(false);
  });
});

describe("CMT-FR-25: the token picker", () => {
  /** A backend whose identity needs a token picked until `bound` is set. */
  function pickerBackend(threads: Backend["threads"]) {
    const state = { bound: false };
    const backend: Backend = { load: { body: BODY, checksum: "ck1" }, threads, calls: [] };
    invokeMock.mockImplementation(async (cmd: string, args: unknown) => {
      backend.calls.push({ cmd, args });
      if (cmd === "load_artifact_contents_by_id") return backend.load;
      if (cmd === "list_discussions") return backend.threads;
      if (cmd === "resolve_comment_author_identity") {
        if (!state.bound) throw GITHUB_TOKEN_ERRORS.selectionRequired;
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
      if (cmd === "append_log_records") return undefined;
      if (cmd === "estimate_discussion_context") return CONTEXT_ESTIMATE;
      throw new Error(`unexpected invoke ${cmd}`);
    });
    render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
        sessions={new EditSessionStore()}
      />,
    );
    return { state, backend };
  }

  it("CMT-FR-25, GHA-FR-16: offers Choose a token… in the column of a file with no discussion, and re-resolves on confirm", async () => {
    // Stating the reason without offering the action would leave the author
    // with nothing to do about it.
    const { state, backend } = pickerBackend([]);
    await screen.findByLabelText("artifact body");
    const column = await openColumn();
    await waitFor(() => expect(column).toHaveTextContent(/Choose which GitHub token/i));
    const composer = within(column).getByRole("textbox", { name: "Discuss this file" });
    expect(composer).toBeDisabled();

    // The action opens the picker.
    fireEvent.click(within(column).getByRole("button", { name: "Choose a token…" }));
    await screen.findByText(/work/);

    // Confirming re-resolves the identity, which is what enables commenting.
    state.bound = true;
    fireEvent.click(screen.getByRole("button", { name: /^Use$/ }));
    await waitFor(() => expect(composer).toBeEnabled());
    expect(backend.calls.some((c) => c.cmd === "set_project_github_token_binding")).toBe(
      true,
    );
    expect(within(column).queryByRole("button", { name: "Choose a token…" })).toBeNull();
  });

  it("CMT-FR-25, GHA-FR-16: offers Choose a token… while a discussion is on screen, and re-resolves on confirm", async () => {
    const { state } = pickerBackend([
      { ...makeThread(), fragmentTarget: fragment({ start: 0, end: 5, quote: BODY.slice(0, 5) }) },
    ]);
    await screen.findByLabelText("artifact body");
    const column = await openColumn();
    await waitFor(() => expect(column).toHaveTextContent(/Choose which GitHub token/i));
    expect(screen.getByLabelText("Reply to thread t1")).toBeDisabled();

    fireEvent.click(within(column).getByRole("button", { name: "Choose a token…" }));
    await screen.findByText(/work/);
    state.bound = true;
    fireEvent.click(screen.getByRole("button", { name: /^Use$/ }));
    await waitFor(() =>
      expect(screen.getByLabelText("Reply to thread t1")).toBeEnabled(),
    );
  });

  it("CMT-FR-25, CMT-FR-26: offers no picker and no settings route when nothing is stored", async () => {
    await mount({ threads: [makeThread()], identity: LOCAL_HUMAN });
    const column = await openColumn();
    await waitFor(() =>
      expect(screen.getByLabelText("Reply to thread t1")).toBeEnabled(),
    );
    expect(screen.queryByRole("button", { name: "Choose a token…" })).toBeNull();
    expect(within(column).queryByText(/Global settings → GitHub/)).toBeNull();
  });
});
