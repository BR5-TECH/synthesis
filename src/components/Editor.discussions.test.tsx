// An artifact's **discussions**, read and continued from the Editor's rail —
// `specifications/ui/ACT-action-control.md` ACT-FR-19 / ACT-FR-22 and
// `specifications/ui/CMT-comments.md` CTA-FR-ZOLW, -43, -56, -58, -60, -61.
//
// The rail is one surface over two conversations: the Discussion section at its
// head is the action control's, the aligned cards below it are `useComments`'s,
// and the two obey different rules. Everything here is about the seam — which
// hook a card's channel reaches, and what the tab counts — because that is what
// a single shared rail gets wrong, and it gets it wrong silently: an untagged
// follow-up simply goes unanswered and a turn simply never shows.
//
// Driven through the real Editor for that reason. A hook test would have both
// hooks behaving correctly in isolation while the tab wired the wrong one in.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { Editor } from "./Editor";
import { EditSessionStore } from "../state/editSessions";
import { followAgentTurn } from "../state/discussionSession";
import { onAgentTurnStateChanged } from "../events";
import type {
  ConversationOrigin,
  AgentTurn,
  AiApiCatalog,
  Discussion,
  ProjectAgent,
} from "../types";
import {
  artifactDiscussionOrigin,
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

function emitTurn(turn: AgentTurn) {
  act(() => {
    for (const handler of listeners.get("agent-turn-state-changed") ?? []) {
      handler({ payload: turn });
    }
  });
}

const BODY = "# Comments\n\nSteps to run before the first session.\n";
const QUOTE = "the first session";

const human = (login: string) => ({ kind: "human", login }) as const;
const bot = (nickname: string) =>
  ({ kind: "agent", handle: nickname, agentId: `agent-${nickname}` }) as const;

/**
 * CMT-FR-55: a discussion aligns to nothing, which is what `anchor: null` says
 * here and what keeps it out of the aligned layer.
 */
function discussion(over: Partial<Discussion> = {}): Discussion {
  return {
    id: "d1",
    target: { kind: "artifact", artifactId: "a.md" },
    fragmentTarget: null,
    comments: [
      {
        id: "dc1",
        author: human("raver119"),
        body: "@arch what's your take on this spec?",
        quotes: [],
        attachments: [],
        createdAt: "2026-01-01T00:00:00Z",
      },
      {
        id: "dc2",
        author: bot("arch"),
        body: "It covers most interaction paths well.",
        quotes: [],
        attachments: [],
        createdAt: "2026-01-01T00:01:00Z",
      },
    ],
    locked: false,
    resolved: false,
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:01:00Z",
    ...over,
  };
}

function anchored(over: Partial<Discussion> = {}): Discussion {
  return {
    id: "t1",
    target: { kind: "artifact", artifactId: "a.md" },
    fragmentTarget: {
      owner: { kind: "artifact", artifactId: "a.md" },
      path: "a.md",
      ...{
      start: BODY.indexOf(QUOTE),
      end: BODY.indexOf(QUOTE) + QUOTE.length,
      quote: QUOTE,
    },
    },
    comments: [
      {
        id: "c1",
        author: human("raver119"),
        body: "@arch which session?",
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

function projectAgent(nickname: string, id: string): ProjectAgent {
  return {
    availability: "ready",
    agent: {
      id,
      nickname,
      title: "",
      modelId: "anthropic/claude-opus-5",
      instructions: "",
      createdAt: "2026-01-01T00:00:00Z",
      updatedAt: "2026-01-01T00:00:00Z",
      reasoning: null,
    },
  };
}

interface DispatchCall {
  nickname: string;
  origin: ConversationOrigin;
}

interface Backend {
  anchoredThreads: Discussion[];
  discussions: Discussion[];
  agents: ProjectAgent[];
  integrations: AiApiCatalog[];
  turns: AgentTurn[];
  dispatches: DispatchCall[];
  calls: { cmd: string; args: unknown }[];
  addCommentError?: string;
  /** AGC-FR-31: what `list_recoverable_agent_turn_failures` returns. */
  recoverable?: AgentTurn[];
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
      // One operation lists the fragment and the whole-target discussions.
      case "list_discussions":
        return [...b.anchoredThreads, ...b.discussions];
      case "resolve_comment_author_identity":
        return human("raver119");
      case "list_project_agents":
        return b.agents;
      case "list_ai_api_catalogs":
        return b.integrations;
      case "get_active_ai_api_catalog": {
        const catalog = b.integrations.find((i) => i.state === "verified") ?? null;
        return { resolution: catalog ? "inherited" : "none_configured", catalog };
      }
      case "list_agent_turns":
        return b.turns;
      case "list_recoverable_agent_turn_failures":
        return b.recoverable ?? [];
      case "retry_agent_turn": {
        // AGC-FR-32: a new turn, and the offer consumed by being taken.
        const { turnId } = args as { turnId: string };
        const failed = (b.recoverable ?? []).find((t) => t.id === turnId);
        if (!failed) throw "turn_not_found";
        b.recoverable = (b.recoverable ?? []).filter((t) => t.id !== turnId);
        dispatched += 1;
        return {
          ...failed,
          id: `turn-retry-${dispatched}`,
          state: "running",
          failure: null,
          retryPermitted: false,
          endedAt: null,
        };
      }
      case "add_comment": {
        // CMS-FR-55: the locator names the *file*, so the backend settles which
        // of its two logs holds the thread by looking. The mock does the same,
        // which is why a reply misrouted by the UI still lands — and why the
        // defect this file covers was invisible from the write alone.
        if (b.addCommentError) throw b.addCommentError;
        const { discussionId: threadId, body } = args as { discussionId: string; body: string };
        const append = (t: Discussion): Discussion => ({
          ...t,
          comments: [
            ...t.comments,
            {
              id: `c-${t.comments.length + 1}`,
              author: human("raver119"),
              body,
              quotes: [],
              attachments: [],
              createdAt: "2026-01-02T00:00:00Z",
            },
          ],
        });
        const inDiscussions = b.discussions.find((t) => t.id === threadId);
        if (inDiscussions) {
          const updated = append(inDiscussions);
          b.discussions = b.discussions.map((t) =>
            t.id === threadId ? updated : t,
          );
          return updated;
        }
        const target = b.anchoredThreads.find((t) => t.id === threadId)!;
        const updated = append(target);
        b.anchoredThreads = b.anchoredThreads.map((t) =>
          t.id === threadId ? updated : t,
        );
        return updated;
      }
      case "set_discussion_resolution": {
        const { discussionId: threadId, resolved } = args as {
          discussionId: string;
          resolved: boolean;
        };
        const inDiscussions = b.discussions.find((t) => t.id === threadId);
        if (inDiscussions) {
          const updated = { ...inDiscussions, resolved };
          b.discussions = b.discussions.map((t) =>
            t.id === threadId ? updated : t,
          );
          return updated;
        }
        const updated = {
          ...b.anchoredThreads.find((t) => t.id === threadId)!,
          resolved,
        };
        b.anchoredThreads = b.anchoredThreads.map((t) =>
          t.id === threadId ? updated : t,
        );
        return updated;
      }
      case "dispatch_agent_turn": {
        const { nickname, origin, triggerCommentId } = args as {
          nickname: string;
          origin: unknown;
          triggerCommentId: string;
        };
        b.dispatches.push({ nickname, origin: expectBackendOrigin(origin) });
        dispatched += 1;
        return {
          id: `turn-${dispatched}`,
          agentId: `agent-${nickname}`,
          nickname,
          origin,
          triggerCommentId,
          state: "running",
          failure: null,
          startedAt: "2026-01-02T00:00:00Z",
          endedAt: null,
        } as AgentTurn;
      }
      case "set_discussion_lock": {
        const { discussionId: threadId, locked } = args as {
          discussionId: string;
          locked: boolean;
        };
        const inDiscussions = b.discussions.find((t) => t.id === threadId);
        if (inDiscussions) {
          const updated = { ...inDiscussions, locked };
          b.discussions = b.discussions.map((t) =>
            t.id === threadId ? updated : t,
          );
          return updated;
        }
        const updated = {
          ...b.anchoredThreads.find((t) => t.id === threadId)!,
          locked,
        };
        b.anchoredThreads = b.anchoredThreads.map((t) =>
          t.id === threadId ? updated : t,
        );
        return updated;
      }
      case "cancel_agent_turn":
        return null;
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
    anchoredThreads: [],
    discussions: [discussion()],
    agents: [projectAgent("arch", "a1"), projectAgent("sec", "a2")],
    integrations: [],
    turns: [],
    dispatches: [],
    calls: [],
    ...over,
  };
  wire(b);
  render(
    <Editor
      artifactId="a.md"
      artifactName="a.md"
      artifactType="spec"
      sessions={new EditSessionStore()}
    />,
  );
  await screen.findByLabelText("artifact body");
  const toggle = await screen.findByRole("button", {
    name: /comments \(\d+ unresolved\)/i,
  });
  if (toggle.getAttribute("aria-expanded") !== "true") {
    await userEvent.click(toggle);
  }
  await screen.findByRole("complementary", { name: "Comments" });
  await screen.findByTestId("discussion-section");
  return b;
}

const counter = () =>
  screen.getByRole("button", { name: /comments \(\d+ unresolved\)/i });

/** A turn of this artifact's discussion `d1`, as the backend would publish it. */
const runningTurn = (id: string, nickname = "arch"): AgentTurn =>
  ({
    id,
    agentId: `agent-${nickname}`,
    nickname,
    origin: artifactDiscussionOrigin("d1", "a.md"),
    triggerCommentId: "dc1",
    state: "running",
    failure: null,
    retryPermitted: false,
    startedAt: "2026-01-02T00:00:00Z",
    endedAt: null,
  }) as AgentTurn;

async function replyTo(threadId: string, text: string) {
  const box = await screen.findByLabelText(`Reply to thread ${threadId}`);
  await userEvent.click(box);
  await userEvent.type(box, text);
  const card = screen.getByTestId(`comment-thread-${threadId}`);
  await userEvent.click(within(card).getByRole("button", { name: "Post" }));
}

// ---------------------------------------------------------------------------

describe("a follow-up reaches the conversation's active agents (CTA-FR-HCMJ)", () => {
  it("reaches the agent the author last named without it being tagged again", async () => {
    // The defect this guards: a reply channel that dropped the conversation on
    // the way to the routing rule. The rule needs the thread's own comments —
    // hand it none and a follow-up that named nobody dispatches nobody, and the
    // conversation quietly stops having a second party (CTA-FR-LCFU).
    const b = await mount();

    await replyTo("d1", "isn't it too much?");

    await waitFor(() => expect(b.dispatches).toHaveLength(1));
    expect(b.dispatches[0].nickname).toBe("arch");
  });

  it("carries the artifact_discussion origin, so the agent is given the file", async () => {
    // AGC-FR-05 / AGC-FR-07: the origin is what decides the material the turn is
    // built from. Dispatching a discussion's turn under `artifact_comment` reads
    // as a success and gets the agent the wrong context — and the resulting turn
    // then belongs to neither card, so nothing renders while it runs.
    const b = await mount();

    await replyTo("d1", "isn't it too much?");

    await waitFor(() => expect(b.dispatches).toHaveLength(1));
    expect(b.dispatches[0].origin).toEqual(artifactDiscussionOrigin("d1", "a.md"));
  });

  it("steers at one agent when the follow-up names one, and dismisses the rest", async () => {
    // CTA-FR-QUXJ's first case: a comment reaches exactly the agents its own tags
    // resolve to, and by standing in the conversation it makes exactly those its
    // active agents from then on — an explicit mention replaces the set.
    const b = await mount();

    await replyTo("d1", "@sec any objection?");

    await waitFor(() => expect(b.dispatches).toHaveLength(1));
    expect(b.dispatches[0].nickname).toBe("sec");
  });

  it("CMT-FR-61, CTA-FR-RPVU, ACT-FR-13 / CTA-FR-DGOC: routes an anchored card in the same rail on identical terms", async () => {
    // The active set binds every conversation this rail holds and distinguishes
    // none of them. The anchored thread's own newest naming comment reads
    // `@arch which session?`, so an untagged reply there reaches `@arch` — by
    // the same rule and the same code path the discussion above uses.
    const b = await mount({ anchoredThreads: [anchored()] });

    await replyTo("t1", "still unclear");

    await waitFor(() =>
      expect(b.calls.filter((c) => c.cmd === "add_comment")).toHaveLength(1),
    );
    await waitFor(() => expect(b.dispatches).toHaveLength(1));
    expect(b.dispatches[0].nickname).toBe("arch");
    // AGC-FR-05: an anchored thread names its fragment target.
    expect(b.dispatches[0].origin).toMatchObject({
      discussionId: "t1",
      target: { kind: "artifact" },
      fragmentTarget: expect.objectContaining({ path: expect.any(String) }),
    });
  });

  it("reaches nobody from an anchored card the author has addressed nobody in", async () => {
    // CTA-FR-IXTC's third rule, on the same surface: an untagged comment in a
    // conversation with no active agent dispatches nothing at all.
    const b = await mount({
      anchoredThreads: [
        anchored({
          comments: [
            {
              id: "c1",
              author: human("raver119"),
              body: "which session?",
              quotes: [],
              attachments: [],
              createdAt: "2026-01-01T00:00:00Z",
            },
          ],
        }),
      ],
    });

    await replyTo("t1", "still unclear");

    await waitFor(() =>
      expect(b.calls.filter((c) => c.cmd === "add_comment")).toHaveLength(1),
    );
    expect(b.dispatches).toHaveLength(0);
  });
});

describe("what a discussion card shows while a turn is outstanding", () => {
  it("renders Thinking… on the discussion card (CTA-FR-AZEU)", async () => {
    // The reported gap. The turn is held by the control's hook, and the rail was
    // given the rail hook's outstanding set alone, so the placeholder that says
    // an answer is coming never appeared and the author had nothing to tell a
    // dispatched turn from a dropped one.
    await mount();

    await replyTo("d1", "isn't it too much?");

    const card = await screen.findByTestId("comment-thread-d1");
    const pending = await within(card).findByTestId("comment-pending");
    expect(pending).toHaveTextContent("Thinking…");
    expect(pending).toHaveTextContent("arch");
  });

  it("clears it when the turn ends, and reports a failure on the card (CTA-FR-ZDDI)", async () => {
    await mount();
    await replyTo("d1", "isn't it too much?");
    const card = await screen.findByTestId("comment-thread-d1");
    await within(card).findByTestId("comment-pending");

    emitTurn({
      id: "turn-1",
      agentId: "agent-arch",
      nickname: "arch",
      origin: artifactDiscussionOrigin("d1", "a.md"),
      triggerCommentId: "c-3",
      state: "failed",
      failure: "unreachable",
      startedAt: "2026-01-02T00:00:00Z",
      endedAt: "2026-01-02T00:00:05Z",
    } as AgentTurn);

    await waitFor(() =>
      expect(within(card).queryByTestId("comment-pending")).not.toBeInTheDocument(),
    );
    expect(await within(card).findByTestId("comment-turn-error")).toHaveTextContent(
      /provider could not be reached/,
    );
  });

  it("clears the failure when the author retries, and does not restore it", async () => {
    // Both hooks file the failure — the rail hook keeps a discussion's turns so
    // its `allTurns` is complete for the control — but only the control clears
    // one. Merged by last-writer-wins, the rail hook's copy outlives the clear
    // and the card reports a failure the author has already retried past.
    const b = await mount();
    emitTurn({
      ...runningTurn("turn-9"),
      state: "failed",
      failure: "unreachable",
      endedAt: "2026-01-02T00:00:05Z",
    } as AgentTurn);
    const card = await screen.findByTestId("comment-thread-d1");
    await within(card).findByTestId("comment-turn-error");

    await replyTo("d1", "@arch trying again");

    await waitFor(() => expect(b.dispatches).toHaveLength(1));
    await waitFor(() =>
      expect(
        within(card).queryByTestId("comment-turn-error"),
      ).not.toBeInTheDocument(),
    );
  });

  it("takes the placeholder back when the author cancels the turn", async () => {
    // The cancellation has to reach the hook that *holds* the turn: performed on
    // the other one it removes nothing, and the card keeps claiming an answer is
    // coming after the author has said they no longer want it.
    await mount();
    await replyTo("d1", "isn't it too much?");
    const card = await screen.findByTestId("comment-thread-d1");
    await within(card).findByTestId("comment-pending");

    await userEvent.click(within(card).getByTestId("comment-pending-cancel"));

    await waitFor(() =>
      expect(within(card).queryByTestId("comment-pending")).not.toBeInTheDocument(),
    );
  });

  it("renders one from the registration event alone, with nothing dispatched here", async () => {
    // The other half of where a turn can come from: the tab's own snapshot,
    // supplied by the rail hook. A discussion whose turn was started elsewhere —
    // or before this tab opened — still says an answer is coming.
    await mount();

    emitTurn(runningTurn("turn-9"));

    const card = await screen.findByTestId("comment-thread-d1");
    expect(await within(card).findByTestId("comment-pending")).toHaveTextContent(
      "Thinking…",
    );
  });

  it("shows one contribution when the dispatch and the event both arrive", async () => {
    // The registration event is published before `dispatch_agent_turn` resolves
    // and there is no ordering between the two, so the same turn reaches the
    // card by both routes. Counted by id, it is one answer coming — not two.
    const b = await mount();
    await replyTo("d1", "isn't it too much?");
    await waitFor(() => expect(b.dispatches).toHaveLength(1));
    const card = await screen.findByTestId("comment-thread-d1");
    await within(card).findByTestId("comment-pending");

    emitTurn(runningTurn("turn-1"));

    expect(within(card).getAllByTestId("comment-pending")).toHaveLength(1);
  });

  it("does not let a late running event resurrect a cancelled turn", async () => {
    // The suppression has to outlast the cancellation itself: the backend may
    // still publish a `running` state for a turn it has not yet stopped, and
    // re-rendering the placeholder would undo what the author just asked for.
    await mount();
    emitTurn(runningTurn("turn-9"));
    const card = await screen.findByTestId("comment-thread-d1");
    await within(card).findByTestId("comment-pending");
    await userEvent.click(within(card).getByTestId("comment-pending-cancel"));
    await waitFor(() =>
      expect(within(card).queryByTestId("comment-pending")).not.toBeInTheDocument(),
    );

    emitTurn(runningTurn("turn-9"));

    expect(
      within(card).queryByTestId("comment-pending"),
    ).not.toBeInTheDocument();
  });

  it("CVP-FR-HWTN: a dispatch result that lands after the turn ended adds no row", async () => {
    // Stands in for the shell's follower (`App.tsx`), which feeds the record of
    // ended turns. This suite mounts the Editor alone.
    await onAgentTurnStateChanged(followAgentTurn);
    const b = await mount();
    const base = invokeMock.getMockImplementation()!;
    let release!: () => void;
    const held = new Promise<void>((resolve) => (release = resolve));
    invokeMock.mockImplementation(async (cmd: string, args: unknown) =>
      cmd === "dispatch_agent_turn"
        ? held.then(() => base(cmd, args))
        : base(cmd, args),
    );
    await replyTo("d1", "isn't it too much?");
    const card = await screen.findByTestId("comment-thread-d1");

    emitTurn(runningTurn("turn-1"));
    await within(card).findByTestId("comment-pending");
    emitTurn({ ...runningTurn("turn-1"), state: "delivered" });
    await act(async () => release());
    await waitFor(() => expect(b.dispatches).toHaveLength(1));

    expect(within(card).queryByTestId("comment-pending")).not.toBeInTheDocument();
  });

  it("keeps one conversation's turns off the other's card", async () => {
    // ACT-FR-22: the tab reads the running set once and both surfaces filter it
    // by their own origin. A union that skipped the filtering would render this
    // discussion's turn on the anchored card too.
    await mount({
      anchoredThreads: [anchored()],
      turns: [
        {
          id: "turn-0",
          agentId: "agent-arch",
          nickname: "arch",
          origin: artifactDiscussionOrigin("d1", "a.md"),
          triggerCommentId: "dc1",
          state: "running",
          failure: null,
          startedAt: "2026-01-01T00:00:30Z",
          endedAt: null,
        } as AgentTurn,
      ],
    });

    const discussionCard = await screen.findByTestId("comment-thread-d1");
    expect(
      await within(discussionCard).findByTestId("comment-pending"),
    ).toBeInTheDocument();
    const anchoredCard = screen.getByTestId("comment-thread-t1");
    expect(
      within(anchoredCard).queryByTestId("comment-pending"),
    ).not.toBeInTheDocument();
  });
});

describe("the tab counts one conversation once (CMT-FR-56)", () => {
  it("does not inflate the count when a discussion is replied to", async () => {
    // The reported miscount: replying folded the returned discussion into the
    // anchored list, so the one thread was counted by both hooks and the tab
    // read 2 for a single unresolved conversation.
    await mount();
    expect(counter()).toHaveAccessibleName(/\(1 unresolved\)/);

    await replyTo("d1", "isn't it too much?");
    await waitFor(() =>
      expect(screen.getByTestId("comment-thread-d1")).toHaveTextContent(
        "isn't it too much?",
      ),
    );

    expect(counter()).toHaveAccessibleName(/\(1 unresolved\)/);
    // And it is still one card, not two: a discussion folded into the aligned
    // layer renders a second time, as an orphan.
    expect(screen.getAllByTestId("comment-thread-d1")).toHaveLength(1);
  });

  it("counts an anchored thread and a discussion as two", async () => {
    // The other half of CMT-FR-56 — the count is the tab's total, so a rail
    // holding one of each reads 2 and neither is omitted.
    await mount({ anchoredThreads: [anchored()] });
    expect(counter()).toHaveAccessibleName(/\(2 unresolved\)/);
  });

  it("locks the discussion from its own card, on the command's reply alone", async () => {
    // Deliberately without a `discussion-changed` event: a lock routed to
    // the anchored hook now returns a thread that hook drops (CMT-FR-61), so the
    // card would go on offering a composer for a conversation that takes no
    // further comment — and only a test that emits nothing catches it.
    const b = await mount();
    const card = await screen.findByTestId("comment-thread-d1");
    await userEvent.click(
      within(card).getByRole("button", { name: "Thread actions for d1" }),
    );
    await userEvent.click(await screen.findByRole("menuitem", { name: /lock/i }));

    await waitFor(() =>
      expect(
        screen.queryByLabelText("Reply to thread d1"),
      ).not.toBeInTheDocument(),
    );
    expect(
      b.calls.filter((c) => c.cmd === "set_discussion_lock"),
    ).toHaveLength(1);
  });

  it("drops back when the discussion is resolved from its own card", async () => {
    // The resolution channel is routed by kind like the reply channel; performed
    // against the anchored hook it would leave the control's count standing and
    // the card in the rail.
    await mount();
    const card = await screen.findByTestId("comment-thread-d1");
    await userEvent.click(
      within(card).getByRole("button", { name: "Thread actions for d1" }),
    );
    await userEvent.click(await screen.findByRole("menuitem", { name: /resolve/i }));

    await waitFor(() =>
      expect(counter()).toHaveAccessibleName(/\(0 unresolved\)/),
    );
  });
});

describe("the routing is per thread, not per kind in bulk", () => {
  it("answers in the discussion that was replied to", async () => {
    // A membership test degraded into "is there any discussion" would dispatch
    // against the wrong conversation and render the placeholder on the wrong
    // card, which reads as the feature working.
    const second = discussion({
      id: "d2",
      comments: [
        {
          id: "d2c1",
          author: human("raver119"),
          body: "@sec and the storage side?",
          quotes: [],
          attachments: [],
          createdAt: "2026-01-01T00:02:00Z",
        },
      ],
    });
    const b = await mount({ discussions: [discussion(), second] });

    await replyTo("d2", "any update?");

    await waitFor(() => expect(b.dispatches).toHaveLength(1));
    expect(b.dispatches[0]).toEqual({
      nickname: "sec",
      origin: artifactDiscussionOrigin("d2", "a.md"),
    });
    const answered = await screen.findByTestId("comment-thread-d2");
    expect(await within(answered).findByTestId("comment-pending")).toBeInTheDocument();
    expect(
      within(screen.getByTestId("comment-thread-d1")).queryByTestId(
        "comment-pending",
      ),
    ).not.toBeInTheDocument();
  });

  it("posts into a discussion nobody is in, and calls nobody", async () => {
    // CTA-FR-HCMJ's third case, through the wiring: the message stands on its own,
    // and an empty participant set is not an occasion to fall back to anything.
    const b = await mount({
      discussions: [
        discussion({
          comments: [
            {
              id: "dc1",
              author: human("raver119"),
              body: "thinking out loud about this one",
              quotes: [],
              attachments: [],
              createdAt: "2026-01-01T00:00:00Z",
            },
          ],
        }),
      ],
    });

    await replyTo("d1", "still unsure");

    await waitFor(() =>
      expect(screen.getByTestId("comment-thread-d1")).toHaveTextContent(
        "still unsure",
      ),
    );
    expect(b.dispatches).toHaveLength(0);
  });

  it("renders a refused reply on the discussion's own card (CMT-FR-34)", async () => {
    // The error channel is a union too. Filed by thread id under the anchored
    // hook alone, a discussion's refusal has nowhere to render and the post
    // appears to have simply vanished.
    await mount({ addCommentError: "discussion_locked" });

    await replyTo("d1", "one more thing");

    const card = await screen.findByTestId("comment-thread-d1");
    await waitFor(() =>
      expect(card).toHaveTextContent(/thread was locked, so it takes no new/),
    );
    // NAW-FR-33: the body survives the refusal, so the message is not retyped.
    expect(screen.getByLabelText("Reply to thread d1")).toHaveValue(
      "one more thing",
    );
  });
});

describe("CTA-FR-YGYP / AGT-FR-37: @all in a discussion", () => {
  /** A discussion whose opening message addressed the room. */
  const addressedEveryone = () =>
    discussion({
      comments: [
        {
          id: "dc1",
          author: human("raver119"),
          body: "@all where should graduation live?",
          quotes: [],
          attachments: [],
          createdAt: "2026-01-01T00:00:00Z",
        },
      ],
    });

  it("makes every agent that can answer a participant, and follows up to all of them", async () => {
    // CTA-FR-YGYP, CTA-FR-QNBS. Through the real surface because
    // `useDiscussions` derives the active agents from the comment bodies it holds; a
    // roster captured at mount rather than read at post time would pass every
    // pure test and dispatch to the wrong set here.
    const b = await mount({ discussions: [addressedEveryone()] });

    await userEvent.type(
      screen.getByLabelText("Reply to thread d1"),
      "and the archive part?",
    );
    await userEvent.click(screen.getByRole("button", { name: "Post" }));

    await waitFor(() => expect(b.dispatches).toHaveLength(2));
    expect(b.dispatches.map((d) => d.nickname)).toEqual(["arch", "sec"]);
    // Each names this thread with the origin its target decides.
    expect(b.dispatches[0].origin).toMatchObject({
      discussionId: "d1",
      target: { kind: "artifact" },
      fragmentTarget: null,
    });
  });

  it("takes in an agent the project enrolled after the handle was written", async () => {
    // AGT-FR-37 / AGT-FR-40: the stored body still reads `@all`, and what it means
    // moves with the project's enrolment — the clause that breaks the moment the
    // expansion is frozen into the message at post time.
    //
    // The roster is read when the rail comes up (`useDiscussions`), so "enrolled
    // afterwards" is exercised by the same stored discussion coming up against a
    // larger enrolment — a reopened tab, or the next launch. The comment is
    // byte-identical in both mounts; only who it reaches differs.
    const stored = addressedEveryone();
    const before = await mount({ discussions: [stored] });
    await userEvent.type(
      screen.getByLabelText("Reply to thread d1"),
      "and the archive part?",
    );
    await userEvent.click(screen.getByRole("button", { name: "Post" }));
    await waitFor(() => expect(before.dispatches).toHaveLength(2));

    cleanup();

    const after = await mount({
      discussions: [addressedEveryone()],
      agents: [
        projectAgent("arch", "a1"),
        projectAgent("sec", "a2"),
        projectAgent("scribe", "a3"),
      ],
    });
    await userEvent.type(
      screen.getByLabelText("Reply to thread d1"),
      "and the archive part?",
    );
    await userEvent.click(screen.getByRole("button", { name: "Post" }));

    await waitFor(() => expect(after.dispatches).toHaveLength(3));
    expect(after.dispatches.map((d) => d.nickname)).toEqual([
      "arch",
      "sec",
      "scribe",
    ]);
    // Nothing anywhere rewrote the comment that said it.
    expect(after.discussions[0].comments[0].body).toBe(
      "@all where should graduation live?",
    );
  });

  it("CTA-FR-YGYP: drops an agent the project has since removed, the body still reading @all", async () => {
    // AGT-FR-37's other direction, and the half the growing case cannot show: an
    // agent withdrawn from the project leaves the conversation, and the comment
    // that named the room is byte-identical either way.
    const shrunk = await mount({
      discussions: [addressedEveryone()],
      agents: [projectAgent("arch", "a1"), projectAgent("scribe", "a3")],
    });
    await userEvent.type(
      screen.getByLabelText("Reply to thread d1"),
      "and the archive part?",
    );
    await userEvent.click(screen.getByRole("button", { name: "Post" }));

    await waitFor(() => expect(shrunk.dispatches).toHaveLength(2));
    expect(shrunk.dispatches.map((d) => d.nickname)).toEqual(["arch", "scribe"]);
    expect(shrunk.discussions[0].comments[0].body).toBe(
      "@all where should graduation live?",
    );
  });

  it("steers at one agent when the follow-up names it, dismissing the others", async () => {
    // CTA-FR-QUXJ: an explicit mention REPLACES the set — an author who turns to
    // somebody has turned to them, and the room they were addressing before is
    // no longer who a following untagged message reaches.
    const b = await mount({ discussions: [addressedEveryone()] });
    await userEvent.type(
      screen.getByLabelText("Reply to thread d1"),
      "@arch only, please",
    );
    await userEvent.click(screen.getByRole("button", { name: "Post" }));

    await waitFor(() => expect(b.dispatches).toHaveLength(1));
    expect(b.dispatches[0].nickname).toBe("arch");
  });
});

// ---------------------------------------------------------------------------
// CTA-FR-MGVJ / CTA-FR-RUVS: a discussion's offer to retry belongs to one holder
// ---------------------------------------------------------------------------

/** AGC-FR-31: a discussion turn whose attempts ran out on a recoverable failure. */
const failedDiscussionTurn = (id = "turn-1", nickname = "arch"): AgentTurn =>
  ({
    ...runningTurn(id, nickname),
    state: "failed",
    failure: "unreachable",
    retryPermitted: true,
    endedAt: "2026-01-02T00:01:00Z",
  }) as AgentTurn;

describe("CTA-FR-MGVJ / CTA-FR-RHPP: a discussion's failed contribution", () => {
  it("renders one offer, and Retry leaves no second copy behind", async () => {
    // The tab mounts two conversational surfaces over one artifact — the rail's
    // anchored threads and the action control's discussions (ACT-FR-22) — and
    // both read the recovery registry. A discussion's offer is the control's:
    // nothing in the rail's own hook ever retries one, so a copy kept there
    // would never be cleared by the retry that consumed it, and the card would
    // go on offering a turn already asked again *beside* the **Thinking…** of
    // the turn that replaced it.
    const b = await mount({ recoverable: [failedDiscussionTurn()] });

    const failed = await screen.findByTestId("comment-failed");
    expect(failed.textContent).toContain("arch");
    expect(screen.getAllByTestId("comment-failed")).toHaveLength(1);

    await userEvent.click(screen.getByTestId("comment-failed-retry"));

    await waitFor(() =>
      expect(b.calls.filter((c) => c.cmd === "retry_agent_turn")).toHaveLength(1),
    );
    // The offer is gone from every holder, not just the one that took it.
    await waitFor(() =>
      expect(screen.queryByTestId("comment-failed")).not.toBeInTheDocument(),
    );

    // And the turn that replaced it renders alone in that position.
    emitTurn(runningTurn("turn-retry-1"));
    expect(await screen.findByTestId("comment-pending")).toBeInTheDocument();
    expect(screen.queryByTestId("comment-failed")).not.toBeInTheDocument();
    // CTA-FR-XNBS: nothing was posted by any of it.
    expect(b.calls.filter((c) => c.cmd === "add_comment")).toHaveLength(0);
  });
});
