/**
 * A post of the author returns the list to its foot
 * (`CVP-conversation-presentation.md` CVP-FR-CUKV, CVP-FR-TWRL, CVP-FR-47).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (cmd: string) => {
    if (cmd === "append_log_records") return undefined;
    throw new Error(`unexpected command ${cmd}`);
  }),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
  emit: vi.fn(async () => {}),
}));

/** The handle the composer gives the voice controller, for a spoken answer. */
let voiceHandle: { postAnswer: (text: string) => Promise<string> } | null = null;
vi.mock("../../voice/voiceController", async (original) => ({
  ...(await original<typeof import("../../voice/voiceController")>()),
  registerVoiceComposer: (_key: string, handle: typeof voiceHandle) => {
    voiceHandle = handle;
    return () => {
      if (voiceHandle === handle) voiceHandle = null;
    };
  },
}));

import { DiscussionSurface, type DiscussionSurfaceProps } from "./DiscussionSurface";
import { resetLogBufferForTest } from "../../logging";
import { resetDiscussionFocus } from "../../state/discussionFocus";
import {
  clearAllDiscussionSessions,
  clearDiscussionSession,
  getDiscussionSession,
  requestDiscussionFoot,
  setDiscussionScroll,
  setDiscussionSeen,
} from "../../state/discussionSession";
import { clearQuestionSets } from "../../state/questionSets";
import { installFakeResizeObserver, installFakeScroll, limitScroll } from "../../test/fakeScroll";
import type { Comment, Discussion, Participant } from "../../types";

const human: Participant = { kind: "human", login: "raver119" };
const arch: Participant = { kind: "agent", agentId: "a1", handle: "arch", title: "Architect" };

function comment(id: string, body: string, author: Participant = human): Comment {
  return {
    id,
    author,
    body,
    quotes: [],
    attachments: [],
    createdAt: "2026-02-01T00:00:00Z",
  };
}

const DISCUSSION: Discussion = {
  id: "d1",
  target: { kind: "artifact", artifactId: "a.md" },
  fragmentTarget: null,
  comments: [comment("c1", "first"), comment("c2", "second", arch)],
  resolved: false,
  createdAt: "2026-02-01T00:00:00Z",
  updatedAt: "2026-02-01T00:00:00Z",
};

function surface(d: Discussion, over: Partial<DiscussionSurfaceProps> = {}) {
  return (
    <DiscussionSurface
      discussion={d}
      owner="rail"
      agents={[]}
      liveTurns={false}
      onReply={vi.fn(async () => undefined)}
      onSetResolved={vi.fn(async () => undefined)}
      {...over}
    />
  );
}

/** A scroller of 1000 in a box of 200, with a clamped offset. */
function layOut(el: HTMLElement) {
  const box = { height: 1000, client: 200, top: 0 };
  Object.defineProperty(el, "scrollHeight", { configurable: true, get: () => box.height });
  Object.defineProperty(el, "clientHeight", { configurable: true, get: () => box.client });
  Object.defineProperty(el, "scrollTop", {
    configurable: true,
    get: () => box.top,
    set: (v: number) => {
      box.top = Math.max(0, Math.min(v, box.height - box.client));
    },
  });
  return box;
}

const list = () => screen.getByTestId("discussion-messages");

function readBack(box: { top: number }) {
  list().scrollTop = 100;
  fireEvent.scroll(list());
  expect(box.top).toBe(100);
  expect(getDiscussionSession("d1").atTail).toBe(false);
}

const withReply = (id: string, author: Participant = human): Discussion => ({
  ...DISCUSSION,
  comments: [...DISCUSSION.comments, comment(id, "new", author)],
});

let restore: () => void;

beforeEach(() => {
  resetLogBufferForTest();
  clearAllDiscussionSessions();
  clearQuestionSets();
  resetDiscussionFocus();
  restore = installFakeResizeObserver();
});

afterEach(() => {
  cleanup();
  restore();
  vi.restoreAllMocks();
});

describe("a post of the author", () => {
  it("CVP-FR-CUKV: a request that comes after the arrivals goes to the foot and clears the unread state", () => {
    const view = render(surface(DISCUSSION));
    const box = layOut(list());
    readBack(box);
    view.rerender(surface(withReply("c3")));
    expect(screen.getByRole("button", { name: /1 new message/ })).toBeInTheDocument();

    act(() => requestDiscussionFoot("d1"));

    expect(box.top).toBe(800);
    expect(getDiscussionSession("d1")).toMatchObject({
      atTail: true,
      unreadCount: 0,
      firstUnreadId: null,
    });
    expect(screen.queryByTestId("discussion-unread-indicator")).toBeNull();
    expect(screen.queryByRole("separator", { name: /unread/ })).toBeNull();
  });

  it("CVP-FR-CUKV: a request that comes before the arrivals makes them followed", () => {
    const view = render(surface(DISCUSSION));
    const box = layOut(list());
    readBack(box);

    act(() => requestDiscussionFoot("d1"));
    expect(box.top).toBe(800);
    box.height = 1300;
    view.rerender(surface(withReply("c3")));
    view.rerender(
      surface({ ...withReply("c3"), comments: [...withReply("c3").comments, comment("c4", "x", arch)] }),
    );

    expect(box.top).toBe(1100);
    expect(getDiscussionSession("d1").unreadCount).toBe(0);
    expect(screen.queryByTestId("discussion-unread-indicator")).toBeNull();
  });

  it("CVP-FR-CUKV, CVP-FR-SWDH: the move to the foot is the list's own, so its scroll event keeps the follow", () => {
    render(surface(DISCUSSION));
    const box = layOut(list());
    readBack(box);
    act(() => requestDiscussionFoot("d1"));
    // The content grows before the event of that move is handled, so the event
    // reads a list that is no longer at its foot.
    box.height = 1300;
    fireEvent.scroll(list());
    expect(getDiscussionSession("d1").atTail).toBe(true);
    expect(box.top).toBe(1100);
  });

  it("CVP-FR-CUKV: a request for another discussion moves nothing", () => {
    render(surface(DISCUSSION));
    const box = layOut(list());
    readBack(box);
    act(() => requestDiscussionFoot("d2"));
    expect(box.top).toBe(100);
    expect(getDiscussionSession("d1").atTail).toBe(false);
  });

  it("CVP-FR-CUKV, CVP-FR-47: a request made while no surface was mounted is not repeated by a surface that mounts later", () => {
    requestDiscussionFoot("d1");
    // The author scrolled away after that request, in a surface since closed.
    setDiscussionScroll("d1", 100, false);
    setDiscussionSeen("d1", ["c1", "c2"]);
    // The list is laid out before it mounts, so a move at mount is seen.
    const undoScroll = installFakeScroll();
    limitScroll('[data-testid="discussion-messages"]', 800);
    const height = vi.spyOn(HTMLElement.prototype, "scrollHeight", "get").mockReturnValue(1000);
    const client = vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(200);
    try {
      render(surface(DISCUSSION));
      expect(list().scrollTop).toBe(100);
      expect(getDiscussionSession("d1").atTail).toBe(false);
    } finally {
      height.mockRestore();
      client.mockRestore();
      undoScroll();
    }
  });

  it("CVP-FR-CUKV: a session cleared while its list is mounted is no post, and moves nothing", () => {
    const view = render(surface(DISCUSSION));
    const box = layOut(list());
    act(() => requestDiscussionFoot("d1"));
    readBack(box);
    act(() => clearDiscussionSession("d1"));
    view.rerender(surface(DISCUSSION));
    expect(box.top).toBe(100);
  });

  it("CVP-FR-CUKV: a reply posted from the composer returns the list to the foot", async () => {
    const onReply = vi.fn(async () => undefined);
    render(surface(DISCUSSION, { onReply }));
    const box = layOut(list());
    readBack(box);

    fireEvent.change(screen.getByRole("textbox", { name: "Reply to thread d1" }), {
      target: { value: "my reply" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Post" }));

    await waitFor(() => expect(box.top).toBe(800));
    expect(onReply).toHaveBeenCalledTimes(1);
    expect(getDiscussionSession("d1").atTail).toBe(true);
  });

  it("CVP-FR-CUKV, CMT-FR-34: a refused reply leaves the list where it is", async () => {
    const onReply = vi.fn(async () => {
      throw new Error("refused");
    });
    render(surface(DISCUSSION, { onReply }));
    const box = layOut(list());
    readBack(box);

    fireEvent.change(screen.getByRole("textbox", { name: "Reply to thread d1" }), {
      target: { value: "my reply" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Post" }));

    await waitFor(() => expect(getDiscussionSession("d1").error).toBeDefined());
    expect(box.top).toBe(100);
    expect(getDiscussionSession("d1").atTail).toBe(false);
    expect(getDiscussionSession("d1").footRequest).toBe(0);
  });

  it("CVP-FR-CUKV: a spoken answer returns the list to the foot", async () => {
    render(surface(DISCUSSION));
    const box = layOut(list());
    readBack(box);
    expect(voiceHandle).not.toBeNull();

    await act(async () => {
      await voiceHandle!.postAnswer("the answer");
    });

    expect(box.top).toBe(800);
    expect(getDiscussionSession("d1")).toMatchObject({ atTail: true, footRequest: 1 });
  });

  it("CVP-FR-CUKV: a refused spoken answer leaves the list where it is", async () => {
    const onReply = vi.fn(async () => {
      throw new Error("refused");
    });
    render(surface(DISCUSSION, { onReply }));
    const box = layOut(list());
    readBack(box);

    await act(async () => {
      expect(await voiceHandle!.postAnswer("the answer")).toBe("failed");
    });

    expect(box.top).toBe(100);
    expect(getDiscussionSession("d1")).toMatchObject({ atTail: false, footRequest: 0 });
  });
});
