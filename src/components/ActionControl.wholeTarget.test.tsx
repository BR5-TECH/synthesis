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
import userEvent from "@testing-library/user-event";

import { ActionControl } from "./ActionControl";
import { DiffView } from "./DiffView";
import { FlowCanvas } from "./Flow";
import type { UseDiscussionsResult } from "../hooks/useDiscussions";
import { EditSessionStore } from "../state/editSessions";
import { FlowSessionStore } from "../state/flowSessions";
import {
  focusDiscussion,
  resetDiscussionFocus,
  surfaceCount,
  surfaceOwner,
} from "../state/discussionFocus";
import {
  clearAllDiscussionSessions,
  mergeDiscussionTurn,
} from "../state/discussionSession";
import type { AgentTurn, Discussion, DiffTarget } from "../types";
import {
  artifactDiscussionOrigin,
} from "../test/origins";

/**
 * Whole-target discussions opened from the Flow, Diff, and History owners and
 * from an artifact's action control (`ACT-action-control.md` ACT-FR-16,
 * ACT-FR-20, ACT-FR-28; `CVP-conversation-presentation.md` CVP-FR-02,
 * CVP-FR-47).
 */
const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
  emit: vi.fn(async () => {}),
}));

const human = { kind: "human", login: "raver119" } as const;
const FLOW_BODY = JSON.stringify({
  version: 1,
  name: "Onboarding review",
  nodes: [],
  edges: [],
});

function discussion(over: Partial<Discussion> = {}): Discussion {
  return {
    id: "d1",
    target: { kind: "artifact", artifactId: "review.flow" },
    fragmentTarget: null,
    comments: [
      {
        id: "c1",
        author: human,
        body: "is this ready?",
        quotes: [],
        attachments: [],
        createdAt: "2026-01-01T00:00:00Z",
      },
    ],
    locked: false,
    resolved: false,
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
    ...over,
  };
}

interface Backend {
  discussions: Discussion[];
  opened: { cmd: string; args: unknown }[];
}

function wire(b: Backend, flowBody = FLOW_BODY) {
  invokeMock.mockImplementation(async (cmd: string, args: unknown) => {
    switch (cmd) {
      case "load_artifact_contents_by_id":
        return { body: flowBody, checksum: "ck1" };
      case "list_discussions":
        return b.discussions;
      case "list_project_agents":
        return [];
      case "get_active_ai_api_catalog":
        return { resolution: "none_configured", catalog: null };
      case "list_ai_api_catalogs":
      case "list_agent_turns":
        return [];
      case "resolve_comment_author_identity":
        return human;
      case "open_discussion": {
        b.opened.push({ cmd, args });
        const created = discussion({ id: "d-new" });
        b.discussions = [...b.discussions, created];
        return created;
      }
      default:
        return undefined;
    }
  });
}

async function renderFlow(b: Backend) {
  wire(b);
  const flows = new FlowSessionStore();
  await act(async () => {
    flows.openTab("review.flow");
    await flows.get("review.flow")?.pendingLoad;
  });
  render(<FlowCanvas flowId="review.flow" flows={flows} />);
  await screen.findByRole("button", { name: "Actions" });
}

async function renderDiff(b: Backend) {
  wire(b);
  const target: DiffTarget = {
    path: "review.flow",
    name: "review.flow",
    scope: { kind: "path", path: "review.flow" },
    comparisonLabel: "Uncommitted",
  };
  render(<DiffView target={target} sessions={new EditSessionStore()} />);
  await screen.findByRole("button", { name: "Actions" });
}

async function discuss(noun: string, text: string) {
  await userEvent.click(screen.getByRole("button", { name: "Actions" }));
  await userEvent.click(screen.getByRole("menuitem", { name: /Discuss/ }));
  const field = await screen.findByRole("textbox", {
    name: `Discuss this ${noun}`,
  });
  await userEvent.type(field, text);
  return field;
}

beforeEach(() => {
  invokeMock.mockReset();
  clearAllDiscussionSessions();
  resetDiscussionFocus();
});
afterEach(cleanup);

const entryPoints: {
  name: string;
  noun: string;
  mount: (b: Backend) => Promise<void>;
}[] = [
  { name: "Flow", noun: "flow", mount: renderFlow },
  { name: "Diff", noun: "file", mount: renderDiff },
];

describe.each(entryPoints)("$name opens a whole-target discussion", (entry) => {
  it("ACT-FR-16, ACT-FR-28, CVP-FR-40, CMS-FR-57: Ctrl+Enter in the shared composer opens it about the whole item, never a fragment", async () => {
    const b: Backend = { discussions: [], opened: [] };
    await entry.mount(b);
    const field = await discuss(entry.noun, "is this ready?");

    fireEvent.keyDown(field, { key: "Enter", ctrlKey: true });
    await waitFor(() => expect(b.opened).toHaveLength(1));
    const request = b.opened[0].args as {
      target: unknown;
      fragmentTarget?: unknown;
      body: string;
    };
    expect(request.target).toEqual({
      kind: "artifact",
      artifactId: "review.flow",
    });
    expect(request.fragmentTarget ?? null).toBeNull();
    expect(request.body).toBe("is this ready?");

    // CVP-FR-02: the owner panel shows it through the one shared surface.
    const panel = await screen.findByRole("dialog", { name: "Discussion" });
    expect(
      await within(panel).findByTestId("comment-thread-d-new"),
    ).toHaveAttribute("data-target", "whole");
    expect(surfaceOwner("d-new")).toBe("panel");
    expect(
      screen.queryByRole("textbox", { name: `Discuss this ${entry.noun}` }),
    ).not.toBeInTheDocument();
  });

  it("CVP-FR-02, CVP-FR-06, ACT-FR-20: reopening focuses the one panel that exists and never mounts a second", async () => {
    const b: Backend = { discussions: [discussion()], opened: [] };
    await entry.mount(b);
    await userEvent.click(
      await screen.findByRole("button", { name: "Show discussion (1)" }),
    );
    await screen.findByRole("dialog", { name: "Discussion" });
    expect(surfaceCount("d1")).toBe(1);

    // A reveal route finds the surface that already shows it.
    expect(focusDiscussion("d1", "composer")).toBe("focused");
    expect(screen.getByLabelText("Reply to thread d1")).toHaveFocus();
    expect(screen.getAllByRole("dialog", { name: "Discussion" })).toHaveLength(1);
    expect(surfaceCount("d1")).toBe(1);

    // Hiding and showing again re-parents the same discussion.
    await userEvent.click(
      screen.getByRole("button", { name: "Hide discussion" }),
    );
    await waitFor(() => expect(surfaceCount("d1")).toBe(0));
    expect(focusDiscussion("d1")).toBe("closed");
    await userEvent.click(
      screen.getByRole("button", { name: "Show discussion (1)" }),
    );
    await screen.findByTestId("comment-thread-d1");
    expect(surfaceCount("d1")).toBe(1);
  });

  it("CVP-FR-47, ACT-FR-25: closing the panel keeps the reply text and the pending agent placeholder", async () => {
    const b: Backend = { discussions: [discussion()], opened: [] };
    await entry.mount(b);
    const turn: AgentTurn = {
      id: "t1",
      agentId: "a1",
      nickname: "arch",
      state: "running",
      origin: artifactDiscussionOrigin("d1", "review.flow"),
    } as unknown as AgentTurn;
    act(() => mergeDiscussionTurn("d1", turn));

    await userEvent.click(
      await screen.findByRole("button", { name: "Show discussion (1)" }),
    );
    const reply = await screen.findByLabelText("Reply to thread d1");
    await userEvent.type(reply, "half a reply");
    expect(screen.getByTestId("comment-pending")).toBeInTheDocument();

    await userEvent.click(
      screen.getByRole("button", { name: "Hide discussion" }),
    );
    await waitFor(() => expect(surfaceCount("d1")).toBe(0));

    await userEvent.click(
      screen.getByRole("button", { name: "Show discussion (1)" }),
    );
    expect(await screen.findByLabelText("Reply to thread d1")).toHaveValue(
      "half a reply",
    );
    expect(screen.getByTestId("comment-pending")).toHaveTextContent("arch");
  });
});

describe("the History detail owner and an unavailable owner", () => {
  function stub(): UseDiscussionsResult {
    const thread = discussion({
      target: { kind: "artifact", artifactId: "notes.md" },
    });
    return {
      target: thread.target,
      discussions: [{ thread, anchor: null }],
      agents: [],
      pendingTurns: [],
      turnFailures: {},
      failedTurns: {},
      imageNotices: {},
      retryingTurnIds: new Set(),
      retryTurn: async () => {},
      cancelTurn: async () => {},
      identity: human,
      identityError: null,
      retryIdentity: async () => {},
      unresolved: 1,
      errors: {},
      open: vi.fn(async () => discussion({ id: "d-new" })),
      reply: async () => {},
      setLock: async () => {},
      setResolved: async () => {},
    } as UseDiscussionsResult;
  }

  it("HVW-FR-10, ACT-FR-16, ACT-FR-28: a read-only History tab opens a whole-target discussion through the shared composer", async () => {
    invokeMock.mockImplementation(async () => []);
    const discussions = stub();
    render(
      <ActionControl discussions={discussions} readOnly itemNoun="file" />,
    );
    await userEvent.click(screen.getByRole("button", { name: "Actions" }));
    const menu = screen.getByRole("menu");
    expect(
      within(menu)
        .getAllByRole("menuitem")
        .map((el) => el.textContent?.trim()),
    ).toEqual(["Discuss"]);
    await userEvent.click(screen.getByRole("menuitem", { name: /Discuss/ }));
    const field = await screen.findByRole("textbox", {
      name: "Discuss this file",
    });
    await userEvent.type(field, "why was this changed?");
    fireEvent.keyDown(field, { key: "Enter", metaKey: true });
    await waitFor(() => expect(discussions.open).toHaveBeenCalledTimes(1));
    expect(discussions.open).toHaveBeenCalledWith(
      "why was this changed?",
      [],
    );
    expect(surfaceCount("d-new")).toBe(0);
  });

  it("CVP-FR-45, ACT-FR-07: an owner that no longer resolves shows the unavailable state in the panel", async () => {
    invokeMock.mockImplementation(async () => []);
    render(
      <ActionControl
        discussions={stub()}
        readOnly
        missing
        ownerLabel="notes.md"
      />,
    );
    await userEvent.click(
      screen.getByRole("button", { name: "Show discussion (1)" }),
    );
    const panel = await screen.findByRole("dialog", { name: "Discussion" });
    expect(
      within(panel).getByTestId("discussion-owner-unavailable"),
    ).toHaveTextContent("Owner unavailable: notes.md no longer exists");
    expect(screen.getByTestId("comment-thread-d1")).toHaveAttribute(
      "data-availability",
      "unavailable",
    );
  });
});
