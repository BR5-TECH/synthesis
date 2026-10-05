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
// CTA-FR-AZEU, CTA-FR-HBAB … TS-85: a turn that could not reach its model
// ---------------------------------------------------------------------------

/** A turn that failed recoverably and is its conversation's current offer. */
function failedTurn(over: Partial<AgentTurn> = {}): AgentTurn {
  return turn({
    id: "turn-1",
    nickname: "arch",
    state: "failed",
    failure: "unreachable",
    retryPermitted: true,
    endedAt: "2026-01-01T00:01:00Z",
    ...over,
  });
}

const card = () => screen.getByTestId("comment-thread-t1");
/** The comments actually in the thread — a failed contribution is not one. */
const postedComments = () =>
  card().querySelectorAll(
    ".comment:not(.comment--pending):not(.comment--failed)",
  );

describe("CTA-FR-HCMJ … CTA-FR-QUXJ: routing by the conversation's active agents", () => {
  /** A thread whose comments are exactly these bodies, human unless `agent`. */
  function conversation(
    lines: { body: string; agent?: string }[],
  ): Discussion {
    return makeThread({
      comments: lines.map((line, i) => ({
        id: `c${i + 1}`,
        author: line.agent
          ? ({ kind: "agent", agentId: `a-${line.agent}`, handle: line.agent } as const)
          : human("raver119"),
        body: line.body,
        quotes: [],
        attachments: [],
        createdAt: `2026-01-01T00:0${i}:00Z`,
      })),
    });
  }

  const dispatchedTo = (b: Backend) =>
    calls(b, "dispatch_agent_turn").map(
      (c) => (c.args as { nickname: string }).nickname,
    );

  const post = async (body: string) => {
    await userEvent.type(composer(), body);
    await userEvent.click(screen.getByRole("button", { name: "Post" }));
  };

  it("CTA-FR-HCMJ: an untagged follow-up reaches the agent the author last named, and nothing stored it", async () => {
    const b = await mount({
      threads: [
        conversation([
          { body: "@arch please review" },
          { body: "Two, and the second is smaller.", agent: "arch" },
          { body: "and the second one?" },
        ]),
      ],
    });

    const before = b.calls.length;
    await post("and the archive part?");
    await waitFor(() => expect(dispatchedTo(b)).toEqual(["arch"]));

    // CTA-FR-LCFU: no operation saved an active agent and no payload carried
    // one. Posting invoked exactly two things — the append and the dispatch —
    // and the set was read out of the comments each time it was wanted.
    const posting = b.calls
      .slice(before)
      // The session log's own batched flush is not part of the feature and
      // lands on its own timer (LGC-FR-05).
      .filter((c) => c.cmd !== "append_log_records");
    expect([...new Set(posting.map((c) => c.cmd))].sort()).toEqual([
      "add_comment",
      "dispatch_agent_turn",
    ]);
    expect(calls(b, "add_comment")[0].args).toEqual({
      artifactId: "a.md",
      discussionId: "t1",
      body: "and the archive part?",
      quotes: [],
      attachments: [],
    });
    expect(calls(b, "dispatch_agent_turn")[0].args).toEqual({
      nickname: "arch",
      // AGC-FR-05: the backend's shape, with the thread's own fragment target.
      origin: {
        discussionId: "t1",
        target: { kind: "artifact", artifactId: "a.md" },
        fragmentTarget: makeThread().fragmentTarget,
      },
      triggerCommentId: "c-4",
    });
  });

  it("CTA-FR-LCFU, CTA-FR-XBIN: a comment naming two agents reaches both, and so does every untagged one after it", async () => {
    const b = await mount({
      threads: [conversation([{ body: "Which session?" }])],
    });

    await post("@arch @sec please review");
    await waitFor(() => expect(dispatchedTo(b)).toEqual(["arch", "sec"]));

    await post("and the archive part?");
    await waitFor(() =>
      expect(dispatchedTo(b)).toEqual(["arch", "sec", "arch", "sec"]),
    );

    await post("and the ordering?");
    await waitFor(() =>
      expect(dispatchedTo(b)).toEqual([
        "arch",
        "sec",
        "arch",
        "sec",
        "arch",
        "sec",
      ]),
    );
  });

  it("CTA-FR-FAWE: a repeated mention asks once, and the appended body keeps both", async () => {
    const b = await mount({
      threads: [conversation([{ body: "@arch please review" }])],
    });

    await post("@arch @arch again please");
    await waitFor(() => expect(dispatchedTo(b)).toEqual(["arch"]));

    // CTA-FR-KREF: routing rewrote nothing — the tags stand in the stored body
    // exactly as they were typed, and that body is what the next derivation
    // reads.
    const appended = calls(b, "add_comment")[0].args as { body: string };
    expect(appended.body).toBe("@arch @arch again please");
    await waitFor(() =>
      expect(card().textContent).toContain("@arch @arch again please"),
    );

    // …and `@arch` is still the conversation's active agent.
    await post("well?");
    await waitFor(() => expect(dispatchedTo(b)).toEqual(["arch", "arch"]));
  });

  it("CTA-FR-FAWE: `@all @arch` dispatches once per agent rather than once per tag", async () => {
    const b = await mount({
      threads: [conversation([{ body: "@arch please review" }])],
    });

    await post("@all @arch again please");
    await waitFor(() => expect(dispatchedTo(b)).toEqual(["arch", "sec"]));
  });

  it("CTA-FR-BMHF, CTA-FR-IMKG: an unresolved mention reaches nobody by name and replaces nothing", async () => {
    const b = await mount({
      threads: [conversation([{ body: "@arch please review" }])],
    });

    await post("@nobody thoughts?");
    // CTA-FR-HCMJ: the comment carries no tag that resolves, so it reaches the
    // conversation's active agents — and `@nobody` names none of them.
    await waitFor(() => expect(dispatchedTo(b)).toEqual(["arch"]));
    expect(dispatchedTo(b)).not.toContain("nobody");
    // AGT-FR-28: the tag renders unemphasised, and the appended body stands.
    await waitFor(() => expect(card().textContent).toContain("@nobody thoughts?"));
    const marked = [...card().querySelectorAll("[data-testid='comment-tag']")].map(
      (el) => el.textContent,
    );
    expect(marked).not.toContain("@nobody");

    // The unresolved mention replaced nothing: the next untagged comment still
    // reaches `@arch`.
    await post("well?");
    await waitFor(() => expect(dispatchedTo(b)).toEqual(["arch", "arch"]));
  });

  it("CTA-FR-XTZC, CTA-FR-DWCK: a conversation whose only tag an agent wrote reaches nobody", async () => {
    const b = await mount({
      threads: [
        conversation([
          { body: "thinking aloud" },
          { body: "@sec should weigh in", agent: "arch" },
        ]),
      ],
    });

    // CTA-FR-IAKP: the empty composer says so before the post, names no agent
    // anywhere, and refuses nothing.
    expect(placeholder()).toBe("Reply…");
    expect(composer()).toBeEnabled();

    await post("what do we do then?");
    await waitFor(() => expect(calls(b, "add_comment")).toHaveLength(1));
    expect(dispatchedTo(b)).toEqual([]);
  });

  it("CTA-FR-RDSD, CMT-FR-11, AGT-FR-29, AGT-FR-37: the composer names the active agents and not the tag being typed", async () => {
    await mount({
      threads: [conversation([{ body: "@arch @sec please review" }])],
    });

    expect(placeholder()).toBe("Reply to @arch, @sec…");
    // CTA-FR-RDSD, CMT-FR-11, AGT-FR-29, AGT-FR-37: the moment the author types, the placeholder gives way to what
    // they typed — and the tag they wrote is not read back to them.
    await userEvent.type(composer(), "@sec ");
    expect(composer()).toHaveValue("@sec ");
    expect(placeholder()).toBe("Reply to @arch, @sec…");
    // …and it returns when the field is emptied again.
    await userEvent.clear(composer());
    expect(composer()).toHaveValue("");
    expect(placeholder()).toBe("Reply to @arch, @sec…");
  });

  it("CTA-FR-RDSD, CMT-FR-11, AGT-FR-29, AGT-FR-37: the posted body carries no part of the placeholder", async () => {
    // CTA-FR-RDSD / CMT-FR-11: the placeholder is presentation and nothing more.
    const b = await mount({
      threads: [conversation([{ body: "@arch @sec please review" }])],
    });
    expect(placeholder()).toBe("Reply to @arch, @sec…");

    await post("and the ordering?");
    await waitFor(() => expect(calls(b, "add_comment")).toHaveLength(1));
    expect(calls(b, "add_comment")[0].args).toMatchObject({
      body: "and the ordering?",
    });
  });

  it("CMT-FR-79, CTA-FR-MGVJ, CTA-FR-QDDG, AGC-FR-04: two agents answer independently, and Retry asks the one that failed", async () => {
    const b = await mount({
      threads: [conversation([{ body: "@arch @sec please review" }])],
    });

    await post("and the archive part?");
    await waitFor(() => expect(dispatchedTo(b)).toEqual(["arch", "sec"]));

    // `@arch` delivers; the card holds its answer as an ordinary comment.
    const withAnswer: Discussion = {
      ...b.threads[0],
      comments: [
        ...b.threads[0].comments,
        {
          id: "c-arch",
          author: { kind: "agent", agentId: "a-arch", handle: "arch" },
          body: "The archive part belongs in the same run.",
          quotes: [],
          attachments: [],
          createdAt: "2026-01-03T00:00:00Z",
        },
      ],
    };
    b.threads = [withAnswer];
    emitThread(withAnswer);
    emitTurn(
      turn({
        id: "turn-1",
        nickname: "arch",
        state: "delivered",
        endedAt: "2026-01-03T00:00:00Z",
      }),
    );

    // `@sec` fails recoverably; its failed contribution carries Retry.
    emitTurn(
      turn({
        id: "turn-2",
        nickname: "sec",
        state: "failed",
        failure: "unreachable",
        retryPermitted: true,
        endedAt: "2026-01-03T00:00:01Z",
      }),
    );

    const failed = await screen.findByTestId("comment-failed");
    expect(failed.textContent).toContain("sec");
    expect(card().textContent).toContain(
      "The archive part belongs in the same run.",
    );
    // Exactly two dispatches happened, and neither agent was asked twice.
    expect(dispatchedTo(b)).toEqual(["arch", "sec"]);

    await userEvent.click(screen.getByTestId("comment-failed-retry"));
    await waitFor(() => expect(calls(b, "retry_agent_turn")).toHaveLength(1));
    expect(calls(b, "retry_agent_turn")[0].args).toEqual({ turnId: "turn-2" });
    // Retry starts one turn for the one agent whose contribution carried it.
    expect(dispatchedTo(b)).toEqual(["arch", "sec"]);
  });

  it("CTA-FR-QUXJ / AGC-FR-23, AGC-FR-28, AGC-FR-29, CTA-FR-LCFU, CTA-FR-JQDM: the routing is identical before a relaunch and after one", async () => {
    const seeded = conversation([
      { body: "@arch please review" },
      { body: "Here is what I would do…", agent: "arch" },
      { body: "@sec @scribe now you two" },
      { body: "Noted.", agent: "sec" },
      { body: "Noted.", agent: "scribe" },
    ]);
    const b = await mount({
      threads: [seeded],
      agents: [
        projectAgent("arch", "a1"),
        projectAgent("sec", "a2"),
        projectAgent("scribe", "a3"),
      ],
      // AGC-FR-23, AGC-FR-28, AGC-FR-29, CTA-FR-LCFU, CTA-FR-QUXJ, CTA-FR-JQDM: `@arch` asked something before the relaunch. Nothing on disk
      // recorded it, and nothing about it adds a recipient (CTA-FR-DMNI).
      turns: [
        turn({
          id: "turn-old",
          nickname: "arch",
          state: "awaiting_reply",
          endedAt: "2026-01-02T00:00:00Z",
        }),
      ],
    });

    expect(placeholder()).toBe("Reply to @sec, @scribe…");
    await post("and the ordering?");
    await waitFor(() => expect(dispatchedTo(b)).toEqual(["sec", "scribe"]));
    const before = dispatchedTo(b);

    // The relaunch: the session is gone and `"list agent turns"` returns none,
    // but the comments the set is read from survive it.
    cleanup();
    b.calls.length = 0;
    b.turns = [];
    await remount(b);

    expect(placeholder()).toBe("Reply to @sec, @scribe…");
    await post("and after that?");
    await waitFor(() => expect(dispatchedTo(b)).toEqual(["sec", "scribe"]));
    expect(dispatchedTo(b)).toEqual(before);
  });
});
