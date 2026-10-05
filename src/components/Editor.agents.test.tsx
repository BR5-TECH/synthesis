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
import { followAgentTurn } from "../state/discussionSession";
import { onAgentTurnStateChanged } from "../events";
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

describe("CTA-FR-VQFJ: the composer offers the project's agents", () => {
  it("opens the mention picker on @ in an unlocked composer", async () => {
    // CTA-FR-SKXK, CMT-FR-15, AGT-FR-27's first half, per AGT-FR-25.
    await mount();
    await userEvent.type(composer(), "@");
    const picker = await screen.findByTestId("mention-picker");
    // AGT-FR-39: the `@all` handle leads, naming the agents it stands for where a
    // nickname's row names its model, then the enrolled agents themselves.
    expect(
      within(picker).getAllByTestId("mention-picker-option").map((o) => o.textContent),
    ).toEqual([
      expect.stringContaining("@all"),
      expect.stringContaining("@arch"),
      expect.stringContaining("@sec"),
    ]);
  });

  it("names an agent's title where a model used to stand, and nothing when it has none", async () => {
    // AGT-FR-26, AGT-FR-39 / AGT-FR-43. The position says what role this collaborator
    // takes; an agent with no title leaves it blank rather than falling back to
    // the model, which is the fallback this replaced and which must not survive
    // anywhere in the picker.
    await mount({
      agents: [
        projectAgent("arch", "a1", "ready", "Architect"),
        projectAgent("sec", "a2"),
        // AGT-FR-43: an agent that carries a title AND cannot answer. The
        // availability reason claims that one position, so the entry states
        // either the role or the reason and never both — which a pair of
        // `ready` agents cannot tell apart.
        projectAgent("scribe", "a3", "provider_unverified", "Reviewer"),
      ],
    });
    await userEvent.type(composer(), "@");
    const picker = await screen.findByTestId("mention-picker");
    // By the row's own leading handle rather than by its text: the `@all` row
    // *names* the agents it stands for, so a substring match finds it first.
    const row = (handle: string) =>
      within(picker)
        .getAllByTestId("mention-picker-option")
        .filter((o) => o.dataset.entry === "agent")
        .find((o) => o.firstElementChild?.textContent === handle)!;

    expect(row("@arch")).toHaveTextContent("Architect");
    // The untitled one carries its handle and nothing else — no placeholder, no
    // `Not defined`, and no model in its stead.
    expect((row("@sec").textContent ?? "").trim()).toBe("@sec");

    const scribe = row("@scribe");
    expect(scribe).not.toHaveTextContent("Reviewer");
    expect(scribe.textContent ?? "").toMatch(/not verified|unverified|provider/i);

    // AGT-FR-43: no model reaches this surface at all, by label or by id.
    const text = picker.textContent ?? "";
    expect(text).not.toContain("Claude Opus 5");
    expect(text).not.toContain("anthropic/claude-opus-5");
    expect(text).not.toContain("Not defined");
  });

  it("completes an agent with Tab, where the composer has real neighbours", async () => {
    // CTA-FR-SKXK, CMT-FR-15, AGT-FR-25's Tab clause, per AGT-FR-27. Asserted here rather than only
    // against the standalone composer because the rail's card is the one place
    // the composer has sibling focusables — Cancel and Post — so it is the only
    // surface where a Tab prevented on the wrong branch actually costs the
    // author something.
    await mount();
    await userEvent.type(composer(), "Ask @ar");
    await screen.findByTestId("mention-picker");

    await userEvent.keyboard("{Tab}");
    await waitFor(() => expect(composer()).toHaveValue("Ask @arch "));
    expect(screen.queryByTestId("mention-picker")).not.toBeInTheDocument();
    expect(composer()).toHaveFocus();
  });

  it("opens no picker for an @ that begins no word", async () => {
    // CTA-FR-SKXK, CMT-FR-15, AGT-FR-27 / AGT-FR-25: an author writing an email address in a comment is
    // not interrupted.
    await mount();
    await userEvent.type(composer(), "mail me at me@example.com");
    expect(screen.queryByTestId("mention-picker")).not.toBeInTheDocument();
  });

  it("gives a locked thread no composer and so no way to address anyone", async () => {
    // CTA-FR-SKXK, AGT-FR-25, AGT-FR-27's second half / CMT-FR-15: a thread that takes no further
    // comment takes no further participant either.
    await mount({ threads: [makeThread({ locked: true })] });
    expect(screen.queryByLabelText("Reply to thread t1")).not.toBeInTheDocument();
    expect(screen.queryByTestId("mention-picker")).not.toBeInTheDocument();
  });
});

describe("CTA-FR-EACI / CTA-FR-ZVKL: appending first, then dispatching", () => {
  it("appends the comment, then dispatches once per distinct agent", async () => {
    // CTA-FR-EACI, CTA-FR-RPVU.
    const b = await mount();
    await userEvent.type(composer(), "@arch @sec is this two specs?");
    await userEvent.click(screen.getByRole("button", { name: "Post" }));

    await waitFor(() => expect(calls(b, "dispatch_agent_turn")).toHaveLength(2));
    // The comment lands first and unconditionally.
    const order = b.calls.map((c) => c.cmd);
    expect(order.indexOf("add_comment")).toBeLessThan(
      order.indexOf("dispatch_agent_turn"),
    );
    expect(
      calls(b, "dispatch_agent_turn").map(
        (c) => (c.args as { nickname: string }).nickname,
      ),
    ).toEqual(["arch", "sec"]);
    // Each names this thread and the just-appended comment.
    for (const call of calls(b, "dispatch_agent_turn")) {
      const args = call.args as {
        origin: unknown;
        triggerCommentId: string;
      };
      // AGC-FR-05: the backend's shape, with the thread's own fragment target.
      expect(args.origin).toEqual({
        discussionId: "t1",
        target: { kind: "artifact", artifactId: "a.md" },
        fragmentTarget: makeThread().fragmentTarget,
      });
      expect(args.triggerCommentId).toBe("c-2");
    }
  });

  it("CTA-FR-DGOC, CMT-FR-61, CTA-FR-RPVU, ACT-FR-13: an anchored thread derives its active agents as a discussion does, and the rail begins no discussion", async () => {
    // CTA-FR-DGOC: the active set binds every conversation the rail holds and
    // distinguishes none of them. A remark pinned to a line is a conversation
    // the author is holding with somebody as much as a discussion is, so an
    // untagged reply here reaches whoever the author last named — `@arch`, whose
    // own answer naming `@sec` settles nothing (CTA-FR-DWCK).
    const b = await mount({
      threads: [
        makeThread({
          comments: [
            {
              id: "c1",
              author: human("raver119"),
              body: "@arch which session?",
              quotes: [],
              attachments: [],
              createdAt: "2026-01-01T00:00:00Z",
            },
            {
              id: "c2",
              author: { kind: "agent", agentId: "a1", handle: "arch" },
              body: "The kickoff one. @sec should weigh in.",
              quotes: [],
              attachments: [],
              createdAt: "2026-01-01T00:01:00Z",
            },
          ],
        }),
      ],
    });

    await userEvent.type(composer(), "and the archive part?");
    await userEvent.click(screen.getByRole("button", { name: "Post" }));
    await waitFor(() => expect(calls(b, "add_comment")).toHaveLength(1));
    await waitFor(() =>
      expect(calls(b, "dispatch_agent_turn")).toHaveLength(1),
    );
    expect(calls(b, "dispatch_agent_turn")[0].args).toMatchObject({
      nickname: "arch",
    });

    //
    // CMT-FR-61: the rail itself never BEGINS a discussion. It reads this
    // artifact's discussions (there are none here, so no section renders), but
    // no affordance in it opens a thread that is not anchored to a passage — the
    // one route to one is the tab's action control (ACT-FR-13).
    expect(screen.queryByTestId("discussion-section")).not.toBeInTheDocument();
    expect(b.calls.some((c) => c.cmd === "open_discussion")).toBe(false);
  });

  it("dispatches nothing for a tag that matches no enrolled agent", async () => {
    // CTA-FR-EACI, CTA-FR-RPVU's second half, per AGT-FR-28.
    const b = await mount();
    await userEvent.type(composer(), "@nobody thoughts?");
    await userEvent.click(screen.getByRole("button", { name: "Post" }));
    await waitFor(() => expect(calls(b, "add_comment")).toHaveLength(1));
    expect(calls(b, "dispatch_agent_turn")).toHaveLength(0);
  });

  it("CTA-FR-ZOLW: addresses an agent again without waiting on or cancelling the first", async () => {
    // CTA-FR-ZVKL / CTA-FR-BXNF, CTA-FR-QXIG, CTA-FR-ZOLW.
    const b = await mount();
    await userEvent.type(composer(), "@arch first");
    await userEvent.click(screen.getByRole("button", { name: "Post" }));
    await waitFor(() => expect(screen.getAllByTestId("comment-pending")).toHaveLength(1));

    await userEvent.type(composer(), "@arch again");
    await userEvent.click(screen.getByRole("button", { name: "Post" }));
    await waitFor(() => expect(calls(b, "dispatch_agent_turn")).toHaveLength(2));

    // CTA-FR-ZOLW: both turns run, and the agent still shows as one placeholder.
    expect(calls(b, "cancel_agent_turn")).toHaveLength(0);
    const pending = screen.getAllByTestId("comment-pending");
    expect(pending).toHaveLength(1);
    expect(pending[0]).toHaveTextContent("arch");
  });
});

describe("CVP-FR-HWTN: a turn that ended never draws as pending again", () => {
  it("a dispatch result that lands after the turn ended adds no placeholder", async () => {
    // Stands in for the shell's follower (`App.tsx`), which feeds the record of
    // ended turns. This suite mounts the Editor alone.
    await onAgentTurnStateChanged(followAgentTurn);
    const b = await mount();
    const base = invokeMock.getMockImplementation()!;
    let release!: () => void;
    const held = new Promise<void>((resolve) => (release = resolve));
    let asked = 0;
    invokeMock.mockImplementation(async (cmd: string, args: unknown) => {
      if (cmd !== "dispatch_agent_turn") return base(cmd, args);
      asked += 1;
      return held.then(() => base(cmd, args));
    });
    await userEvent.type(composer(), "@arch thoughts?");
    await userEvent.click(screen.getByRole("button", { name: "Post" }));
    await waitFor(() => expect(asked).toBe(1));

    const live = turn({ id: "turn-1", nickname: "arch" });
    emitTurn(live);
    await screen.findByTestId("comment-pending");
    emitTurn({ ...live, state: "delivered", endedAt: "2026-01-01T00:01:00Z" });
    await act(async () => release());
    await waitFor(() => expect(calls(b, "dispatch_agent_turn")).toHaveLength(1));

    expect(screen.queryByTestId("comment-pending")).not.toBeInTheDocument();
  });
});

describe("CTA-FR-ZOLW, CTA-FR-XMCQ / CTA-FR-VHOY: what the placeholder says while an agent thinks", () => {
  it("reads Thinking…, attributed to the agent, with no timestamp and no attachment", async () => {
    await mount();
    await userEvent.type(composer(), "@arch thoughts?");
    await userEvent.click(screen.getByRole("button", { name: "Post" }));

    const pending = await screen.findByTestId("comment-pending");
    // Worded as a participant who has not answered yet, rather than as a
    // spinner or a progress bar.
    expect(pending).toHaveTextContent("Thinking…");
    expect(pending).toHaveTextContent("arch");
    // Shaped like a comment: it occupies the position the answer will take,
    // among the thread's own comments.
    expect(pending.className).toContain("comment");
    // Nothing to stamp yet, and nothing attached.
    expect(pending.querySelector(".comment__time")).toBeNull();
    expect(
      within(pending).queryByTestId("comment-attachments"),
    ).not.toBeInTheDocument();
    // It carries the control that abandons the turn.
    expect(
      within(pending).getByRole("button", { name: /Cancel .*'s reply/ }),
    ).toBeInTheDocument();
  });

  it("leaves no trace when the application is relaunched mid-turn", async () => {
    // CTA-FR-ZJZD: the placeholder is held in memory by the rail alone and no log
    // line records it, so a fresh mount with the turn gone shows the author's
    // comment and nothing else (AGC-FR-23).
    await mount();
    await userEvent.type(composer(), "@arch thoughts?");
    await userEvent.click(screen.getByRole("button", { name: "Post" }));
    await screen.findByTestId("comment-pending");

    cleanup();
    // A relaunch: the backend reports no turn in flight anywhere.
    await mount({ turns: [] });

    await waitFor(() =>
      expect(screen.queryByTestId("comment-pending")).not.toBeInTheDocument(),
    );
    expect(screen.queryByText("Thinking…")).not.toBeInTheDocument();
  });
});

describe("CTA-FR-FBJR, CTA-FR-IHOB … CTA-FR-IVNG, CTA-FR-JQUU: the activity status, driven by the event", () => {
  const status = () =>
    screen.getByTestId("comment-pending-status").textContent ?? "";

  /** The turn the rail already holds, with these calls now active in it. */
  function active(...calls: AgentTurn["activeToolCalls"]): AgentTurn {
    return turn({ id: "turn-1", nickname: "arch", activeToolCalls: calls });
  }

  it("follows the calls the turn reports, and is blank at no moment", async () => {
    // CTA-FR-FBJR, CTA-FR-IHOB / CTA-FR-IVNG, CTA-FR-JQUU through the channel the rail actually reads:
    // `"agent turn state changed"` (CTA-FR-QXIG, AGC-FR-34). The static-prop
    // tests in `CommentRail.test.tsx` cannot see a reducer that dropped
    // `activeToolCalls` or a surface that ignored a non-terminal event.
    await mount({ turns: [active()] });
    expect(status()).toBe("Thinking…");

    // A call becomes active in that turn.
    emitTurn(active({ id: "call-1", tool: "search_specifications", activationSeq: 1 }));
    expect(status()).toBe("Searching related specifications…");

    // A second, activated later, is the one spoken for (CTA-FR-IWOJ, CTA-FR-KWOF).
    emitTurn(
      active(
        { id: "call-1", tool: "search_specifications", activationSeq: 1 },
        { id: "call-2", tool: "openrouter:web_search", activationSeq: 2 },
      ),
    );
    expect(status()).toBe("Searching the web…");

    // The web search finishes: the line goes to the most recently activated
    // call STILL active, at once, and never to an empty string (CTA-FR-IVNG, CTA-FR-JQUU).
    emitTurn(active({ id: "call-1", tool: "search_specifications", activationSeq: 1 }));
    expect(status()).toBe("Searching related specifications…");

    // And when that one refuses, the turn is waiting on its model again.
    emitTurn(active());
    expect(status()).toBe("Thinking…");
  });

  it("reads the turn's activation order, not the order the events arrived", async () => {
    // CTA-FR-IWOJ, CTA-FR-KWOF's second clause: one payload is a whole reading of the turn
    // (AGC-FR-34), and the order inside it is the turn's own — so a payload
    // listing the later call first settles on the same status.
    await mount({ turns: [active()] });
    emitTurn(
      active(
        { id: "call-2", tool: "search_skills", activationSeq: 2 },
        { id: "call-1", tool: "read_file", activationSeq: 1 },
      ),
    );
    expect(status()).toBe("Searching available skills…");
  });

  it("reads Working… for a tool the rail has not been taught", async () => {
    // CTA-FR-FBJR, CTA-FR-IHOB's last clause / TLC-FR-27: never an empty line, and never the
    // tool's own name.
    await mount({ turns: [active()] });
    emitTurn(active({ id: "call-1", tool: "a_tool_added_since", activationSeq: 1 }));
    expect(status()).toBe("Working…");
    expect(screen.getByTestId("comment-pending").textContent ?? "").not.toContain(
      "a_tool_added_since",
    );
  });

  it("cancels from the contribution while a call is active", async () => {
    // CTA-FR-IVNG, CTA-FR-JQUU's last clause: the control invokes `"cancel agent turn"` for
    // this turn exactly as it does for one with no tool call active.
    const b = await mount({ turns: [active()] });
    emitTurn(active({ id: "call-1", tool: "read_file", activationSeq: 1 }));
    expect(status()).toBe("Reading a project file…");

    await userEvent.click(
      screen.getByRole("button", { name: /Cancel .*'s reply/ }),
    );
    await waitFor(() =>
      expect(calls(b, "cancel_agent_turn")).toHaveLength(1),
    );
    expect(calls(b, "cancel_agent_turn")[0].args).toMatchObject({
      turnId: "turn-1",
    });
  });

  it("leaves two pending contributions in the order they were registered", async () => {
    // CTA-FR-ZOLW / CMT-FR-79: a running turn is now reported many times rather
    // than once (AGC-FR-34), and each contribution has to keep occupying the
    // position that agent's answer will take. A reducer that moved the
    // reported turn to the end of its list would swap the two every time
    // either agent reached for a tool.
    const first = turn({ id: "turn-1", nickname: "arch" });
    const second = turn({ id: "turn-2", nickname: "sec" });
    await mount({ turns: [first, second] });
    const order = () =>
      screen
        .getAllByTestId("comment-pending")
        .map((node) => node.getAttribute("data-turn-id"));
    expect(order()).toEqual(["turn-1", "turn-2"]);

    // The OLDER turn reaches for a tool.
    emitTurn({
      ...first,
      activeToolCalls: [{ id: "call-1", tool: "read_file", activationSeq: 1 }],
    });
    expect(order()).toEqual(["turn-1", "turn-2"]);
    expect(
      screen.getAllByTestId("comment-pending-status").map((n) => n.textContent),
    ).toEqual(["Reading a project file…", "Thinking…"]);

    // …and finishes it. Still in place.
    emitTurn({ ...first, activeToolCalls: [] });
    expect(order()).toEqual(["turn-1", "turn-2"]);
  });
});

describe("CTA-FR-ZOLW / CTA-FR-QTNB: what a pending contribution becomes", () => {
  it("renders one pending contribution when the event beats the dispatch reply", async () => {
    // The backend publishes the registration event *before*
    // `dispatch_agent_turn` resolves, and there is no ordering between the two.
    // A rail that appended on both would render one outstanding turn twice.
    const b = await mount();
    await userEvent.type(composer(), "@arch thoughts?");
    await userEvent.click(screen.getByRole("button", { name: "Post" }));
    await screen.findByTestId("comment-pending");
    const turnId = screen
      .getByTestId("comment-pending")
      .getAttribute("data-turn-id")!;

    // The same turn arriving again on the event channel, as it does in the app.
    emitTurn(turn({ id: turnId, nickname: "arch" }));
    expect(screen.getAllByTestId("comment-pending")).toHaveLength(1);
    expect(calls(b, "dispatch_agent_turn")).toHaveLength(1);
  });

  it("renders what is outstanding when the artifact opens", async () => {
    // CTA-FR-QXIG, CTA-FR-FSBH's first clause.
    const b = await mount({ turns: [turn({ id: "turn-9", nickname: "arch" })] });
    await waitFor(() => expect(calls(b, "list_agent_turns")).toHaveLength(1));
    const pending = await screen.findByTestId("comment-pending");
    expect(pending).toHaveTextContent("arch");
  });

  it("replaces the pending contribution with the agent's comment on delivery", async () => {
    // CTA-FR-QXIG, CTA-FR-FSBH's second clause / CTA-FR-QTNB.
    const b = await mount();
    await userEvent.type(composer(), "@arch thoughts?");
    await userEvent.click(screen.getByRole("button", { name: "Post" }));
    const pending = await screen.findByTestId("comment-pending");
    const turnId = pending.getAttribute("data-turn-id")!;

    // The agent wrote through the backend's own append path, so the comment
    // reaches the rail on `"discussion changed"` (CMS-FR-51) — the turn
    // event only clears the pending contribution.
    const delivered: Discussion = {
      ...b.threads[0],
      comments: [
        ...b.threads[0].comments,
        {
          id: "c-agent",
          author: { kind: "agent", agentId: "a1", handle: "arch", model: "m" },
          body: "Two specs, I think.",
          quotes: [],
          attachments: [],
          createdAt: "2026-01-03T00:00:00Z",
        },
      ],
    };
    b.threads = [delivered];
    emitThread(delivered);
    emitTurn(
      turn({
        id: turnId,
        nickname: "arch",
        state: "delivered",
        endedAt: "2026-01-03T00:00:00Z",
      }),
    );

    expect(await screen.findByText("Two specs, I think.")).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.queryByTestId("comment-pending")).not.toBeInTheDocument(),
    );
  });

  it("removes the pending contribution and states the failure, adding no comment", async () => {
    // CTA-FR-QXIG, CTA-FR-FSBH's third clause / CTA-FR-QTNB, CTA-FR-OBRO.
    await mount();
    await userEvent.type(composer(), "@arch thoughts?");
    await userEvent.click(screen.getByRole("button", { name: "Post" }));
    const pending = await screen.findByTestId("comment-pending");
    const turnId = pending.getAttribute("data-turn-id")!;
    const commentsBefore = screen.getAllByText(/Which session\?|@arch thoughts\?/).length;

    emitTurn(
      turn({
        id: turnId,
        nickname: "arch",
        state: "failed",
        failure: "unreachable",
        endedAt: "2026-01-03T00:00:00Z",
      }),
    );

    expect(await screen.findByTestId("comment-turn-error")).toHaveTextContent(
      /could not be reached/i,
    );
    expect(screen.queryByTestId("comment-pending")).not.toBeInTheDocument();
    expect(
      screen.getAllByText(/Which session\?|@arch thoughts\?/).length,
    ).toBe(commentsBefore);
  });

  it("cancels an outstanding turn, leaving the thread's comments untouched", async () => {
    // CTA-FR-QXIG, CTA-FR-FSBH's last clause / CTA-FR-ZOLW.
    const b = await mount();
    await userEvent.type(composer(), "@arch thoughts?");
    await userEvent.click(screen.getByRole("button", { name: "Post" }));
    await screen.findByTestId("comment-pending");

    await userEvent.click(screen.getByTestId("comment-pending-cancel"));
    await waitFor(() => expect(calls(b, "cancel_agent_turn")).toHaveLength(1));
    expect(screen.queryByTestId("comment-pending")).not.toBeInTheDocument();
    expect(screen.getByText("Which session?")).toBeInTheDocument();
  });

  it("reports a refused dispatch without unwinding the post", async () => {
    // CTA-FR-FKLG: the author's message stands whether or not any agent answers.
    const b = await mount({ dispatchError: "agent_unavailable" });
    await userEvent.type(composer(), "@arch thoughts?");
    await userEvent.click(screen.getByRole("button", { name: "Post" }));

    await waitFor(() => expect(calls(b, "add_comment")).toHaveLength(1));
    expect(await screen.findByTestId("comment-turn-error")).toHaveTextContent(
      /no longer available/i,
    );
    // The body is rendered with its tag marked (AGT-FR-29), so it spans more
    // than one element — the card's text is what the author sees.
    expect(
      screen.getByTestId("comment-thread-t1").textContent ?? "",
    ).toContain("@arch thoughts?");
  });
});

describe("AGT-FR-30 / CMT-FR-32: the picker is the card's transient surface", () => {
  it("is dismissed by the card's overflow menu, as the card's other transients are", async () => {
    // AGT-FR-30, SNV-FR-56, CMT-FR-32's second half. The picker is not a floating overlay of the
    // window, so it takes part in the *card's* one-transient-at-a-time rule
    // rather than the window's.
    await mount();
    await userEvent.type(composer(), "@");
    await screen.findByTestId("mention-picker");

    await userEvent.click(
      screen.getByRole("button", { name: "Thread actions for t1" }),
    );
    await waitFor(() =>
      expect(screen.queryByTestId("mention-picker")).not.toBeInTheDocument(),
    );
  });
});

describe("CTA-FR-OBRO: a pending contribution is not part of the conversation", () => {
  it("is counted by no unresolved-thread total and matches nothing in Find", async () => {
    // CTA-FR-XKRY / CTA-FR-OBRO, CMT-FR-29, CMT-FR-31.
    await mount();
    const toggle = () =>
      screen.getByRole("button", { name: /comments \(\d+ unresolved\)/i });
    const before = toggle().getAttribute("title");

    await userEvent.type(composer(), "@arch thoughts?");
    await userEvent.click(screen.getByRole("button", { name: "Post" }));
    await screen.findByTestId("comment-pending");

    expect(toggle()).toHaveAttribute("title", before!);
    // It lives in the rail, which is outside the artifact body Find searches.
    const body = screen.getByLabelText("artifact body");
    expect(body.textContent ?? "").not.toContain("Thinking");
  });
});

describe("CTA-FR-YGYP / AGT-FR-35 … AGT-FR-38: @all from a card", () => {
  it("dispatches once per agent that can answer, and not to a degraded one", async () => {
    // CTA-FR-YGYP. Driven through the real rail because the pure rule is only half
    // the feature: `useComments` has to hand `dispatchTargets` the enrolment
    // *with its availability*. Flatten that to a bare nickname list at
    // the call site and `@all` reaches an agent whose provider is gone — the exact
    // failure AGT-FR-36 exists to prevent, and one no pure test can see.
    const b = await mount({
      agents: [
        projectAgent("arch", "a1"),
        projectAgent("sec", "a2"),
        projectAgent("scribe", "a3", "provider_unverified"),
      ],
    });

    await userEvent.type(composer(), "@all is this two specs?");
    await userEvent.click(screen.getByRole("button", { name: "Post" }));

    // CTA-FR-EACI: appended first and unconditionally, then dispatched.
    await waitFor(() => expect(calls(b, "add_comment")).toHaveLength(1));
    await waitFor(() => expect(calls(b, "dispatch_agent_turn")).toHaveLength(2));
    expect(
      calls(b, "dispatch_agent_turn").map(
        (c) => (c.args as { nickname: string }).nickname,
      ),
    ).toEqual(["arch", "sec"]);
  });

  it("asks an agent once when the handle and its nickname are both written", async () => {
    // CTA-FR-YGYP / CTA-FR-FAWE: one turn per distinct agent, not per tag.
    const b = await mount();
    await userEvent.type(composer(), "@all @arch again please");
    await userEvent.click(screen.getByRole("button", { name: "Post" }));

    await waitFor(() => expect(calls(b, "add_comment")).toHaveLength(1));
    await waitFor(() => expect(calls(b, "dispatch_agent_turn")).toHaveLength(2));
    expect(
      calls(b, "dispatch_agent_turn").map(
        (c) => (c.args as { nickname: string }).nickname,
      ),
    ).toEqual(["arch", "sec"]);
  });

  it("appends the comment and dispatches nothing where nobody can answer", async () => {
    // CTA-FR-YGYP / AGT-FR-38: an ordinary comment in every respect.
    const b = await mount({ agents: [] });
    await userEvent.type(composer(), "@all thoughts?");
    await userEvent.click(screen.getByRole("button", { name: "Post" }));

    await waitFor(() => expect(calls(b, "add_comment")).toHaveLength(1));
    expect(calls(b, "dispatch_agent_turn")).toHaveLength(0);
  });

  it("bolds a live tag in the card and leaves every dead @ as prose", async () => {
    // AGT-FR-24 / AGT-FR-28, AGT-FR-38 / AGT-FR-29, in the rail — the surface the
    // requirement names first. `CommentRail` has to hand the renderer the roster;
    // pass it an empty one and the marking silently disappears.
    await mount({
      threads: [
        makeThread({
          comments: [
            {
              id: "c1",
              author: human("raver119"),
              body: "@all and @arch, plus me@example.com, @media and @nobody",
              quotes: [],
              attachments: [],
              createdAt: "2026-01-01T00:00:00Z",
            },
          ],
        }),
      ],
    });

    const marked = await screen.findAllByTestId("comment-tag");
    expect(marked.map((e) => e.textContent)).toEqual(["@all", "@arch"]);
    // The rest keep the weight of the prose around them (AGT-FR-28).
    const card = marked[0].closest(".comment-card") as HTMLElement;
    expect(card.textContent).toContain("me@example.com");
    expect(card.textContent).toContain("@media");
    expect(card.textContent).toContain("@nobody");
  });

  it("leaves @all as prose in a project enrolling nobody", async () => {
    // AGT-FR-38: the same body, marked in one project and not in another.
    await mount({
      agents: [],
      threads: [
        makeThread({
          comments: [
            {
              id: "c1",
              author: human("raver119"),
              body: "@all take a look",
              quotes: [],
              attachments: [],
              createdAt: "2026-01-01T00:00:00Z",
            },
          ],
        }),
      ],
    });
    await screen.findByRole("complementary", { name: "Comments" });
    expect(screen.queryAllByTestId("comment-tag")).toHaveLength(0);
    expect(document.body.textContent).toContain("@all take a look");
  });
});

describe("CVP-FR-47, CTA-FR-ZOLW, CMT-FR-HQNV: a pending placeholder outlives the rail", () => {
  it("CVP-FR-47: shows the same one placeholder when the rail is hidden and shown again", async () => {
    await mount();
    await userEvent.type(composer(), "@arch thoughts?");
    await userEvent.click(screen.getByRole("button", { name: "Post" }));
    await screen.findByTestId("comment-pending");

    const toggle = () =>
      screen.getByRole("button", { name: /comments \(\d+ unresolved\)/i });
    await userEvent.click(toggle());
    await waitFor(() =>
      expect(screen.queryByRole("complementary", { name: "Comments" })).toBeNull(),
    );
    expect(screen.queryByTestId("comment-pending")).toBeNull();

    await userEvent.click(toggle());
    await screen.findByRole("complementary", { name: "Comments" });
    // One placeholder and one card: the surface of the discussion is not doubled.
    expect(await screen.findAllByTestId("comment-pending")).toHaveLength(1);
    expect(screen.getAllByTestId("comment-thread-t1")).toHaveLength(1);
  });

  it("CVP-FR-47: keeps the unsent text of a reply across the same round trip", async () => {
    await mount();
    await userEvent.type(composer(), "half written");
    const toggle = () =>
      screen.getByRole("button", { name: /comments \(\d+ unresolved\)/i });
    await userEvent.click(toggle());
    await userEvent.click(toggle());
    await screen.findByRole("complementary", { name: "Comments" });
    expect(composer()).toHaveValue("half written");
  });
});
