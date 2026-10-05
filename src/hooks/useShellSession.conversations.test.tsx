/**
 * Conversation tabs in the strip and the one reveal route
 * (`TAB-tabs.md` TAB-FR-23 … TAB-FR-25, TAB-FR-QXMV;
 * `CVP-conversation-presentation.md` CVP-FR-06, CVP-FR-07, CVP-FR-47,
 * CVP-FR-49, CVP-FR-54, CVP-FR-60, CVP-FR-61).
 *
 * Held apart from the main `useShellSession` suite because these mount the
 * session stores and the focus registry as well as the shell, and every one of
 * them has to reset them again.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook } from "@testing-library/react";

import { useShellSession } from "./useShellSession";
import { clearConversationThreads } from "../state/conversationThreads";
import {
  consumePendingFocus,
  registerSurface,
  resetDiscussionFocus,
  surfaceCount,
} from "../state/discussionFocus";
import {
  clearAllDiscussionSessions,
  getDiscussionSession,
  setComposerBody,
} from "../state/discussionSession";
import {
  publishKnownArtifacts,
  resetOwnerAvailability,
} from "../state/ownerAvailability";
import type { DiscussionReveal } from "../state/revealDiscussion";
import type { Tab } from "../types";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

type EventHandler = (ev: { payload: unknown }) => void;
let eventHandlers: [string, EventHandler][] = [];
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, handler: EventHandler) => {
    eventHandlers.push([name, handler]);
    return Promise.resolve(() => {
      eventHandlers = eventHandlers.filter(([, h]) => h !== handler);
    });
  },
}));
const fireBus = (name: string, payload: unknown) =>
  eventHandlers.filter(([n]) => n === name).forEach(([, h]) => h({ payload }));

const noteReveal = (
  discussionId: string | undefined,
  noteId = "n1",
): DiscussionReveal => ({
  discussionId,
  target: { kind: "note", noteId },
  ownerLabel: "Ask legal",
  subject: "what legal said",
});

const artifactReveal = (discussionId: string, path = "docs/a.md"): DiscussionReveal => ({
  discussionId,
  target: { kind: "artifact", artifactId: path },
  ownerLabel: path,
  subject: "a passage",
});

function reset() {
  clearConversationThreads();
  clearAllDiscussionSessions();
  resetDiscussionFocus();
  resetOwnerAvailability();
}

beforeEach(() => {
  eventHandlers = [];
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "save_artifact_contents") return { checksum: "ck" };
    if (cmd === "load_artifact_contents_by_id") return { body: "", checksum: "ck" };
    return undefined;
  });
  reset();
});
afterEach(() => {
  cleanup();
  reset();
});

const convTabs = (tabs: readonly Tab[]) =>
  tabs.filter((t) => t.kind === "conversation");

describe("TAB-FR-QXMV, CVP-FR-54, TAB-FR-23: a note opens one conversation tab", () => {
  it("opens it at the end of the strip, names it Chat: <owner>, and focuses it", () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openArtifact({ id: "docs/a.md", name: "a.md" }));

    let outcome: string | undefined;
    act(() => {
      outcome = result.current.revealDiscussion(noteReveal("t1"));
    });

    expect(outcome).toBe("tab");
    const tabs = convTabs(result.current.tabs);
    expect(tabs).toHaveLength(1);
    expect(result.current.tabs[result.current.tabs.length - 1]).toBe(tabs[0]);
    // CVP-FR-57: the strip names it the way its header does, and the subject is
    // on the tooltip.
    expect(tabs[0].label).toBe("Chat: Ask legal");
    expect(tabs[0].tooltip).toBe("what legal said");
    expect(result.current.activeTab).toBe(tabs[0].id);
  });

  it("names an artifact's fallback tab by its file name", () => {
    const { result } = renderHook(() => useShellSession());
    act(() => {
      result.current.revealDiscussion({
        ...artifactReveal("t1", "docs/onboarding.md"),
        ownerUnavailable: true,
      });
    });
    expect(convTabs(result.current.tabs)[0].label).toBe("Chat: onboarding.md");
  });

  it("jumps to the existing tab for a second reveal, creating no second", () => {
    const { result } = renderHook(() => useShellSession());
    act(() => {
      result.current.revealDiscussion(noteReveal("t1"));
    });
    act(() => result.current.openArtifact({ id: "other.md", name: "other.md" }));
    expect(result.current.activeTab).toBe("art:other.md");

    act(() => {
      result.current.revealDiscussion(noteReveal("t1"));
    });
    expect(convTabs(result.current.tabs)).toHaveLength(1);
    expect(result.current.activeTab).toBe(convTabs(result.current.tabs)[0].id);
  });

  it("leaves one tab when two reveals arrive in the same tick (CVP-FR-07)", () => {
    const { result } = renderHook(() => useShellSession());
    act(() => {
      result.current.revealDiscussion(noteReveal("t1"));
      result.current.revealDiscussion(noteReveal("t1"));
      result.current.revealDiscussion(noteReveal(undefined));
    });
    expect(convTabs(result.current.tabs)).toHaveLength(1);
  });

  it("opens a second tab for a second discussion", () => {
    const { result } = renderHook(() => useShellSession());
    act(() => {
      result.current.revealDiscussion(noteReveal("t1", "n1"));
      result.current.revealDiscussion(noteReveal("t2", "n2"));
    });
    expect(convTabs(result.current.tabs)).toHaveLength(2);
  });

  it("holds the focus request for the surface that mounts later", () => {
    const { result } = renderHook(() => useShellSession());
    act(() => {
      result.current.revealDiscussion(noteReveal("t1"));
    });
    expect(consumePendingFocus("t1")).toBe("discussion");
  });
});

describe("CVP-FR-60: a note with no discussion opens the opening state", () => {
  it("opens one tab keyed by the note, asking the backend for nothing", () => {
    const { result } = renderHook(() => useShellSession());
    invokeMock.mockClear();
    act(() => {
      result.current.revealDiscussion(noteReveal(undefined));
      result.current.revealDiscussion(noteReveal(undefined));
    });
    const tabs = convTabs(result.current.tabs);
    expect(tabs).toHaveLength(1);
    expect(tabs[0].threadId).toBeUndefined();
    expect(tabs[0].noteId).toBe("n1");
    expect(consumePendingFocus("note:n1")).toBe("composer");
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("binds the same tab to the created discussion, in place", () => {
    const { result } = renderHook(() => useShellSession());
    act(() => {
      result.current.revealDiscussion(noteReveal(undefined));
    });
    const before = convTabs(result.current.tabs)[0];

    act(() =>
      result.current.bindConversationTab(before.id, {
        id: "t9",
        target: { kind: "note", noteId: "n1" },
        fragmentTarget: null,
        comments: [],
        locked: false,
        resolved: false,
        createdAt: "",
        updatedAt: "",
      }),
    );
    const after = convTabs(result.current.tabs);
    expect(after).toHaveLength(1);
    expect(after[0].id).toBe(before.id);
    expect(after[0].threadId).toBe("t9");

    // A later reveal by the discussion's id focuses that tab.
    act(() => result.current.activateTab("dashboard"));
    act(() => {
      result.current.revealDiscussion(noteReveal("t9"));
    });
    expect(convTabs(result.current.tabs)).toHaveLength(1);
    expect(result.current.activeTab).toBe(before.id);
  });

  it("discards an unposted opening message when the tab closes", async () => {
    const { result } = renderHook(() => useShellSession());
    act(() => {
      result.current.revealDiscussion(noteReveal(undefined));
    });
    act(() => setComposerBody("note:n1", "half a thought"));
    const id = convTabs(result.current.tabs)[0].id;

    await act(async () => {
      await result.current.closeTab(id);
    });
    expect(getDiscussionSession("note:n1").body).toBe("");
    expect(
      invokeMock.mock.calls.filter(([c]) => String(c).includes("discussion")),
    ).toHaveLength(0);
  });
});

describe("CVP-FR-54: the owner decides where an available discussion goes", () => {
  it("opens the artifact's Editor tab and no conversation tab", () => {
    const { result } = renderHook(() => useShellSession());
    publishKnownArtifacts(["docs/a.md"]);

    let outcome: string | undefined;
    act(() => {
      outcome = result.current.revealDiscussion(artifactReveal("t1"));
    });
    expect(outcome).toBe("owner");
    expect(result.current.tabs.some((t) => t.id === "art:docs/a.md")).toBe(true);
    expect(convTabs(result.current.tabs)).toHaveLength(0);
    expect(result.current.focusThread).toEqual({
      artifactId: "docs/a.md",
      threadId: "t1",
    });
    expect(consumePendingFocus("t1")).toBe("discussion");
  });

  it("opens one fallback tab when the owner no longer resolves", () => {
    const { result } = renderHook(() => useShellSession());
    publishKnownArtifacts(["docs/other.md"]);

    let outcome: string | undefined;
    act(() => {
      outcome = result.current.revealDiscussion(artifactReveal("t1"));
      result.current.revealDiscussion(artifactReveal("t1"));
    });
    expect(outcome).toBe("tab");
    expect(result.current.tabs.some((t) => t.id === "art:docs/a.md")).toBe(false);
    const tabs = convTabs(result.current.tabs);
    expect(tabs).toHaveLength(1);
    expect(tabs[0].ownerTarget).toEqual({ kind: "artifact", artifactId: "docs/a.md" });
  });

  it("opens the draft's New Artifact tab for an available draft", () => {
    const { result } = renderHook(() => useShellSession());
    act(() => {
      result.current.revealDiscussion({
        discussionId: "t1",
        target: { kind: "draft", draftId: "d1" },
        ownerLabel: "Rewrite",
        subject: "s",
      });
    });
    expect(result.current.tabs.some((t) => t.id === "draft:d1")).toBe(true);
    expect(convTabs(result.current.tabs)).toHaveLength(0);
  });

  it("focuses the surface that already shows the discussion and opens nothing", () => {
    const { result } = renderHook(() => useShellSession());
    const focus = vi.fn();
    const held = registerSurface("t1", "rail", focus);
    const before = result.current.tabs.length;

    let outcome: string | undefined;
    act(() => {
      outcome = result.current.revealDiscussion(artifactReveal("t1"));
    });
    expect(outcome).toBe("focused");
    expect(focus).toHaveBeenCalledTimes(1);
    expect(result.current.tabs).toHaveLength(before);
    held.release();
  });

  it("a discussion in a conversation tab is focused there, never opened in its owner", () => {
    const { result } = renderHook(() => useShellSession());
    publishKnownArtifacts(["docs/other.md"]);
    act(() => {
      result.current.revealDiscussion(artifactReveal("t1"));
    });
    // The owner becomes available again. The tab stays until the author closes
    // it (TAB-FR-QXMV), so the route still focuses it.
    act(() => publishKnownArtifacts(["docs/a.md"]));
    act(() => result.current.activateTab("dashboard"));

    act(() => {
      result.current.revealDiscussion(artifactReveal("t1"));
    });
    expect(convTabs(result.current.tabs)).toHaveLength(1);
    expect(result.current.tabs.some((t) => t.id === "art:docs/a.md")).toBe(false);
    expect(result.current.activeTab).toBe(convTabs(result.current.tabs)[0].id);
    expect(surfaceCount("t1")).toBe(0);
  });
});

describe("TAB-FR-24, TAB-FR-25, TAB-FR-19, TAB-FR-15, CVP-FR-47: closing and path removal", () => {
  it("survives the removal of the file it is about, and closes without writing", async () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openArtifact({ id: "docs/a.md", name: "a.md" }));
    act(() => {
      result.current.revealDiscussion({ ...artifactReveal("t1"), ownerUnavailable: true });
    });
    const id = convTabs(result.current.tabs)[0].id;

    act(() => fireBus("project-tree-changed", { removedPaths: ["docs"] }));
    expect(result.current.tabs.some((t) => t.id === "art:docs/a.md")).toBe(false);
    expect(convTabs(result.current.tabs)).toHaveLength(1);

    invokeMock.mockClear();
    await act(async () => {
      await result.current.closeTab(id);
    });
    expect(convTabs(result.current.tabs)).toHaveLength(0);
    expect(
      invokeMock.mock.calls.filter(([c]) =>
        String(c).startsWith("save_") ||
        ["add_comment", "set_discussion_lock", "set_discussion_resolution"].includes(
          String(c),
        ),
      ),
    ).toHaveLength(0);
  });

  it("keeps the composer text and the unread state across a close and a reopen", async () => {
    const { result } = renderHook(() => useShellSession());
    act(() => {
      result.current.revealDiscussion(noteReveal("t1"));
    });
    act(() => setComposerBody("t1", "an unsent reply"));
    const id = convTabs(result.current.tabs)[0].id;

    await act(async () => {
      await result.current.closeTab(id);
    });
    expect(convTabs(result.current.tabs)).toHaveLength(0);
    expect(getDiscussionSession("t1").body).toBe("an unsent reply");

    act(() => {
      result.current.revealDiscussion(noteReveal("t1"));
    });
    expect(convTabs(result.current.tabs)).toHaveLength(1);
    expect(getDiscussionSession("t1").body).toBe("an unsent reply");
  });

  it("opens a Dashboard tab when closing it empties the strip (TAB-FR-15)", async () => {
    const { result } = renderHook(() => useShellSession());
    act(() => {
      result.current.revealDiscussion(noteReveal("t1"));
    });
    await act(async () => {
      await result.current.closeTab("dashboard");
    });
    expect(result.current.tabs).toHaveLength(1);

    await act(async () => {
      await result.current.closeTab(convTabs(result.current.tabs)[0].id);
    });
    expect(result.current.tabs.map((t) => t.id)).toEqual(["dashboard"]);
    expect(result.current.activeTab).toBe("dashboard");
  });
});

describe("NTS-FR-30, CVP-FR-61: deleting a note", () => {
  it("closes its tab and drops its session state, leaving other tabs alone", () => {
    const { result } = renderHook(() => useShellSession());
    act(() => {
      result.current.revealDiscussion(noteReveal("t1", "n1"));
      result.current.revealDiscussion(noteReveal("t2", "n2"));
    });
    act(() => setComposerBody("t1", "typed"));

    act(() => result.current.dropNoteConversationTab("n1"));

    const left = convTabs(result.current.tabs);
    expect(left).toHaveLength(1);
    expect(left[0].noteId).toBe("n2");
    expect(getDiscussionSession("t1").body).toBe("");
  });
});

describe("CVP-FR-49 / TAB-FR-14, TAB-FR-15: a worktree change tears every surface down", () => {
  it("closes the conversation tabs and clears sessions and registrations", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "activate_worktree")
        return { activeWorktreePath: "/w/b", branch: "b", isDetached: false };
      if (cmd === "list_worktrees_and_branches")
        return { worktrees: [], branches: [] };
      if (cmd === "get_active_worktree") return null;
      return undefined;
    });
    const { result } = renderHook(() => useShellSession());
    act(() => {
      result.current.revealDiscussion(noteReveal("t1"));
    });
    act(() => setComposerBody("t1", "typed"));
    registerSurface("t5", "rail", () => {});
    expect(convTabs(result.current.tabs)).toHaveLength(1);

    await act(async () => {
      await result.current.switchWorktree({
        path: "/w/b",
        branch: "b",
        isPrimary: false,
      } as never);
    });

    expect(convTabs(result.current.tabs)).toHaveLength(0);
    expect(result.current.tabs.map((t) => t.id)).toEqual(["dashboard"]);
    expect(getDiscussionSession("t1").body).toBe("");
    expect(surfaceCount("t5")).toBe(0);
    expect(consumePendingFocus("t1")).toBeNull();
    // CVP-FR-50: the teardown wrote to no discussion.
    expect(
      invokeMock.mock.calls.filter(([c]) =>
        ["add_comment", "set_discussion_lock", "set_discussion_resolution"].includes(
          String(c),
        ),
      ),
    ).toHaveLength(0);
  });
});
