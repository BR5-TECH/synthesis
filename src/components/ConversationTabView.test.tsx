/**
 * The conversation tab (`TAB-tabs.md` TAB-FR-QXMV, TAB-FR-25;
 * `CVP-conversation-presentation.md` CVP-FR-02, CVP-FR-45, CVP-FR-46,
 * CVP-FR-47, CVP-FR-57, CVP-FR-60; `NTS-notes.md` NTS-FR-27).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (..._args: unknown[]) => () => {}),
  emit: vi.fn(async () => {}),
}));

import { ConversationTabView } from "./ConversationTabView";
import { clearConversationThreads, publishThread } from "../state/conversationThreads";
import { requestPendingFocus, resetDiscussionFocus } from "../state/discussionFocus";
import {
  clearAllDiscussionSessions,
  getDiscussionSession,
  mergeDiscussionTurn,
  patchDiscussionSession,
  setComposerBody,
} from "../state/discussionSession";
import {
  publishKnownArtifacts,
  resetOwnerAvailability,
} from "../state/ownerAvailability";
import { resetSharedCommentIdentity } from "../hooks/useSharedCommentIdentity";
import { resetProjectIdentity } from "../state/projectIdentity";
import type { AgentTurn, Comment, Discussion, Tab } from "../types";
import {
  noteDiscussionOrigin,
} from "../test/origins";

const human = { kind: "human", login: "raver119" } as const;
const localHuman = { kind: "human", login: "", displayName: "Me" } as const;

function comment(id: string, body: string): Comment {
  return { id, author: human, body, quotes: [], attachments: [], createdAt: "2026-02-01T00:00:00Z" };
}

function discussion(over: Partial<Discussion> = {}): Discussion {
  return {
    id: "d1",
    target: { kind: "note", noteId: "n1" },
    fragmentTarget: null,
    comments: [comment("c1", "first"), comment("c2", "second")],
    locked: false,
    resolved: false,
    createdAt: "2026-02-01T00:00:00Z",
    updatedAt: "2026-02-01T00:00:00Z",
    ...over,
  };
}

const noteTab = (over: Partial<Tab> = {}): Tab => ({
  id: "conv:d1",
  label: "Chat: Ask legal",
  tooltip: "what legal said",
  kind: "conversation",
  threadId: "d1",
  noteId: "n1",
  ownerTarget: { kind: "note", noteId: "n1" },
  ownerLabel: "Ask legal",
  subject: "what legal said",
  ...over,
});

const runningTurn = {
  id: "turn-1",
  agentId: "a1",
  nickname: "arch",
  origin: noteDiscussionOrigin("d1", "n1"),
  triggerCommentId: "c2",
  state: "running",
  failure: null,
  retryPermitted: false,
  startedAt: "2026-02-01T00:00:00Z",
  endedAt: null,
  activeToolCalls: [],
} as unknown as AgentTurn;

const commands = () => invokeMock.mock.calls.map((c) => c[0] as string);

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "resolve_comment_author_identity") return human;
    if (cmd === "list_agent_turns") return [];
    return undefined;
  });
  clearConversationThreads();
  clearAllDiscussionSessions();
  resetDiscussionFocus();
  resetOwnerAvailability();
  resetSharedCommentIdentity();
  resetProjectIdentity();
});
afterEach(cleanup);

describe("TAB-FR-QXMV, CVP-FR-57: the tab hosts the shared surface", () => {
  it("renders the discussion through the shared surface with owner tab, named Chat: <owner>", async () => {
    publishThread(discussion());
    render(<ConversationTabView tab={noteTab()} onOpened={vi.fn()} />);

    const surface = await screen.findByTestId("comment-thread-d1");
    expect(surface).toHaveAttribute("data-discussion-owner", "tab");
    expect(screen.getByTestId("conversation-tab-view")).toHaveAttribute(
      "aria-label",
      "Chat: Ask legal",
    );
    expect(screen.getByText("Chat: Ask legal")).toBeInTheDocument();
    expect(screen.getByText(/what legal said/)).toBeInTheDocument();
    expect(screen.getByText("first")).toBeInTheDocument();
    // The one composer of every discussion.
    expect(screen.getByRole("textbox")).toBeInTheDocument();
  });

  it("CVP-FR-02: a second tab view of one discussion renders no second surface", async () => {
    publishThread(discussion());
    render(
      <>
        <ConversationTabView tab={noteTab()} onOpened={vi.fn()} />
        <ConversationTabView tab={noteTab({ id: "conv:other" })} onOpened={vi.fn()} />
      </>,
    );
    await screen.findByTestId("comment-thread-d1");
    expect(screen.getAllByTestId("comment-thread-d1")).toHaveLength(1);
    expect(screen.getAllByText("first")).toHaveLength(1);
  });

  it("shows a loading state, not an empty conversation, before the thread is read", () => {
    render(<ConversationTabView tab={noteTab()} onOpened={vi.fn()} />);
    expect(screen.getByRole("status")).toHaveTextContent(/loading the conversation/i);
  });
});

describe("CVP-FR-45, CVP-FR-46: the unavailable-owner fallback tab", () => {
  const fallbackTab = noteTab({
    ownerTarget: { kind: "artifact", artifactId: "docs/gone.md" },
    noteId: undefined,
    ownerLabel: "docs/gone.md",
    label: "Chat: gone.md",
  });
  const artifactThread = () =>
    discussion({ target: { kind: "artifact", artifactId: "docs/gone.md" } });

  it("names what is missing in words and keeps the composer enabled", async () => {
    publishKnownArtifacts(["docs/other.md"]);
    publishThread(artifactThread());
    render(<ConversationTabView tab={fallbackTab} onOpened={vi.fn()} />);

    await screen.findByTestId("comment-thread-d1");
    expect(screen.getByTestId("comment-thread-d1")).toHaveAttribute(
      "data-availability",
      "unavailable",
    );
    expect(
      screen.getByText(/Owner unavailable: docs\/gone\.md no longer exists/),
    ).toBeInTheDocument();
    expect(screen.getByRole("textbox")).toBeEnabled();
  });

  it("renders as available while the owner resolves", async () => {
    publishKnownArtifacts(["docs/gone.md"]);
    publishThread(artifactThread());
    render(<ConversationTabView tab={fallbackTab} onOpened={vi.fn()} />);
    await screen.findByTestId("comment-thread-d1");
    expect(screen.getByTestId("comment-thread-d1")).toHaveAttribute(
      "data-availability",
      "available",
    );
  });
});

describe("TAB-FR-25, CVP-FR-47: closing and reopening the tab keeps the session", () => {
  it("keeps the unsent composer text, the pending placeholder, and the unread state", async () => {
    publishThread(discussion());
    act(() => {
      setComposerBody("d1", "an unsent reply");
      mergeDiscussionTurn("d1", runningTurn);
      patchDiscussionSession("d1", {
        atTail: false,
        unreadCount: 2,
        firstUnreadId: "c2",
        seenIds: ["c1", "c2"],
      });
    });

    const first = render(<ConversationTabView tab={noteTab()} onOpened={vi.fn()} />);
    await screen.findByTestId("comment-thread-d1");
    expect(screen.getByRole("textbox")).toHaveValue("an unsent reply");
    expect(screen.getByTestId("comment-pending")).toHaveTextContent("arch");
    first.unmount();

    // The tab closed. Nothing was written to the discussion.
    expect(commands().filter((c) => ["add_comment", "set_discussion_lock", "set_discussion_resolution"].includes(c))).toHaveLength(0);

    render(<ConversationTabView tab={noteTab()} onOpened={vi.fn()} />);
    await screen.findByTestId("comment-thread-d1");
    expect(screen.getByRole("textbox")).toHaveValue("an unsent reply");
    expect(screen.getByTestId("comment-pending")).toHaveTextContent("arch");
    expect(getDiscussionSession("d1").unreadCount).toBe(2);
    expect(getDiscussionSession("d1").firstUnreadId).toBe("c2");
  });
});

describe("NTS-FR-27, NTS-FR-QVSP, CVP-FR-TIBK: a note discussion without a GitHub token", () => {
  const openingTab = noteTab({ id: "conv:note:n1", threadId: undefined });
  const authors = () =>
    Array.from(document.querySelectorAll(".comment__author")).map((el) => el.textContent);

  it("opens the note discussion as Me: the composer is enabled and offers no token setup", async () => {
    const created = discussion({
      comments: [{ ...comment("c1", "what did legal say?"), author: localHuman }],
    });
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "resolve_comment_author_identity") return localHuman;
      if (cmd === "get_or_create_note_discussion") return created;
      return undefined;
    });
    const onOpened = vi.fn();
    render(<ConversationTabView tab={openingTab} onOpened={onOpened} />);

    const field = await screen.findByRole("textbox");
    await waitFor(() => expect(field).toBeEnabled());
    expect(screen.queryByText(/GitHub account|Global settings|Choose a token/i)).toBeNull();
    await userEvent.type(field, "what did legal say?");
    fireEvent.keyDown(field, { key: "Enter", ctrlKey: true });

    await waitFor(() => expect(onOpened).toHaveBeenCalledTimes(1));
    const [call] = invokeMock.mock.calls.filter((c) => c[0] === "get_or_create_note_discussion");
    expect(Object.keys(call[1] as object).sort()).toEqual(["attachments", "body", "noteId"]);
  });

  it("labels the fixed local participant's comments Me, then by the project login once a token resolves", async () => {
    let identity: unknown = localHuman;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "resolve_comment_author_identity") return identity;
      if (cmd === "list_agent_turns") return [];
      return undefined;
    });
    const d = discussion({
      comments: [
        { ...comment("c1", "first"), author: localHuman },
        comment("c2", "second"),
      ],
    });
    publishThread(d);
    render(<ConversationTabView tab={noteTab()} onOpened={vi.fn()} />);
    await waitFor(() => expect(authors()).toEqual(["Me", "raver119"]));

    identity = { kind: "human", login: "octocat" };
    await act(async () => {
      const { listen } = await import("@tauri-apps/api/event");
      const call = (listen as unknown as { mock: { calls: unknown[][] } }).mock.calls.find(
        (c) => c[0] === "github-tokens-changed",
      );
      (call?.[1] as () => void)();
    });
    await waitFor(() => expect(authors()).toEqual(["octocat", "raver119"]));
    expect(d.comments[0].author).toEqual(localHuman);
  });
});

describe("CVP-FR-60, NTS-FR-27: the opening state of a note", () => {
  const openingTab = noteTab({ id: "conv:note:n1", threadId: undefined });

  it("renders the composer alone and invokes nothing on mount", async () => {
    render(<ConversationTabView tab={openingTab} onOpened={vi.fn()} />);

    expect(await screen.findByTestId("note-discussion-opener")).toBeInTheDocument();
    expect(screen.getByRole("textbox")).toBeInTheDocument();
    expect(screen.queryByTestId("discussion-messages")).toBeNull();
    expect(screen.queryByText(/^Active$|^Locked$|^Resolved$/)).toBeNull();
    await waitFor(() => expect(commands()).toContain("resolve_comment_author_identity"));
    const writes = commands().filter((c) =>
      ["get_or_create_note_discussion", "open_discussion", "add_comment", "dispatch_agent_turn"].includes(c),
    );
    expect(writes).toHaveLength(0);
  });

  it("posts with Ctrl+Enter, creates the discussion once, and binds the same tab", async () => {
    const created = discussion({ comments: [comment("c1", "what did legal say?")] });
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "resolve_comment_author_identity") return human;
      if (cmd === "get_or_create_note_discussion") return created;
      return undefined;
    });
    const onOpened = vi.fn();
    render(<ConversationTabView tab={openingTab} onOpened={onOpened} />);

    const field = await screen.findByRole("textbox");
    await userEvent.type(field, "what did legal say?");
    await waitFor(() => expect(field).toHaveValue("what did legal say?"));
    fireEvent.keyDown(field, { key: "Enter", ctrlKey: true });

    await waitFor(() => expect(onOpened).toHaveBeenCalledTimes(1));
    expect(onOpened).toHaveBeenCalledWith("conv:note:n1", created);
    const calls = invokeMock.mock.calls.filter((c) => c[0] === "get_or_create_note_discussion");
    expect(calls).toHaveLength(1);
    expect(calls[0][1]).toEqual(
      expect.objectContaining({ noteId: "n1", body: "what did legal say?" }),
    );
    // Posting a message that is not a duplicate opening appends nothing extra.
    expect(commands()).not.toContain("add_comment");
    // The composer cleared and the typed text is gone from the session.
    expect(getDiscussionSession("note:n1").body).toBe("");
  });

  it("keeps the typed text when the backend refuses", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "resolve_comment_author_identity") return human;
      if (cmd === "get_or_create_note_discussion") throw "note_not_found";
      return undefined;
    });
    const onOpened = vi.fn();
    render(<ConversationTabView tab={openingTab} onOpened={onOpened} />);

    const field = await screen.findByRole("textbox");
    await userEvent.type(field, "hello");
    await userEvent.keyboard("{Control>}{Enter}{/Control}");

    await waitFor(() => expect(screen.getByRole("alert")).toBeInTheDocument());
    expect(onOpened).not.toHaveBeenCalled();
    expect(getDiscussionSession("note:n1").body).toBe("hello");
  });

  it("takes a held focus request into the composer", async () => {
    requestPendingFocus("note:n1", "composer");
    render(<ConversationTabView tab={openingTab} onOpened={vi.fn()} />);
    const field = await screen.findByRole("textbox");
    await waitFor(() => expect(field).toHaveFocus());
  });
});

describe("CVP-FR-43: an owner becoming unavailable is announced", () => {
  it("announces once when the file goes while the tab is open", async () => {
    const { useDiscussionAnnouncement } = await import("../state/discussionAnnouncer");
    publishKnownArtifacts(["docs/gone.md"]);
    publishThread(
      discussion({ target: { kind: "artifact", artifactId: "docs/gone.md" } }),
    );
    function Probe() {
      return <span data-testid="said">{useDiscussionAnnouncement().message}</span>;
    }
    render(
      <>
        <Probe />
        <ConversationTabView
          tab={noteTab({
            ownerTarget: { kind: "artifact", artifactId: "docs/gone.md" },
            noteId: undefined,
            label: "Chat: gone.md",
          })}
          onOpened={vi.fn()}
        />
      </>,
    );
    await screen.findByTestId("comment-thread-d1");
    expect(screen.getByTestId("said")).toHaveTextContent("");

    act(() => publishKnownArtifacts(["docs/other.md"]));
    await waitFor(() =>
      expect(screen.getByTestId("said")).toHaveTextContent(/Chat: gone\.md.*no longer/),
    );
  });
});
