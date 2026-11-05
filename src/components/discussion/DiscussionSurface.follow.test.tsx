/**
 * The follow of a list whose content grows with no new message
 * (`CVP-conversation-presentation.md` CVP-FR-MPDK, CVP-FR-SWDH, CVP-FR-TWRL).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined) }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
  emit: vi.fn(async () => {}),
}));

import { DiscussionSurface } from "./DiscussionSurface";
import { resetLogBufferForTest } from "../../logging";
import { resetDiscussionFocus } from "../../state/discussionFocus";
import { clearAllDiscussionSessions, getDiscussionSession } from "../../state/discussionSession";
import { clearQuestionSets } from "../../state/questionSets";
import { installFakeResizeObserver, resizeAll } from "../../test/fakeScroll";
import type { Comment, Discussion } from "../../types";

const human = { kind: "human", login: "raver119" } as const;

function comment(id: string, body: string): Comment {
  return {
    id,
    author: human,
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
  comments: [comment("c1", "first"), comment("c2", "second")],
  resolved: false,
  createdAt: "2026-02-01T00:00:00Z",
  updatedAt: "2026-02-01T00:00:00Z",
};

/** A scroller with a layout that the test changes, and a clamped offset. */
interface Box {
  height: number;
  client: number;
  top: number;
}

function layOut(el: HTMLElement, box: Box) {
  Object.defineProperty(el, "scrollHeight", { configurable: true, get: () => box.height });
  Object.defineProperty(el, "clientHeight", { configurable: true, get: () => box.client });
  Object.defineProperty(el, "scrollTop", {
    configurable: true,
    get: () => box.top,
    set: (v: number) => {
      box.top = Math.max(0, Math.min(v, box.height - box.client));
    },
  });
}

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

const list = () => screen.getByTestId("discussion-messages");

/** The surface, laid out at its foot: 1000 of content in a box of 200. */
function atFoot(): Box {
  render(
    <DiscussionSurface
      discussion={DISCUSSION}
      owner="rail"
      agents={[]}
      liveTurns={false}
      onReply={vi.fn(async () => undefined)}
      onSetResolved={vi.fn(async () => undefined)}
    />,
  );
  const box = { height: 1000, client: 200, top: 0 };
  layOut(list(), box);
  list().scrollTop = 800;
  fireEvent.scroll(list());
  expect(getDiscussionSession("d1").atTail).toBe(true);
  return box;
}

describe("content that grows with no new message", () => {
  it("CVP-FR-MPDK: at the foot, content that grows keeps the list at the foot", () => {
    const box = atFoot();
    box.height = 1400;
    act(() => resizeAll());
    expect(box.top).toBe(1200);
  });

  it("CVP-FR-MPDK: reading back, content that grows moves nothing and marks nothing unread", () => {
    const box = atFoot();
    list().scrollTop = 100;
    fireEvent.scroll(list());
    box.height = 1400;
    act(() => resizeAll());
    expect(box.top).toBe(100);
    expect(getDiscussionSession("d1").unreadCount).toBe(0);
    expect(screen.queryByTestId("discussion-unread-indicator")).toBeNull();
  });

  it("CVP-FR-MPDK: at the foot, a list box that becomes smaller keeps the list at the foot", () => {
    const box = atFoot();
    box.client = 120;
    act(() => resizeAll());
    expect(box.top).toBe(880);
  });

  it("CVP-FR-MPDK: a row added later is observed", async () => {
    const observe = vi.spyOn(globalThis.ResizeObserver.prototype, "observe");
    atFoot();
    const row = document.createElement("div");
    list().appendChild(row);
    // The mutation record arrives in a microtask.
    await vi.waitFor(() => expect(observe).toHaveBeenCalledWith(row));
  });
});

describe("a move that the list makes itself", () => {
  it("CVP-FR-SWDH: the scroll event of its own move, read after more growth, keeps the list at the foot", () => {
    const box = atFoot();
    box.height = 1400;
    act(() => resizeAll());
    expect(box.top).toBe(1200);
    // The content grows again before the event of that move is handled.
    box.height = 1700;
    fireEvent.scroll(list());
    expect(getDiscussionSession("d1").atTail).toBe(true);
    expect(box.top).toBe(1500);
  });

  it("CVP-FR-SWDH, CVP-FR-TWRL: a scroll of the author away from the foot still stops the follow", () => {
    const box = atFoot();
    box.height = 1400;
    act(() => resizeAll());
    list().scrollTop = 600;
    fireEvent.scroll(list());
    expect(getDiscussionSession("d1").atTail).toBe(false);
    box.height = 1800;
    act(() => resizeAll());
    expect(box.top).toBe(600);
  });
});
