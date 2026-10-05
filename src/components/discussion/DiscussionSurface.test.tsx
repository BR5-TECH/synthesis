/**
 * The shared discussion surface (`CVP-conversation-presentation.md` CVP-FR-SDMQ,
 * CVP-FR-TWRL, CVP-FR-05, CVP-FR-34, CVP-FR-44, CVP-FR-45, CVP-FR-46, CVP-FR-47;
 * `CMT-comments.md` CMT-FR-WZTE, CMT-FR-RPLC; `CTA-comment-agent-turns.md`
 * CTA-FR-ZOLW, CTA-FR-MGVJ).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined) }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
  emit: vi.fn(async () => {}),
}));

import { DiscussionSurface, type DiscussionSurfaceProps } from "./DiscussionSurface";
import { resetDiscussionFocus, focusDiscussion, surfaceOwner } from "../../state/discussionFocus";
import {
  clearAllDiscussionSessions,
  getDiscussionSession,
  mergeDiscussionTurn,
  setComposerBody,
} from "../../state/discussionSession";
import { clearQuestionSets, publishSet } from "../../state/questionSets";
import type {
  AgentTurn,
  Comment,
  Discussion,
  PendingQuestionSet,
} from "../../types";
import {
  artifactDiscussionOrigin,
} from "../../test/origins";

beforeEach(() => {
  clearAllDiscussionSessions();
  clearQuestionSets();
  resetDiscussionFocus();
});
afterEach(cleanup);

const human = { kind: "human", login: "raver119" } as const;
const arch = { kind: "agent", nickname: "arch", title: "Architect" } as never;

function comment(id: string, body: string, author: Comment["author"] = human): Comment {
  return {
    id,
    author,
    body,
    quotes: [],
    attachments: [],
    createdAt: "2026-02-01T00:00:00Z",
  };
}

function discussion(over: Partial<Discussion> = {}): Discussion {
  return {
    id: "d1",
    target: { kind: "artifact", artifactId: "a.md" },
    fragmentTarget: null,
    comments: [comment("c1", "first"), comment("c2", "second")],
    locked: false,
    resolved: false,
    createdAt: "2026-02-01T00:00:00Z",
    updatedAt: "2026-02-01T00:00:00Z",
    ...over,
  };
}

function fragmentTarget() {
  return {
    owner: { kind: "artifact", artifactId: "a.md" },
    path: "a.md",
    start: 0,
    end: 5,
    quote: "the first session",
  } as const;
}

function runningTurn(id = "t1", nickname = "arch"): AgentTurn {
  return {
    id,
    agentId: `agent-${nickname}`,
    nickname,
    origin: artifactDiscussionOrigin("d1", "a.md"),
    triggerCommentId: "c2",
    state: "running",
    failure: null,
    retryPermitted: false,
    startedAt: "2026-02-01T00:00:00Z",
    endedAt: null,
    activeToolCalls: [],
  } as unknown as AgentTurn;
}

function surface(
  d: Discussion,
  over: Partial<DiscussionSurfaceProps> = {},
): React.ReactElement {
  return (
    <DiscussionSurface
      discussion={d}
      owner="rail"
      agents={[]}
      liveTurns={false}
      onReply={vi.fn(async () => undefined)}
      onSetLock={vi.fn(async () => undefined)}
      onSetResolved={vi.fn(async () => undefined)}
      {...over}
    />
  );
}

/**
 * jsdom lays nothing out, so the scroller's geometry is stood in for. `scrollTop`
 * is a real settable value, so a program that moves the list can be observed.
 */
function geometry(el: HTMLElement, scrollHeight = 1000, clientHeight = 200) {
  let top = 0;
  Object.defineProperty(el, "scrollHeight", { configurable: true, get: () => scrollHeight });
  Object.defineProperty(el, "clientHeight", { configurable: true, get: () => clientHeight });
  Object.defineProperty(el, "scrollTop", {
    configurable: true,
    get: () => top,
    set: (v: number) => {
      top = v;
    },
  });
}

const list = () => screen.getByTestId("discussion-messages");

function scrollTo(el: HTMLElement, top: number) {
  el.scrollTop = top;
  fireEvent.scroll(el);
}

describe("scroll-follow and the unread indicator", () => {
  it("CVP-FR-TWRL: the list opens at its foot", () => {
    render(surface(discussion()));
    expect(getDiscussionSession("d1").atTail).toBe(true);
  });

  it("CVP-FR-TWRL: at the foot, a new reply is revealed and nothing is marked unread", () => {
    const d = discussion();
    const view = render(surface(d));
    geometry(list());
    scrollTo(list(), 800);
    view.rerender(
      surface({ ...d, comments: [...d.comments, comment("c3", "third", arch)] }),
    );
    expect(list().scrollTop).toBe(1000);
    expect(screen.queryByTestId("discussion-unread-indicator")).toBeNull();
    expect(getDiscussionSession("d1").unreadCount).toBe(0);
  });

  it("CVP-FR-TWRL: scrolled away, the list keeps its position and shows a labelled unread indicator and divider", () => {
    const d = discussion();
    const view = render(surface(d));
    geometry(list());
    scrollTo(list(), 100);
    view.rerender(
      surface({ ...d, comments: [...d.comments, comment("c3", "third", arch)] }),
    );
    expect(list().scrollTop).toBe(100);
    const indicator = screen.getByRole("button", { name: /1 new message/ });
    expect(indicator).toHaveTextContent("1 new message");
    const divider = screen.getByRole("separator", { name: "1 unread" });
    expect(divider).toHaveTextContent("1 new");
    // The divider stands directly above the first unread message.
    expect(divider.nextElementSibling).toHaveAttribute("data-comment-id", "c3");
  });

  it("CVP-FR-TWRL: the indicator counts every arrival and keeps the divider at the first", () => {
    const d = discussion();
    const view = render(surface(d));
    geometry(list());
    scrollTo(list(), 100);
    const more = [...d.comments, comment("c3", "third"), comment("c4", "fourth")];
    view.rerender(surface({ ...d, comments: more.slice(0, 3) }));
    view.rerender(surface({ ...d, comments: more }));
    expect(screen.getByRole("button", { name: /2 new messages/ })).toBeInTheDocument();
    expect(screen.getByRole("separator").nextElementSibling).toHaveAttribute(
      "data-comment-id",
      "c3",
    );
  });

  it("CVP-FR-TWRL: activating the indicator goes to the first unread message and clears the state", () => {
    const d = discussion();
    const view = render(surface(d));
    geometry(list());
    scrollTo(list(), 100);
    view.rerender(surface({ ...d, comments: [...d.comments, comment("c3", "third")] }));
    const target = list().querySelector('[data-comment-id="c3"]') as HTMLElement;
    target.scrollIntoView = vi.fn();
    fireEvent.click(screen.getByTestId("discussion-unread-indicator"));
    expect(target.scrollIntoView).toHaveBeenCalled();
    expect(screen.queryByTestId("discussion-unread-indicator")).toBeNull();
    expect(screen.queryByRole("separator")).toBeNull();
  });

  it("CVP-FR-TWRL: reaching the foot clears the unread state", () => {
    const d = discussion();
    const view = render(surface(d));
    geometry(list());
    scrollTo(list(), 100);
    view.rerender(surface({ ...d, comments: [...d.comments, comment("c3", "third")] }));
    expect(screen.getByTestId("discussion-unread-indicator")).toBeInTheDocument();
    scrollTo(list(), 790);
    expect(screen.queryByTestId("discussion-unread-indicator")).toBeNull();
  });

  it("CVP-FR-TWRL: a question set that arrives at the foot is revealed", () => {
    render(surface(discussion()));
    geometry(list());
    scrollTo(list(), 800);
    act(() => publishSet("d1", questionSet("s1")));
    expect(list().scrollTop).toBe(1000);
    expect(screen.queryByTestId("discussion-unread-indicator")).toBeNull();
  });

  it("CVP-FR-TWRL: a question set that arrives while reading back marks the discussion unread", () => {
    render(surface(discussion()));
    geometry(list());
    scrollTo(list(), 100);
    act(() => publishSet("d1", questionSet("s1")));
    expect(list().scrollTop).toBe(100);
    expect(screen.getByRole("button", { name: /1 new message/ })).toBeInTheDocument();
  });

  it("CVP-FR-47: a message that arrived while no surface was mounted is unread when one mounts", () => {
    const d = discussion();
    const first = render(surface(d));
    geometry(list());
    scrollTo(list(), 100);
    first.unmount();
    render(surface({ ...d, comments: [...d.comments, comment("c3", "third")] }));
    expect(screen.getByRole("button", { name: /1 new message/ })).toBeInTheDocument();
  });
});

describe("session state across owners", () => {
  it("CTA-FR-ZOLW / CVP-FR-47: a pending placeholder from the store survives an owner change", () => {
    const d = discussion();
    act(() => mergeDiscussionTurn("d1", runningTurn()));
    const first = render(surface(d, { owner: "rail" }));
    expect(screen.getByTestId("comment-pending")).toHaveTextContent("arch");
    first.unmount();
    render(surface(d, { owner: "tab" }));
    const pending = screen.getByTestId("comment-pending");
    expect(pending).toHaveTextContent("arch");
    expect(within(pending).getByRole("button", { name: "Cancel arch's reply" })).toBeInTheDocument();
  });

  it("CTA-FR-ZOLW: each agent with a running turn is its own placeholder and a finished one leaves", () => {
    const d = discussion();
    act(() => {
      mergeDiscussionTurn("d1", runningTurn("t1", "arch"));
      mergeDiscussionTurn("d1", runningTurn("t2", "sec"));
    });
    render(surface(d));
    expect(screen.getAllByTestId("comment-pending")).toHaveLength(2);
    act(() => mergeDiscussionTurn("d1", { ...runningTurn("t1", "arch"), state: "delivered" } as never));
    expect(screen.getAllByTestId("comment-pending")).toHaveLength(1);
  });

  it("CVP-FR-47: the unsent reply text is shown again when the surface changes owner", () => {
    const d = discussion();
    const first = render(surface(d, { owner: "rail" }));
    act(() => setComposerBody("d1", "half written"));
    first.unmount();
    render(surface(d, { owner: "tab" }));
    expect(screen.getByRole("textbox")).toHaveValue("half written");
  });

  it("CVP-FR-47: the scroll position is restored when the surface mounts again", () => {
    const d = discussion();
    const first = render(surface(d));
    geometry(list());
    scrollTo(list(), 300);
    first.unmount();
    render(surface(d, { owner: "tab" }));
    expect(getDiscussionSession("d1").scrollTop).toBe(300);
    expect(getDiscussionSession("d1").atTail).toBe(false);
  });
});

describe("one instance per discussion", () => {
  it("CVP-FR-05: two surfaces of one discussion render the discussion once", () => {
    const d = discussion();
    render(
      <>
        {surface(d, { owner: "rail" })}
        {surface(d, { owner: "tab" })}
      </>,
    );
    expect(document.querySelectorAll('[data-discussion-id="d1"]')).toHaveLength(1);
    expect(surfaceOwner("d1")).toBe("rail");
  });

  it("CVP-FR-05: the waiting surface takes over when the active one unmounts", () => {
    const d = discussion();
    const view = render(
      <>
        {surface(d, { owner: "rail" })}
        {surface(d, { owner: "tab" })}
      </>,
    );
    view.rerender(<>{surface(d, { owner: "tab" })}</>);
    const root = document.querySelector('[data-discussion-id="d1"]');
    expect(root).toHaveAttribute("data-discussion-owner", "tab");
  });

  it("CVP-FR-06: focusing a mounted discussion focuses its surface, and an unmounted one is closed", () => {
    render(surface(discussion()));
    expect(focusDiscussion("d1")).toBe("focused");
    expect(document.querySelector('[data-discussion-id="d1"]')).toHaveFocus();
    expect(focusDiscussion("d1", "composer")).toBe("focused");
    expect(screen.getByRole("textbox")).toHaveFocus();
    expect(focusDiscussion("other")).toBe("closed");
  });

  it("CVP-FR-42: focus returns to the composer where it held focus when the surface was last left", () => {
    render(surface(discussion()));
    fireEvent.focus(screen.getByRole("textbox"));
    expect(getDiscussionSession("d1").focusWasComposer).toBe(true);
    (document.activeElement as HTMLElement | null)?.blur();
    focusDiscussion("d1");
    expect(screen.getByRole("textbox")).toHaveFocus();
  });

  it("CVP-FR-SDMQ: the surface exposes a stable data-discussion-id", () => {
    render(surface(discussion({ id: "abc-123" })));
    expect(document.querySelector('[data-discussion-id="abc-123"]')).not.toBeNull();
  });
});

describe("badges and owner state", () => {
  it("CMT-FR-WZTE: a fragment-targeted discussion carries the word Fragment and its quote", () => {
    render(surface(discussion({ fragmentTarget: fragmentTarget() })));
    expect(screen.getByTestId("discussion-target-badge")).toHaveTextContent("Fragment");
    expect(screen.getByTestId("discussion-fragment-quote")).toHaveTextContent(
      "the first session",
    );
    expect(document.querySelector("[data-discussion-id]")).toHaveAttribute(
      "data-target",
      "fragment",
    );
  });

  it("CMT-FR-WZTE: a whole-target discussion carries the word Whole and no fragment quote", () => {
    render(surface(discussion()));
    expect(screen.getByTestId("discussion-target-badge")).toHaveTextContent("Whole");
    expect(screen.queryByTestId("discussion-fragment-quote")).toBeNull();
  });

  it("CVP-FR-SDMQ: the fragment quote is a labelled control that calls onFocusFragment", () => {
    const onFocusFragment = vi.fn();
    const d = discussion({ fragmentTarget: fragmentTarget() });
    render(surface(d, { onFocusFragment }));
    fireEvent.click(screen.getByRole("button", { name: /the first session/ }));
    expect(onFocusFragment).toHaveBeenCalledWith(fragmentTarget(), d);
  });

  it("CMT-FR-RPLC: an orphaned discussion keeps its quote, says Orphaned, and stays answerable", () => {
    render(
      surface(discussion({ fragmentTarget: fragmentTarget() }), {
        availability: "orphaned",
      }),
    );
    expect(screen.getByText("Orphaned")).toBeInTheDocument();
    expect(screen.getByTestId("discussion-fragment-quote")).toBeInTheDocument();
    expect(screen.getByTestId("discussion-target-badge")).toHaveTextContent("Fragment");
    expect(screen.getByRole("textbox")).toBeEnabled();
  });

  it("CVP-FR-45 / CVP-FR-46: an unavailable owner is named in words and the composer stays enabled", () => {
    render(
      surface(discussion(), { availability: "unavailable", ownerLabel: "specs/gone.md" }),
    );
    expect(screen.getByTestId("discussion-owner-unavailable")).toHaveTextContent(
      "Owner unavailable: specs/gone.md no longer exists in this project",
    );
    expect(screen.getByRole("textbox")).toBeEnabled();
    expect(screen.getByRole("button", { name: /Quote comment 1/ })).toBeInTheDocument();
  });

  it("CVP-FR-44: Locked and Resolved are words", () => {
    render(surface(discussion({ locked: true, resolved: true })));
    const badges = screen.getByTestId("discussion-badges");
    expect(badges).toHaveTextContent("Locked");
    expect(badges).toHaveTextContent("Resolved");
  });
});

describe("accessibility", () => {
  it("CVP-FR-44: the surface, the message log, and the unread region have roles and names", () => {
    render(surface(discussion()));
    expect(screen.getByRole("region", { name: "Discussion d1" })).toBeInTheDocument();
    const log = screen.getByRole("log", { name: "Messages" });
    expect(log).toHaveAttribute("aria-live", "polite");
    expect(screen.getByTestId("discussion-unread-status")).toHaveAttribute("role", "status");
  });

  it("CVP-FR-44: every control carries the name it would have spelled out", () => {
    render(surface(discussion()));
    expect(screen.getByRole("button", { name: "Quote comment 2 by raver119" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Thread actions for d1" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Post" })).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Reply to thread d1" })).toBeInTheDocument();
  });

  it("CVP-FR-44: the unread indicator is a keyboard-operable button", () => {
    const d = discussion();
    const view = render(surface(d));
    geometry(list());
    scrollTo(list(), 100);
    view.rerender(surface({ ...d, comments: [...d.comments, comment("c3", "third")] }));
    const indicator = screen.getByTestId("discussion-unread-indicator");
    expect(indicator.tagName).toBe("BUTTON");
    expect(indicator).not.toHaveAttribute("tabindex", "-1");
    indicator.focus();
    expect(indicator).toHaveFocus();
  });

  it("CVP-FR-44: a failed contribution is marked by a glyph with a name and by words", () => {
    render(surface(discussion(), { failedTurn: runningTurn() }));
    const failed = screen.getByTestId("comment-failed");
    expect(within(failed).getByRole("img", { name: "Failed" })).toBeInTheDocument();
    expect(failed).toHaveTextContent("arch could not produce a response.");
  });
});

describe("lifecycle and operations", () => {
  it("CVP-FR-34: a locked discussion carries no composer and no Quote action", () => {
    render(surface(discussion({ locked: true })));
    expect(screen.queryByRole("textbox")).toBeNull();
    expect(screen.queryByRole("button", { name: /Quote comment/ })).toBeNull();
    expect(screen.getByText("first")).toBeInTheDocument();
  });

  it("CVP-FR-34: a resolved discussion carries no composer and no Quote action", () => {
    render(surface(discussion({ resolved: true })));
    expect(screen.queryByRole("textbox")).toBeNull();
    expect(screen.queryByRole("button", { name: /Quote comment/ })).toBeNull();
  });

  it("CMT-FR-15: Lock and Resolve are offered from the thread menu", () => {
    const onSetLock = vi.fn(async () => undefined);
    const onSetResolved = vi.fn(async () => undefined);
    render(surface(discussion(), { onSetLock, onSetResolved }));
    fireEvent.click(screen.getByRole("button", { name: "Thread actions for d1" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Lock thread" }));
    expect(onSetLock).toHaveBeenCalledWith("d1", true);
    fireEvent.click(screen.getByRole("button", { name: "Thread actions for d1" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Mark resolved" }));
    expect(onSetResolved).toHaveBeenCalledWith("d1", true);
  });

  it("CMT-FR-12: Quote seeds the composer from the message and the quote travels in the post", async () => {
    const onReply = vi.fn(async () => undefined);
    render(surface(discussion(), { onReply }));
    fireEvent.click(screen.getByRole("button", { name: "Quote comment 1 by raver119" }));
    expect(screen.getByRole("button", { name: "Remove quote" })).toBeInTheDocument();
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "agreed" } });
    fireEvent.click(screen.getByRole("button", { name: "Post" }));
    await act(async () => {});
    expect(onReply).toHaveBeenCalledWith(
      "d1",
      "agreed",
      [{ commentId: "c1", excerpt: "first" }],
      [],
    );
  });

  it("CTA-FR-MGVJ / CVP-FR-62: Retry on a failed contribution calls onRetryTurn with its turn", () => {
    const onRetryTurn = vi.fn();
    render(surface(discussion(), { failedTurn: runningTurn("t9"), onRetryTurn }));
    fireEvent.click(screen.getByTestId("comment-failed-retry"));
    expect(onRetryTurn).toHaveBeenCalledWith("t9");
  });

  it("CTA-FR-XZUO: a locked discussion offers no Retry", () => {
    render(surface(discussion({ locked: true }), { failedTurn: runningTurn("t9") }));
    expect(screen.queryByTestId("comment-failed-retry")).toBeNull();
  });

  it("CTA-FR-ZOLW: Cancel on a pending placeholder calls onCancelTurn", () => {
    const onCancelTurn = vi.fn();
    act(() => mergeDiscussionTurn("d1", runningTurn("t1")));
    render(surface(discussion(), { onCancelTurn }));
    fireEvent.click(screen.getByTestId("comment-pending-cancel"));
    expect(onCancelTurn).toHaveBeenCalledWith("t1");
  });

  it("CVP-FR-QDBX: a pending question set renders above a disabled-in-place composer", () => {
    act(() => publishSet("d1", questionSet("s1")));
    render(surface(discussion()));
    expect(screen.getByTestId("discussion-questions-composer-note")).toBeInTheDocument();
    expect(screen.queryByRole("textbox", { name: "Reply to thread d1" })).toBeNull();
    expect(screen.queryByRole("button", { name: /Quote comment/ })).toBeNull();
  });

  it("CVP-FR-SDMQ: a typed turn failure renders inline as an alert", () => {
    render(surface(discussion(), { turnFailure: "unreachable" }));
    expect(screen.getByTestId("comment-turn-error")).toHaveAttribute("role", "alert");
  });
});

function questionSet(setId: string): PendingQuestionSet {
  return {
    setId,
    discussionId: "d1",
    askedBy: arch,
    askedAt: "2026-02-01T00:00:00Z",
    questions: [
      {
        position: 1,
        text: "Which session?",
        options: [{ position: 1, value: "kickoff" }],
      },
    ],
  };
}
