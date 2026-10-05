// Addressing an agent from the comment rail — `specifications/ui/CMT-comments.md`
// CTA-FR-SKXK, CMT-FR-15, AGT-FR-25, AGT-FR-27 … CMT-FR-37, CTA-FR-RPVU, AGC-FR-07.
//
// Driven through the real Editor, so what is exercised is the wiring: which
// operation a post invokes and in what order, what a card renders while an
// agent is still thinking, and what it renders when the answer lands or does
// not. Whether the agent is *reachable* belongs to `AGC-agent-conversations.md`
// and is covered by its own Rust tests.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { Editor } from "./Editor";
import { EditSessionStore } from "../state/editSessions";
import type {
  AgentTurn,
  AiApiCatalog,
  Discussion,
  ProjectAgent,
} from "../types";
import {
  BODY,
  addressedThread,
  calls,
  human,
  makeThread,
  projectAgent,
  turn,
} from "../test/editorAgentsFixtures";
import type { Backend } from "../test/editorAgentsFixtures";
import {
  artifactCommentOrigin,
  expectBackendOrigin,
} from "../test/origins";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

type Handler = (event: { payload: unknown }) => void;
const listeners = new Map<string, Handler[]>();
vi.mock("@tauri-apps/api/event", () => ({
  listen: async (name: string, cb: Handler) => {
    listeners.set(name, [...(listeners.get(name) ?? []), cb]);
    return () => {
      listeners.set(name, (listeners.get(name) ?? []).filter((h) => h !== cb));
    };
  },
}));

/**
 * CMS-FR-51: a thread the backend has just written into. An agent's answer
 * arrives on this channel like any other append — the turn event says the turn
 * ended, this one carries the comment (CMT-FR-04).
 */
function emitThread(thread: Discussion) {
  act(() => {
    for (const handler of listeners.get("discussion-changed") ?? []) {
      handler({ payload: thread });
    }
  });
}

function emitTurn(turn: AgentTurn) {
  act(() => {
    for (const handler of listeners.get("agent-turn-state-changed") ?? []) {
      handler({ payload: turn });
    }
  });
}

function wire(b: Backend) {
  let dispatched = 0;
  invokeMock.mockImplementation(async (cmd: string, args: unknown) => {
    b.calls.push({ cmd, args });
    switch (cmd) {
      case "load_artifact_contents_by_id":
        return { body: BODY, checksum: "ck1" };
      case "save_artifact_contents":
        return { checksum: "ck2" };
      case "list_discussions":
        return b.threads;
      case "resolve_comment_author_identity":
        return human("raver119");
      case "list_project_agents":
        return b.agents;
      case "list_agent_turns":
        return b.turns;
      case "list_recoverable_agent_turn_failures":
        return b.recoverable ?? [];
      case "list_agent_turn_image_notices":
        return b.imageNotices ?? [];
      case "retry_agent_turn": {
        if (b.retryGate) await b.retryGate;
        if (b.retryError) throw b.retryError;
        const { turnId } = args as { turnId: string };
        const failed = (b.recoverable ?? []).find((t) => t.id === turnId);
        dispatched += 1;
        return turn({
          id: `turn-retry-${dispatched}`,
          nickname: failed?.nickname ?? "arch",
          origin: failed?.origin ?? artifactCommentOrigin("t1"),
          triggerCommentId: failed?.triggerCommentId ?? "c1",
        });
      }
      case "list_ai_api_catalogs":
        return b.integrations;
      case "get_active_ai_api_catalog": {
        const catalog = b.integrations.find((i) => i.state === "verified") ?? null;
        return { resolution: catalog ? "inherited" : "none_configured", catalog };
      }
      case "add_comment": {
        const { discussionId: threadId, body } = args as { discussionId: string; body: string };
        const target = b.threads.find((t) => t.id === threadId)!;
        const updated: Discussion = {
          ...target,
          comments: [
            ...target.comments,
            {
              id: `c-${target.comments.length + 1}`,
              author: human("raver119"),
              body,
              quotes: [],
              attachments: [],
              createdAt: "2026-01-02T00:00:00Z",
            },
          ],
        };
        b.threads = b.threads.map((t) => (t.id === threadId ? updated : t));
        return updated;
      }
      case "dispatch_agent_turn": {
        if (b.dispatchError) throw b.dispatchError;
        const { nickname, origin, triggerCommentId } = args as {
          nickname: string;
          origin: unknown;
          triggerCommentId: string;
        };
        dispatched += 1;
        return turn({
          id: `turn-${dispatched}`,
          nickname,
          origin: expectBackendOrigin(origin),
          triggerCommentId,
        });
      }
      case "cancel_agent_turn":
        return null;
      case "set_discussion_lock": {
        const { discussionId: threadId, locked } = args as { discussionId: string; locked: boolean };
        const updated = { ...b.threads.find((t) => t.id === threadId)!, locked };
        b.threads = b.threads.map((t) => (t.id === threadId ? updated : t));
        return updated;
      }
      default:
        return null;
    }
  });
}

beforeEach(() => {
  invokeMock.mockReset();
  listeners.clear();
});
afterEach(cleanup);

async function mount(over: Partial<Backend> = {}) {
  const b: Backend = {
    threads: [makeThread()],
    agents: [projectAgent("arch", "a1"), projectAgent("sec", "a2")],
    integrations: [
      {
        provider: "openrouter",
        displayName: "OpenRouter",
        state: "verified",
        models: [{ id: "anthropic/claude-opus-5", label: "Claude Opus 5" }],
      },
    ],
    turns: [],
    calls: [],
    ...over,
  };
  wire(b);
  render(
    <Editor
      artifactId="a.md"
      artifactName="a.md"
      artifactType="skill"
      sessions={new EditSessionStore()}
    />,
  );
  await screen.findByLabelText("artifact body");
  const toggle = screen.getByRole("button", {
    name: /comments \(\d+ unresolved\)/i,
  });
  if (toggle.getAttribute("aria-expanded") !== "true") {
    await userEvent.click(toggle);
  }
  await screen.findByRole("complementary", { name: "Comments" });
  return b;
}

/**
 * Mount the Editor again over the **same** wired backend, so what a remounted
 * card reads is counted against what the first mount read (CTA-FR-STWA).
 *
 * A fresh `EditSessionStore` because a relaunch is what this stands in for: the
 * conversation is what survives, and everything the session held does not.
 */
async function remount(b: Backend) {
  render(
    <Editor
      artifactId="a.md"
      artifactName="a.md"
      artifactType="skill"
      sessions={new EditSessionStore()}
    />,
  );
  await screen.findByLabelText("artifact body");
  const toggle = screen.getByRole("button", {
    name: /comments \(\d+ unresolved\)/i,
  });
  if (toggle.getAttribute("aria-expanded") !== "true") {
    await userEvent.click(toggle);
  }
  await screen.findByRole("complementary", { name: "Comments" });
  return b;
}

const composer = () => screen.getByLabelText("Reply to thread t1");
/**
 * CTA-FR-IAKP: what the composer's placeholder names — the one place a
 * conversation states who an untagged reply reaches.
 */
const placeholder = () => composer().getAttribute("placeholder");

// ---------------------------------------------------------------------------
// CTA-FR-IEPG: an agent that asked the author something
// ---------------------------------------------------------------------------

describe("CTA-FR-LCVQ, CTA-FR-ELIJ: a turn that ends by asking a question", () => {
  it("renders no pending contribution once the question is posted", async () => {
    // CTA-FR-LCVQ: the question arrived as an ordinary comment through the same
    // event any comment arrives by, so the placeholder is replaced rather than
    // joined — and nothing in the card says a reply is owed.
    await mount();
    await userEvent.type(composer(), "@arch thoughts?");
    await userEvent.click(screen.getByRole("button", { name: "Post" }));
    // The first dispatch of this mount, so `turn-1`.
    await waitFor(() =>
      expect(screen.getByTestId("comment-pending")).toHaveTextContent("Thinking…"),
    );

    emitTurn(
      turn({
        id: "turn-1",
        nickname: "arch",
        state: "awaiting_reply",
        endedAt: "2026-01-03T00:00:00Z",
      }),
    );

    await waitFor(() =>
      expect(screen.queryByTestId("comment-pending")).not.toBeInTheDocument(),
    );
    // No failure is rendered either: asking is not a failure (CVL-FR-15).
    expect(screen.queryByTestId("comment-turn-error")).not.toBeInTheDocument();
  });

  it("CTA-FR-HCMJ: dispatches to the agent that asked, it being the active agent", async () => {
    // CTA-FR-LCFU / CTA-FR-HCMJ: the author answers in their own words and the agent
    // that asked hears them — because the newest human comment naming anybody
    // reads `@arch which session?`, not because a wait was remembered.
    const b = await mount({ threads: [addressedThread()] });
    emitTurn(
      turn({
        id: "turn-7",
        nickname: "arch",
        state: "awaiting_reply",
        endedAt: "2026-01-03T00:00:00Z",
      }),
    );

    await userEvent.type(composer(), "two, definitely");
    await userEvent.click(screen.getByRole("button", { name: "Post" }));

    await waitFor(() => expect(calls(b, "add_comment")).toHaveLength(1));
    await waitFor(() =>
      expect(calls(b, "dispatch_agent_turn")).toHaveLength(1),
    );
    expect(calls(b, "dispatch_agent_turn")[0].args).toMatchObject({
      nickname: "arch",
    });
  });

  it("CTA-FR-HCMJ: dispatches for the comment after that one too", async () => {
    // The active set is what the comments say rather than anything the wait left
    // behind, so a second untagged comment reaches `@arch` again.
    const b = await mount({ threads: [addressedThread()] });
    emitTurn(
      turn({
        id: "turn-7",
        nickname: "arch",
        state: "awaiting_reply",
        endedAt: "2026-01-03T00:00:00Z",
      }),
    );

    await userEvent.type(composer(), "two, definitely");
    await userEvent.click(screen.getByRole("button", { name: "Post" }));
    await waitFor(() =>
      expect(calls(b, "dispatch_agent_turn")).toHaveLength(1),
    );

    await userEvent.type(composer(), "and the archive part?");
    await userEvent.click(screen.getByRole("button", { name: "Post" }));
    await waitFor(() => expect(calls(b, "add_comment")).toHaveLength(2));
    await waitFor(() =>
      expect(calls(b, "dispatch_agent_turn")).toHaveLength(2),
    );
    expect(
      calls(b, "dispatch_agent_turn").map(
        (c) => (c.args as { nickname: string }).nickname,
      ),
    ).toEqual(["arch", "arch"]);
  });

  it("dispatches to nobody in a conversation the author has addressed nobody in", async () => {
    // CTA-FR-WYID: no human comment names anybody, so there is no active agent —
    // and an agent that merely answered makes no claim on the next comment.
    const b = await mount();
    emitTurn(
      turn({
        id: "turn-8",
        nickname: "arch",
        state: "delivered",
        endedAt: "2026-01-03T00:00:00Z",
      }),
    );

    await userEvent.type(composer(), "and the archive part?");
    await userEvent.click(screen.getByRole("button", { name: "Post" }));

    await waitFor(() => expect(calls(b, "add_comment")).toHaveLength(1));
    expect(calls(b, "dispatch_agent_turn")).toHaveLength(0);
  });

  it("CTA-FR-JQDM, CTA-FR-DMNI, CTA-FR-IEPG: dispatches to the tagged agent alone, an awaiting turn adding no recipient", async () => {
    // CTA-FR-DMNI: an author who steers their next message at somebody else has
    // turned to them, and the agent that asked is not dispatched to merely for
    // having asked.
    const b = await mount({ threads: [addressedThread()] });
    emitTurn(
      turn({
        id: "turn-9",
        nickname: "arch",
        state: "awaiting_reply",
        endedAt: "2026-01-03T00:00:00Z",
      }),
    );

    await userEvent.type(composer(), "@sec what do you think?");
    await userEvent.click(screen.getByRole("button", { name: "Post" }));

    await waitFor(() =>
      expect(calls(b, "dispatch_agent_turn")).toHaveLength(1),
    );
    expect(calls(b, "dispatch_agent_turn")[0].args).toMatchObject({
      nickname: "sec",
    });

    // And the mention REPLACED the set: the untagged comment after it reaches
    // `@sec` alone and still not `@arch` (CTA-FR-QUXJ).
    emitThread(
      addressedThread({
        comments: [
          ...addressedThread().comments,
          {
            id: "c2",
            author: human("raver119"),
            body: "@sec what do you think?",
            quotes: [],
            attachments: [],
            createdAt: "2026-01-03T00:01:00Z",
          },
        ],
      }),
    );
    await userEvent.type(composer(), "anything?");
    await userEvent.click(screen.getByRole("button", { name: "Post" }));
    await waitFor(() =>
      expect(calls(b, "dispatch_agent_turn")).toHaveLength(2),
    );
    expect(calls(b, "dispatch_agent_turn")[1].args).toMatchObject({
      nickname: "sec",
    });
  });

  it("CTA-FR-KVIF, CTA-FR-ELIJ: renders no pending contribution for a turn already awaiting on mount, and it contributes no recipient", async () => {
    // The seeding read does not filter by state — the turn is still outstanding
    // and `"list agent turns"` still returns it — but it draws nothing and it
    // obliges no dispatch.
    const b = await mount({
      turns: [
        turn({
          id: "turn-9",
          nickname: "arch",
          state: "awaiting_reply",
          endedAt: "2026-01-03T00:00:00Z",
        }),
      ],
    });
    await waitFor(() => expect(calls(b, "list_agent_turns")).toHaveLength(1));
    expect(screen.queryByTestId("comment-pending")).not.toBeInTheDocument();

    // The thread's own comments name nobody, so the awaiting turn is the only
    // thing that could have produced a recipient. It produces none.
    await userEvent.type(composer(), "two, definitely");
    await userEvent.click(screen.getByRole("button", { name: "Post" }));
    await waitFor(() => expect(calls(b, "add_comment")).toHaveLength(1));
    expect(calls(b, "dispatch_agent_turn")).toHaveLength(0);
  });

  it("dispatches nothing for an agent withdrawn while it waited, and everything for the active one", async () => {
    // AGC-FR-28 discards the registration without announcing it, so the rail can
    // still hold the turn. `@ghost` is no longer enrolled and was never named,
    // so the reply reaches `@arch` alone — the conversation's active agent — and
    // the held turn adds nobody (CTA-FR-DMNI, per AGT-FR-28).
    const b = await mount({ threads: [addressedThread()] });
    emitTurn(
      turn({
        id: "turn-11",
        nickname: "ghost",
        state: "awaiting_reply",
        endedAt: "2026-01-03T00:00:00Z",
      }),
    );

    await userEvent.type(composer(), "two, definitely");
    await userEvent.click(screen.getByRole("button", { name: "Post" }));
    await waitFor(() => expect(calls(b, "add_comment")).toHaveLength(1));
    await waitFor(() =>
      expect(calls(b, "dispatch_agent_turn")).toHaveLength(1),
    );
    expect(calls(b, "dispatch_agent_turn")[0].args).toMatchObject({
      nickname: "arch",
    });
  });
});
