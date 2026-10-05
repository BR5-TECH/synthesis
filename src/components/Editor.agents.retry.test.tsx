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
  fragment,
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

describe("CTA-FR-MGVJ … CTA-FR-XZUO: the failed contribution and its Retry", () => {
  it("CTA-FR-AZEU, CTA-FR-HBAB: reads Thinking… throughout, whatever the retries are doing", async () => {
    // The rail cannot see an attempt at all: a turn repeating a model call is
    // still `running`, and nothing about a retry reaches a surface (CVL-FR-20).
    // What this witnesses is that the card renders the running turn as a pending
    // contribution and shows no failure and no count while it does.
    const b = await mount({ turns: [turn({ id: "turn-1", nickname: "arch" })] });
    const pending = await screen.findByTestId("comment-pending");
    expect(pending.textContent).toContain("Thinking…");
    expect(pending.textContent).not.toMatch(/attempt|retry|1 of 3/i);
    expect(
      within(pending).getByTestId("comment-pending-cancel"),
    ).toBeInTheDocument();
    expect(screen.queryByTestId("comment-failed")).not.toBeInTheDocument();
    // Read by each conversational surface the tab mounts, exactly as
    // `list_agent_turns` is (ACT-FR-22).
    expect(
      calls(b, "list_recoverable_agent_turn_failures").length,
    ).toBeGreaterThanOrEqual(1);
  });

  it("CTA-FR-ZDDI: replaces the pending contribution in place when the attempts run out", async () => {
    await mount({ turns: [turn({ id: "turn-1", nickname: "arch" })] });
    await screen.findByTestId("comment-pending");

    emitTurn(failedTurn());

    const failed = await screen.findByTestId("comment-failed");
    // Attributed to the same agent, in the position the answer would have taken.
    expect(failed.textContent).toContain("arch");
    expect(failed.textContent).toContain("could not produce a response");
    // Distinguishable from a delivered comment and from a pending one by more
    // than colour: its own marker, and its own container.
    expect(failed.querySelector('[data-icon="alert-triangle"]')).toBeTruthy();
    expect(failed).toHaveClass("comment--failed");
    expect(screen.queryByTestId("comment-pending")).not.toBeInTheDocument();
    // It names no failure code and no provider — the diagnosis is in the log.
    expect(failed.textContent).not.toMatch(/unreachable|openrouter|timed_out/i);
    // The thread's comments are exactly what they were.
    expect(postedComments()).toHaveLength(1);
  });

  it("CTA-FR-QDDG, CTA-FR-XNBS, CTA-FR-CERW: Retry starts a fresh turn, posts nothing, and disables while it dispatches", async () => {
    const b = await mount({ recoverable: [failedTurn()] });
    const retry = await screen.findByTestId("comment-failed-retry");

    const user = userEvent.setup();
    await user.click(retry);

    await waitFor(() => expect(calls(b, "retry_agent_turn")).toHaveLength(1));
    expect(calls(b, "retry_agent_turn")[0].args).toEqual({ turnId: "turn-1" });
    // CTA-FR-XNBS: it must not append or repost the human comment.
    expect(calls(b, "add_comment")).toHaveLength(0);
    expect(calls(b, "dispatch_agent_turn")).toHaveLength(0);
    // The failed contribution is gone; the new turn's own event supplies the
    // pending contribution that takes its place.
    await waitFor(() =>
      expect(screen.queryByTestId("comment-failed")).not.toBeInTheDocument(),
    );
    emitTurn(turn({ id: "turn-retry-1", nickname: "arch" }));
    expect(await screen.findByTestId("comment-pending")).toBeInTheDocument();
  });

  it("CTA-FR-QDDG, CTA-FR-XNBS, CTA-FR-CERW: one activation is one turn however impatiently it is pressed", async () => {
    const b = await mount({ recoverable: [failedTurn()] });
    const retry = await screen.findByTestId("comment-failed-retry");

    // Three clicks in the same tick, before any state has settled.
    await act(async () => {
      retry.click();
      retry.click();
      retry.click();
    });

    await waitFor(() => expect(calls(b, "retry_agent_turn")).toHaveLength(1));
  });

  it("CTA-FR-ESNJ: the control keeps focus while it dispatches and after a refusal", async () => {
    // A `disabled` button is not focusable, so disabling one under the author's
    // own keyboard blurs it and focus lands on `<body>` with nothing to return
    // it — leaving a keyboard user tabbing from the top of the document to reach
    // the very offer they were already on.
    const b = await mount({
      recoverable: [failedTurn()],
      retryError: "discussion_locked",
    });
    const retry = await screen.findByTestId("comment-failed-retry");
    retry.focus();
    expect(document.activeElement).toBe(retry);

    await userEvent.setup().click(retry);
    await waitFor(() => expect(calls(b, "retry_agent_turn")).toHaveLength(1));

    // The offer is back, and the keyboard never left it.
    expect(await screen.findByTestId("comment-failed")).toBeInTheDocument();
    expect(document.activeElement).toBe(
      screen.getByTestId("comment-failed-retry"),
    );
    expect(document.activeElement).not.toBe(document.body);
  });

  it("CTA-FR-QDDG, CTA-FR-TJCA, CMT-FR-34, CMT-FR-15: a refused retry restores the failure and states the reason", async () => {
    const b = await mount({
      recoverable: [failedTurn()],
      retryError: "discussion_locked",
    });
    const user = userEvent.setup();
    await user.click(await screen.findByTestId("comment-failed-retry"));

    await waitFor(() => expect(calls(b, "retry_agent_turn")).toHaveLength(1));
    // The offer is exactly where it was, so it can be taken again.
    expect(await screen.findByTestId("comment-failed")).toBeInTheDocument();
    // And the typed refusal renders inline at the foot of that card.
    await waitFor(() =>
      expect(
        card().querySelector(".comment-card__error")?.textContent,
      ).toBeTruthy(),
    );
    expect(calls(b, "add_comment")).toHaveLength(0);
  });

  it("CTA-FR-QDDG, CTA-FR-TJCA, CMT-FR-34, CMT-FR-15: a locked thread renders the failure and withholds Retry", async () => {
    await mount({
      threads: [makeThread({ locked: true })],
      recoverable: [failedTurn()],
    });
    const failed = await screen.findByTestId("comment-failed");
    expect(failed.textContent).toContain("could not produce a response");
    expect(
      screen.queryByTestId("comment-failed-retry"),
    ).not.toBeInTheDocument();
  });

  it("CTA-FR-CSZT, CMT-FR-16, CTA-FR-QTNB: a resolved thread keeps Retry although it carries no composer", async () => {
    await mount({
      threads: [makeThread({ resolved: true })],
      recoverable: [failedTurn()],
    });
    // The resolved disclosure is where a resolved card is reached.
    const disclosure = screen.getByRole("button", { name: /resolved thread/i });
    await userEvent.setup().click(disclosure);

    expect(await screen.findByTestId("comment-failed")).toBeInTheDocument();
    expect(screen.getByTestId("comment-failed-retry")).toBeInTheDocument();
  });

  it("CTA-FR-STWA: recovers the offer when the card is remounted", async () => {
    const b = await mount({ recoverable: [failedTurn()] });
    expect(await screen.findByTestId("comment-failed")).toBeInTheDocument();
    const before = calls(b, "list_recoverable_agent_turn_failures").length;
    expect(before).toBeGreaterThanOrEqual(1);

    // Genuinely torn down and rebuilt, which is the claim: the registry is what
    // carries the offer across, no event having been emitted for a turn that
    // failed before this surface existed.
    cleanup();
    await remount(b);

    expect(
      calls(b, "list_recoverable_agent_turn_failures").length,
    ).toBeGreaterThan(before);
    expect(await screen.findByTestId("comment-failed")).toBeInTheDocument();
    expect(screen.getByTestId("comment-failed-retry")).toBeInTheDocument();
  });

  it("CTA-FR-STWA: a relaunch finds neither the turn nor the offer", async () => {
    // AGC-FR-23 / CTA-FR-SWXX: the registry is memory of the running application,
    // so a relaunch is it answering with nothing — and the thread reads exactly
    // as it did before the agent was addressed.
    await mount({ recoverable: [] });
    expect(screen.queryByTestId("comment-failed")).not.toBeInTheDocument();
    expect(postedComments()).toHaveLength(1);
  });

  it("CTA-FR-XZUO: a lock arriving while the failure shows takes the control, not the contribution", async () => {
    // A lock and a resolution announce themselves on the same channel a comment
    // does, with the comment list untouched — and in the ordinary case the
    // thread's last comment is the author's own, the failed turn having appended
    // nothing (AGC-FR-18). Treating that as "a later human comment" would retire
    // the offer outright, where CTA-FR-XZUO says a locked thread loses its Retry
    // *in place*.
    await mount({ recoverable: [failedTurn()] });
    await screen.findByTestId("comment-failed");
    expect(screen.getByTestId("comment-failed-retry")).toBeInTheDocument();

    emitThread(makeThread({ locked: true }));

    await waitFor(() =>
      expect(
        screen.queryByTestId("comment-failed-retry"),
      ).not.toBeInTheDocument(),
    );
    expect(screen.getByTestId("comment-failed")).toBeInTheDocument();

    emitThread(makeThread({ locked: false }));
    expect(await screen.findByTestId("comment-failed-retry")).toBeInTheDocument();
  });

  it("CTA-FR-ESNJ: retrying one conversation leaves another's offer takeable", async () => {
    // The disable is on *the* control whose dispatch is initiating, not on every
    // Retry the rail happens to be rendering.
    const second = makeThread({
      id: "t2",
      fragmentTarget: fragment({ start: 0, end: 12, quote: "# Onboarding" }),
    });
    const b = await mount({
      threads: [makeThread(), second],
      recoverable: [
        failedTurn(),
        failedTurn({
          id: "turn-2",
          nickname: "sec",
          origin: artifactCommentOrigin("t2"),
        }),
      ],
    });

    // Held open, so the second offer is exercised while the first dispatch is
    // genuinely still in flight — the only window in which the disable of
    // CTA-FR-ESNJ is observable at all.
    let release!: () => void;
    b.retryGate = new Promise<void>((resolve) => {
      release = resolve;
    });

    await waitFor(() =>
      expect(screen.getAllByTestId("comment-failed-retry")).toHaveLength(2),
    );
    const [first, other] = screen.getAllByTestId("comment-failed-retry");
    const user = userEvent.setup();
    await user.click(first);
    await waitFor(() => expect(calls(b, "retry_agent_turn")).toHaveLength(1));

    // Its own control is off while it dispatches — `aria-disabled`, so it keeps
    // focus rather than dropping it to the document body (CTA-FR-ESNJ).
    await waitFor(() => expect(first).toHaveAttribute("aria-disabled", "true"));
    // ...and the other conversation's is not.
    expect(other).toHaveAttribute("aria-disabled", "false");
    await user.click(other);
    await waitFor(() => expect(calls(b, "retry_agent_turn")).toHaveLength(2));

    await act(async () => {
      release();
      await Promise.resolve();
    });
  });

  it("CTA-FR-RHPP, CTA-FR-OUYQ: a later recoverable failure replaces the one before it", async () => {
    await mount({ recoverable: [failedTurn()] });
    await screen.findByTestId("comment-failed");
    expect(screen.getByTestId("comment-failed").textContent).toContain("arch");

    emitTurn(
      failedTurn({ id: "turn-2", nickname: "sec", failure: "empty_reply" }),
    );

    await waitFor(() => {
      const cards = screen.getAllByTestId("comment-failed");
      expect(cards).toHaveLength(1);
      expect(cards[0].textContent).toContain("sec");
    });
  });

  it("CTA-FR-RHPP, CTA-FR-OUYQ: an agent's answer leaves the offer standing and a human comment retires it", async () => {
    await mount({ recoverable: [failedTurn()] });
    await screen.findByTestId("comment-failed");

    // An agent-authored comment retires nothing.
    emitThread(
      makeThread({
        comments: [
          ...makeThread().comments,
          {
            id: "c2",
            author: { kind: "agent", agentId: "a2", handle: "sec" },
            body: "sec's answer",
            quotes: [],
            attachments: [],
            createdAt: "2026-01-02T00:00:00Z",
          },
        ],
      }),
    );
    expect(screen.getByTestId("comment-failed")).toBeInTheDocument();

    // A later human comment does.
    emitThread(
      makeThread({
        comments: [
          ...makeThread().comments,
          {
            id: "c3",
            author: human("raver119"),
            body: "never mind",
            quotes: [],
            attachments: [],
            createdAt: "2026-01-03T00:00:00Z",
          },
        ],
      }),
    );
    await waitFor(() =>
      expect(screen.queryByTestId("comment-failed")).not.toBeInTheDocument(),
    );
  });

  it("CTA-FR-XKRY, CMT-FR-31, CMT-FR-29: a failed contribution is not part of the conversation", async () => {
    await mount({ recoverable: [failedTurn()] });
    await screen.findByTestId("comment-failed");

    // Not a comment: the thread holds exactly the one that was posted, and the
    // unresolved count is what it was before the agent was addressed.
    expect(postedComments()).toHaveLength(1);
    expect(
      screen.getByRole("button", { name: /comments \(1 unresolved\)/i }),
    ).toBeInTheDocument();
  });

  it("CTA-FR-ZDDI: a non-recoverable failure renders inline with no Retry", async () => {
    await mount({ turns: [turn({ id: "turn-1", nickname: "arch" })] });
    await screen.findByTestId("comment-pending");

    emitTurn(
      turn({
        id: "turn-1",
        nickname: "arch",
        state: "failed",
        failure: "rejected",
        retryPermitted: false,
        endedAt: "2026-01-01T00:01:00Z",
      }),
    );

    await waitFor(() =>
      expect(screen.queryByTestId("comment-pending")).not.toBeInTheDocument(),
    );
    expect(screen.queryByTestId("comment-failed")).not.toBeInTheDocument();
    expect(screen.queryByTestId("comment-failed-retry")).not.toBeInTheDocument();
    expect(screen.getByTestId("comment-turn-error").textContent).toBeTruthy();
  });
});

// ---------------------------------------------------------------------------
// CTA-FR-GFZE / CTA-FR-LVPC — the unsupported-image notice (CTA-FR-ARBB)
// ---------------------------------------------------------------------------

describe("CTA-FR-ARBB: a turn whose images could not be sent", () => {
  /** A delivered turn that sent metadata in place of its pictures. */
  function omittedTurn(over: Partial<AgentTurn> = {}): AgentTurn {
    return turn({
      id: "turn-1",
      nickname: "arch",
      state: "delivered",
      endedAt: "2026-01-01T00:01:00Z",
      imagesOmitted: true,
      ...over,
    });
  }

  it("CTA-FR-ARBB, CTA-FR-GFZE: renders a notice beside the contribution, announced and legible without colour", async () => {
    await mount({ turns: [turn({ id: "turn-1", nickname: "arch" })] });
    await screen.findByTestId("comment-pending");

    emitTurn(omittedTurn());

    const notice = await screen.findByTestId("comment-image-notice");
    // The exact words CTA-FR-GFZE fixes.
    expect(notice.textContent).toBe(
      "Images were not sent; text and image metadata were sent instead",
    );
    // Announced through a status role, so a screen-reader user learns of it
    // without hunting for it.
    expect(notice).toHaveAttribute("role", "status");
    // Reachable and readable from the keyboard alone.
    expect(notice).toHaveAttribute("tabindex", "0");
    // A status and not an error: it is not rendered as a blocking one, and the
    // answer is an ordinary comment rather than a failed contribution.
    expect(notice.getAttribute("role")).not.toBe("alert");
    expect(screen.queryByTestId("comment-failed")).not.toBeInTheDocument();
    // It sits with the agent's own contribution, so a reader can tell which
    // turn it is about.
    expect(notice.getAttribute("data-turn-id")).toBe("turn-1");
    // Legible without colour: the words say it, rather than a tone.
    expect(notice.textContent).toMatch(/were not sent/);
  });

  it("CTA-FR-ARBB, CTA-FR-GFZE: renders again from the registry when the conversation is mounted afresh", async () => {
    // The turn ended before this surface existed, so nothing about it arrived
    // on an event — the notice comes from `"list agent turn image notices"`.
    await mount({ imageNotices: [omittedTurn()] });
    const notice = await screen.findByTestId("comment-image-notice");
    expect(notice.textContent).toMatch(/Images were not sent/);
  });

  it("CTA-FR-ARBB, CTA-FR-GFZE: a later turn that carried its images clears the notice", async () => {
    await mount({ imageNotices: [omittedTurn()] });
    await screen.findByTestId("comment-image-notice");

    // The author changed the agent's model to one that takes images and asked
    // again: that turn shows no notice and the conversation carries none.
    emitTurn(omittedTurn({ id: "turn-2", imagesOmitted: false }));
    await waitFor(() =>
      expect(screen.queryByTestId("comment-image-notice")).not.toBeInTheDocument(),
    );
  });

  it("CTA-FR-LVPC: blocks nothing — a failed turn still carries its Retry beside it", async () => {
    await mount({
      imageNotices: [omittedTurn()],
      recoverable: [failedTurn({ id: "turn-9" })],
    });
    await screen.findByTestId("comment-image-notice");
    // The failed contribution still carries its Retry, unaffected by the notice.
    expect(await screen.findByTestId("comment-failed-retry")).toBeInTheDocument();
  });

  it("CTA-FR-LVPC: no comment is marked by it and nothing posts it into the thread", async () => {
    const b = await mount({ imageNotices: [omittedTurn()] });
    await screen.findByTestId("comment-image-notice");
    // The notice is held by the rail alone: no comment was appended for it, and
    // no reader of the conversation ever sees it (CTA-FR-NCVL).
    expect(calls(b, "add_comment")).toHaveLength(0);
    for (const comment of postedComments()) {
      expect(comment.textContent).not.toMatch(/Images were not sent/);
    }
  });
});
