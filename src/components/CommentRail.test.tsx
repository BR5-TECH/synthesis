/**
 * The rail's pinned sections (`CMT-comments.md` CMT-FR-27, CMT-FR-53,
 * CMT-FR-55, CMT-FR-62).
 *
 * Driven directly rather than through a tab, because the claim under test is
 * about the *rail's* three regions — the Discussion section at its head, the
 * aligned layer that travels with the body, and the orphaned/resolved sections
 * at its foot — and the two surfaces that mount it each serve only some of them
 * today. A New Artifact tab passes no aligned cards (the draft scope of anchored
 * threads is not served yet) and an Editor tab passes no discussions
 * (CMT-FR-61), so neither can witness the two together.
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined) }));

import { CommentRail } from "./CommentRail";
import { fragment } from "../test/discussionFixtures";
import type { AnchoredThread } from "../state/commentAnchors";
import type {
  AgentTurn,
  FragmentRange,
  Discussion,
  Participant,
  ProjectAgent,
} from "../types";
import {
  artifactCommentOrigin,
  draftDiscussionOrigin,
} from "../test/origins";

// This repo does not enable Testing Library's automatic cleanup, so each render
// is torn down explicitly — otherwise a second mount finds the first still in
// the document and every `getBy` becomes ambiguous.
afterEach(cleanup);

const author: Participant = { kind: "human", login: "raver119" };

function comments(id: string, body: string) {
  return [
    {
      id: `${id}-c1`,
      author,
      body,
      quotes: [],
      attachments: [],
      createdAt: "2026-02-01T00:00:00Z",
    },
  ];
}

function anchored(id: string, anchor: FragmentRange, resolved = false): AnchoredThread {
  const thread: Discussion = {
    id,
    target: { kind: "artifact", artifactId: "a.md" },
    fragmentTarget: {
      owner: { kind: "artifact", artifactId: "a.md" },
      path: "a.md",
      ...anchor,
    },
    comments: comments(id, "a passage"),
    locked: false,
    resolved,
    createdAt: "2026-02-01T00:00:00Z",
    updatedAt: "2026-02-01T00:00:00Z",
  };
  return { thread, anchor };
}

function discussion(id: string, resolved = false): AnchoredThread {
  const thread: Discussion = {
    id, target: { kind: "draft", draftId: "d1" }, fragmentTarget: null,
    comments: comments(id, "the whole draft"), locked: false, resolved,
    createdAt: "2026-02-01T00:00:00Z", updatedAt: "2026-02-01T00:00:00Z",
  };
  return { thread, anchor: null };
}

type Overrides = {
    threads?: AnchoredThread[];
    discussions?: AnchoredThread[];
    anchorTops?: Record<string, number>;
    scrollTop?: number;
    focusedThreadId?: string | null;
    showResolved?: boolean;
    arrangement?: "beside" | "below";
    draftQuote?: string | null;
    pendingTurns?: AgentTurn[];
    agents?: ProjectAgent[];
    onCancelTurn?: (turnId: string) => void;
};

/**
 * The rail as an element rather than a mount, so a test can re-render it with
 * different threads. Some of what the rail decides is memoised on those threads,
 * and a claim about what happens when one is resolved is not tested by mounting
 * the resolved state from scratch.
 */
const refuse = async (): Promise<never> => { throw new Error("opens nothing"); };
const draftFor = (quote?: string | null) =>
  quote == null ? null : {
    target: { kind: "artifact" as const, artifactId: "a.md" },
    fragmentTarget: fragment({ start: 0, end: quote.length, quote }),
  };
function view(over: Overrides = {}) {
  const noop = async () => {};
  return (
    <CommentRail
      threads={over.threads ?? []}
      discussions={over.discussions ?? []}
      anchorTops={over.anchorTops ?? {}}
      scrollTop={over.scrollTop ?? 0}
      arrangement={over.arrangement ?? "beside"}
      identity={author}
      identityBlock={null}
      blocked={false}
      focusedThreadId={over.focusedThreadId ?? null}
      onFocusThread={() => {}}
      draft={draftFor(over.draftQuote)}
      onOpenDraft={refuse}
      onCancelDraft={() => {}}
      onReply={noop}
      onSetLock={noop}
      onSetResolved={noop}
      errors={{}}
      showResolved={over.showResolved ?? false}
      onToggleResolved={() => {}}
      agents={over.agents ?? []}
      pendingTurns={over.pendingTurns ?? []}
      turnFailures={{}}
      onCancelTurn={over.onCancelTurn ?? (() => {})}
    />
  );
}

function mount(over: Overrides = {}) {
  return render(view(over));
}

/** The `translateY` the aligned layer carries, in pixels. */
function alignedShift(): string {
  const layer = document.querySelector(".comment-rail__aligned") as HTMLElement;
  return layer.style.transform;
}

describe("CMT-FR-62: what is pinned and what travels", () => {
  it("keeps the Discussion section out of the layer that moves with the body", () => {
    // The claim is not "the section is elsewhere in the DOM" but "it stays put
    // while the aligned cards scroll". The aligned layer is translated by the
    // body's scroll offset; the pinned sections are not in it, so they are not
    // translated by anything.
    mount({
      threads: [anchored("t1", { start: 0, end: 3, quote: "abc" })],
      discussions: [discussion("d-1")],
      anchorTops: { t1: 400 },
      scrollTop: 0,
    });
    const section = screen.getByTestId("discussion-section");
    expect(within(section).getByTestId("comment-thread-d-1")).toBeInTheDocument();
    expect(alignedShift()).toBe("translateY(0px)");

    // Remounted rather than re-rendered, so the second case starts from the same
    // state the first did and the shift below is not a reconciliation artefact.
    cleanup();
    mount({
      threads: [anchored("t1", { start: 0, end: 3, quote: "abc" })],
      discussions: [discussion("d-1")],
      anchorTops: { t1: 400 },
      scrollTop: 250,
    });
    // The aligned card moved by exactly the scroll; the pinned section did not
    // move at all, because nothing translates it.
    expect(alignedShift()).toBe("translateY(-250px)");
    const pinned = screen.getByTestId("discussion-section");
    expect(pinned.closest(".comment-rail__aligned")).toBeNull();
    expect(pinned.style.transform).toBe("");
    // …and the aligned card is in the layer that does move.
    expect(
      screen
        .getByTestId("comment-thread-t1")
        .closest(".comment-rail__aligned"),
    ).not.toBeNull();
  });

  it("renders no Discussion section for a draft carrying none", () => {
    // CMT-FR-53: no section and no header for one.
    mount({ threads: [anchored("t1", { start: 0, end: 3, quote: "abc" })] });
    expect(screen.queryByTestId("discussion-section")).not.toBeInTheDocument();
    expect(screen.queryByText("Discussion")).not.toBeInTheDocument();
  });
});

describe("CMT-FR-08 / CMT-FR-15: a card leads with a message, not with a header", () => {
  it("carries the thread's menu on its first comment and renders no row above it", () => {
    // A card used to open with its own row — the first comment's author, the
    // thread's last-touched time, and the overflow menu. On a discussion, which
    // has no anchor quote to separate the two (CMT-FR-55), that row sat directly
    // above a second one naming the same author, and read as an empty first
    // message that happened to own the menu.
    mount({ discussions: [discussion("d-1")] });

    const card = screen.getByTestId("comment-thread-d-1");
    const comments = card.querySelectorAll(".comment");
    expect(comments).toHaveLength(1);

    // The menu is inside the message, so there is no headed row left to be
    // mistaken for one.
    const menu = screen.getByRole("button", { name: "Thread actions for d-1" });
    expect(menu.closest(".comment")).toBe(comments[0]);
    expect(card.querySelector(".comment-card__head")).toBeNull();

    // And it is still the thread's menu, carrying both entries (CMT-FR-15,
    // CMT-FR-16) — moving it changed where it hangs, not what it does.
    fireEvent.click(menu);
    const opened = within(card).getByRole("menu");
    expect(
      within(opened).getByRole("menuitem", { name: "Lock thread" }),
    ).toBeInTheDocument();
    expect(
      within(opened).getByRole("menuitem", { name: "Mark resolved" }),
    ).toBeInTheDocument();
  });

  it("carries exactly one menu however many messages the thread holds", () => {
    // The menu belongs to the THREAD. Rendering it per comment would offer to
    // lock the thread three times over and leave two of them un-labelled.
    const thread = discussion("d-1");
    thread.thread.comments = [
      ...comments("d-1", "the whole draft"),
      {
        id: "d-1-c2",
        author: { kind: "agent", agentId: "a1", handle: "arch" } as Participant,
        body: "a reply",
        quotes: [],
        attachments: [],
        createdAt: "2026-02-01T00:01:00Z",
      },
    ];
    mount({ discussions: [thread] });

    expect(
      screen.getAllByRole("button", { name: "Thread actions for d-1" }),
    ).toHaveLength(1);
  });
});

describe("CMT-FR-16: a resolved thread takes no further comment", () => {
  it("renders no composer and no Quote inside the disclosure, and both again once reopened", () => {
    // A thread the author has settled is not a thread to add to. A composer
    // inside the resolved disclosure invites a reply nobody is coming back to
    // read, and Quote — which seeds that composer — is a control that visibly
    // does nothing once it is gone.
    mount({ discussions: [discussion("d-1", true)], showResolved: true });

    const card = screen.getByTestId("comment-thread-d-1");
    expect(within(card).queryByPlaceholderText(/reply/i)).not.toBeInTheDocument();
    expect(
      within(card).queryByRole("button", { name: /^Quote comment/ }),
    ).not.toBeInTheDocument();
    // The menu stays: reopening is the one thing a resolved card does offer,
    // and it is what puts the composer back (CMT-FR-16).
    expect(
      within(card).getByRole("button", { name: "Thread actions for d-1" }),
    ).toBeInTheDocument();

    cleanup();
    mount({ discussions: [discussion("d-1")] });
    const open = screen.getByTestId("comment-thread-d-1");
    expect(within(open).getByPlaceholderText(/reply/i)).toBeInTheDocument();
    expect(
      within(open).getByRole("button", { name: /^Quote comment/ }),
    ).toBeInTheDocument();
  });
});

describe("CMS-FR-53: a discussion carrying no anchor field at all", () => {
  it("renders as a discussion rather than taking the window down with it", () => {
    // Guards the shape, not the type. `anchor` is a required field holding null,
    // and a card reads it to decide whether to show a quote — but a Rust field
    // that is *skipped* rather than serialized arrives here as `undefined`, and
    // `undefined !== null`, so the guard passes and `.quote` is read off nothing.
    // That threw during render, which in React 19 unmounts the whole tree: the
    // author posted the first message of a discussion and got a white window.
    //
    // The backend now emits the field (`comments.rs`, "a discussion serializes an
    // anchor that is present and null"). This is the other half — the rail
    // survives the shape whatever the wire does.
    const bare = discussion("d-1");
    delete (bare.thread as { anchor?: unknown }).anchor;

    mount({ discussions: [bare] });

    const section = screen.getByTestId("discussion-section");
    const card = within(section).getByTestId("comment-thread-d-1");
    expect(card).toBeInTheDocument();
    expect(card.querySelector(".comment-card__quote")).toBeNull();
    // …and it is not mistaken for an orphan, which is the other thing an absent
    // field reads as: a thread that stores an anchor none was resolved for.
    expect(screen.queryByText("Orphaned")).not.toBeInTheDocument();
  });
});

describe("NAW-FR-32 / CMT-FR-28: a focused card is brought into view", () => {
  it("scrolls the focused card's own scroller, and leaves an unfocused one alone", () => {
    // The pinned sections scroll within themselves, so a card appended to the
    // Discussion section lands below its fold as soon as the section is full —
    // and on a short window the card just posted is the first one past it. The
    // author would see the composer close and nothing appear.
    const seen: unknown[] = [];
    const original = Element.prototype.scrollIntoView;
    Element.prototype.scrollIntoView = function (
      this: HTMLElement,
      arg?: unknown,
    ) {
      seen.push([this.dataset.testid, arg]);
    } as typeof original;
    try {
      mount({
        discussions: [discussion("d-1"), discussion("d-2")],
        focusedThreadId: "d-2",
      });
      expect(seen).toEqual([["comment-thread-d-2", { block: "nearest" }]]);
    } finally {
      Element.prototype.scrollIntoView = original;
    }
  });
});

describe("CMT-FR-55: a discussion is never orphaned", () => {
  it("stays in the Discussion section though it aligns to nothing", () => {
    // An orphan is a thread whose STORED quote is nowhere in the content. A
    // discussion stores no quote at all, so there is nothing to fail to find —
    // and it must not fall into the orphaned section for looking the same as one
    // from the aligned layer's point of view.
    mount({
      threads: [
        // A genuine orphan: it stores an anchor, and none was resolved.
        { ...anchored("t1", { start: 0, end: 3, quote: "gone" }), anchor: null },
      ],
      discussions: [discussion("d-1")],
    });

    const orphaned = screen.getByText("Orphaned", { selector: ".comment-rail__section-title" }).parentElement!;
    expect(within(orphaned).getByTestId("comment-thread-t1")).toBeInTheDocument();
    expect(
      within(orphaned).queryByTestId("comment-thread-d-1"),
    ).not.toBeInTheDocument();
    expect(
      within(screen.getByTestId("discussion-section")).getByTestId(
        "comment-thread-d-1",
      ),
    ).toBeInTheDocument();
  });

  it("moves a resolved discussion to the disclosure at the foot, with the resolved threads", () => {
    // CMT-FR-56, from the rail's own side: the disclosure counts discussions and
    // anchored threads together, because a conversation waiting on the author is
    // waiting on them whether it names a passage or not.
    mount({
      threads: [anchored("t1", { start: 0, end: 3, quote: "abc" }, true)],
      discussions: [discussion("d-1", true), discussion("d-2")],
    });

    expect(
      screen.getByRole("button", { name: /2 resolved threads/ }),
    ).toBeInTheDocument();
    const section = screen.getByTestId("discussion-section");
    expect(within(section).getByTestId("comment-thread-d-2")).toBeInTheDocument();
    expect(
      within(section).queryByTestId("comment-thread-d-1"),
    ).not.toBeInTheDocument();
  });
});

describe("CMT-FR-63: the foot is bounded by what else the margin holds", () => {
  // The bound itself is a `max-height` and so lives in the stylesheet
  // (`style-invariants.test.ts` holds the two rules to it). What is decided here
  // is *when* it applies — and getting that wrong is what left an artifact whose
  // every thread was resolved reading in the lower half of an empty margin.
  const foot = () =>
    document.querySelector(".comment-rail__unaligned") as HTMLElement;

  it("keeps no height back when the margin has nothing else to show", () => {
    mount({ threads: [anchored("t1", { start: 0, end: 3, quote: "abc" }, true)] });
    expect(foot().dataset.bounded).toBe("false");
  });

  it("keeps height back for a thread still aligned in the body", () => {
    mount({
      threads: [
        anchored("t1", { start: 0, end: 3, quote: "abc" }),
        anchored("t2", { start: 9, end: 12, quote: "xyz" }, true),
      ],
    });
    expect(foot().dataset.bounded).toBe("true");
  });

  it("keeps height back for a discussion pinned at the head", () => {
    mount({
      discussions: [discussion("d-1"), discussion("d-2", true)],
    });
    expect(foot().dataset.bounded).toBe("true");
  });

  it("gives the height back and takes it again as threads are resolved and reopened", () => {
    // CMT-FR-63, CMT-FR-62, CMT-FR-17's second and third beats. Mounting each state afresh cannot see
    // this: `aligned` and `openDiscussions` are memos, and a bug that computed
    // the flag once and never again would pass every mount-from-scratch test in
    // this describe while the margin stayed wrong for the whole session.
    const open = anchored("t1", { start: 0, end: 3, quote: "abc" });
    const settled = anchored("t2", { start: 9, end: 12, quote: "xyz" }, true);
    const { rerender } = mount({ threads: [open, settled] });
    expect(foot().dataset.bounded).toBe("true");

    rerender(
      view({
        threads: [
          anchored("t1", { start: 0, end: 3, quote: "abc" }, true),
          settled,
        ],
      }),
    );
    expect(foot().dataset.bounded).toBe("false");

    rerender(view({ threads: [open, settled] }));
    expect(foot().dataset.bounded).toBe("true");
  });

  it("keeps height back for the card being written, wherever it is anchored", () => {
    // A draft composer is something the margin has to show. Without it in the
    // reckoning, starting a thread on an artifact whose every thread is resolved
    // gave the expanded footer the whole margin — and the composer, trapped in
    // the aligned layer's own stacking context, opened behind it.
    mount({
      threads: [anchored("t1", { start: 0, end: 3, quote: "abc" }, true)],
      showResolved: true,
      draftQuote: "a passage",
    });
    expect(screen.getByTestId("comment-draft")).toBeInTheDocument();
    expect(foot().dataset.bounded).toBe("true");
  });

  it("stops keeping it back once the last open thread is resolved", () => {
    // The state that motivated the rule: the disclosure was capped and scrolling
    // inside itself while two-fifths of the margin above it stood empty, because
    // the cap could not tell that there was nothing left to keep it for.
    mount({
      threads: [
        anchored("t1", { start: 0, end: 3, quote: "abc" }, true),
        anchored("t2", { start: 9, end: 12, quote: "xyz" }, true),
      ],
      discussions: [discussion("d-1", true)],
      showResolved: true,
    });
    expect(foot().dataset.bounded).toBe("false");
    // …and every one of them is still readable inside it.
    const section = foot();
    for (const id of ["t1", "t2", "d-1"]) {
      expect(within(section).getByTestId(`comment-thread-${id}`)).toBeInTheDocument();
    }
  });
});

describe("CMT-FR-64: the cards below the page", () => {
  it("stops aligning and stops travelling once there is no margin to do it in", () => {
    // Beside the page a card is placed at its anchor's line and the whole layer
    // is shifted by the body's scroll. Below the page there is no line to be
    // level with — and keeping the shift would slide the column off the top of
    // the rail by however far the author had scrolled.
    mount({
      threads: [
        anchored("t1", { start: 0, end: 3, quote: "abc" }),
        anchored("t2", { start: 9, end: 12, quote: "xyz" }),
      ],
      anchorTops: { t1: 40, t2: 400 },
      scrollTop: 250,
      arrangement: "below",
    });

    expect(
      document.querySelector(".comment-rail")?.getAttribute("data-arrangement"),
    ).toBe("below");
    expect(alignedShift()).toBe("");
    for (const id of ["t1", "t2"]) {
      const card = screen.getByTestId(`comment-thread-${id}`);
      expect(card.dataset.positioned).toBe("false");
      expect(card.style.top).toBe("");
    }
  });

  it("lays the card being written in the column too", () => {
    // The one card that is not a `ThreadCard`, and so the one the arrangement
    // was not threaded into. Left positioned, it resolves its offsets against
    // the tab — the rail and its aligned layer are both static here — and the
    // composer is laid across the page at whatever pixel its anchor was at,
    // which is precisely what CMT-FR-64 says never happens.
    mount({
      threads: [anchored("t1", { start: 0, end: 3, quote: "abc" })],
      anchorTops: { __draft__: 800 },
      arrangement: "below",
      draftQuote: "a passage",
    });
    const draft = screen.getByTestId("comment-draft");
    expect(draft.dataset.positioned).toBe("false");
    expect(draft.style.top).toBe("");

    // …and beside the page it is aligned to its selection like any other card.
    cleanup();
    mount({
      threads: [anchored("t1", { start: 0, end: 3, quote: "abc" })],
      anchorTops: { __draft__: 800 },
      draftQuote: "a passage",
    });
    const beside = screen.getByTestId("comment-draft");
    expect(beside.dataset.positioned).toBe("true");
    expect(beside.style.top).toBe("800px");
  });

  it("reads as one column in document order, discussions first and resolved last", () => {
    // The order the cards are already in is the order the column reads in, which
    // is what keeps the two arrangements the same rail rather than two. The
    // order is the same beside the page — nothing here reorders — so this is a
    // guard against a future column that sorts, not a witness of today's.
    mount({
      threads: [
        anchored("t1", { start: 0, end: 3, quote: "abc" }),
        anchored("t2", { start: 9, end: 12, quote: "xyz" }, true),
      ],
      discussions: [discussion("d-1")],
      showResolved: true,
      arrangement: "below",
    });

    const rail = document.querySelector(".comment-rail") as HTMLElement;
    const order = Array.from(
      rail.querySelectorAll("[data-testid^='comment-thread-']"),
    ).map((el) => el.getAttribute("data-testid"));
    expect(order).toEqual([
      "comment-thread-d-1",
      "comment-thread-t1",
      "comment-thread-t2",
    ]);
  });

  it("leaves every card's own controls exactly as they are beside the page", () => {
    // A different arrangement, not a different card: the composer, the menu and
    // the quote actions are what a thread is for, and losing them on a narrow
    // window would be a worse defect than the overlap this replaced. No control
    // branches on the arrangement today, so this too is a guard against one that
    // starts to rather than a witness of the column.
    mount({
      threads: [anchored("t1", { start: 0, end: 3, quote: "abc" })],
      arrangement: "below",
    });
    const card = screen.getByTestId("comment-thread-t1");
    expect(
      within(card).getByRole("textbox", { name: /Reply to thread t1/ }),
    ).toBeInTheDocument();
    expect(
      within(card).getByRole("button", { name: /Thread actions for t1/ }),
    ).toBeInTheDocument();
    expect(within(card).getByRole("button", { name: /^Quote comment 1/ })).toBeInTheDocument();
  });

  it("still aligns and still travels while there is a margin", () => {
    // The default, and the one the rest of this file exercises: beside the page,
    // a card is positioned at its anchor and the layer carries the scroll.
    mount({
      threads: [anchored("t1", { start: 0, end: 3, quote: "abc" })],
      anchorTops: { t1: 400 },
      scrollTop: 250,
    });
    expect(alignedShift()).toBe("translateY(-250px)");
    expect(screen.getByTestId("comment-thread-t1").dataset.positioned).toBe("true");
  });
});

// ---------------------------------------------------------------------------
// CTA-FR-CHUJ, CMT-FR-10, CMS-FR-65 / CTA-FR-KYPK: the agent title line (CTA-FR-KFUF)
// ---------------------------------------------------------------------------

describe("CTA-FR-YOGW: an agent comment's title line", () => {
  /** One agent comment, with whatever title snapshot its participant carries. */
  function agentComment(id: string, handle: string, title?: string) {
    return {
      id,
      author: {
        kind: "agent",
        agentId: `id-${handle}`,
        handle,
        model: "m",
        ...(title === undefined ? {} : { title }),
      } as Participant,
      body: "an answer",
      quotes: [],
      attachments: [],
      createdAt: "2026-02-01T00:01:00Z",
    };
  }

  function withComments(extra: ReturnType<typeof agentComment>[]) {
    const t = anchored("t1", { start: 0, end: 3, quote: "abc" });
    t.thread.comments = [...t.thread.comments, ...extra];
    mount({ threads: [t] });
    return screen.getByTestId("comment-thread-t1");
  }

  it("renders the snapshot the comment carries, not the agent's current title", () => {
    // CTA-FR-CHUJ, CMT-FR-10, CMS-FR-65. Two comments by one agent under two titles: the pair can only
    // both be right if each is read from its own participant, which is the
    // whole point of the snapshot (CMS-FR-65).
    const card = withComments([
      agentComment("c2", "arch", "Developer"),
      agentComment("c3", "arch", "Architect"),
    ]);
    expect(
      within(card)
        .getAllByTestId("comment-agent-title")
        .map((e) => e.textContent),
    ).toEqual(["Developer", "Architect"]);
  });

  it("renders no line and no placeholder for an absent or an empty snapshot", () => {
    // CTA-FR-KYPK. Absent is an older log (CMS-FR-65); empty is an agent that
    // carries no title today. Both render nothing — and in particular never
    // `Not defined`, which belongs to a prompt alone (CVL-FR-04).
    const card = withComments([
      agentComment("c2", "scribe"),
      agentComment("c3", "sec", ""),
    ]);
    expect(within(card).queryAllByTestId("comment-agent-title")).toHaveLength(0);
    expect(card.textContent ?? "").not.toContain("Not defined");
    // The comments themselves are still there and still readable.
    expect(card).toHaveTextContent("scribe");
    expect(card).toHaveTextContent("sec");
  });

  it("gives a human comment no title line", () => {
    // CTA-FR-CHUJ: human participants are untouched by any of this.
    const card = withComments([]);
    expect(within(card).queryAllByTestId("comment-agent-title")).toHaveLength(0);
  });

  it("gives a pending and a failed contribution no title line either", () => {
    // CTA-FR-KYPK's second half. Neither is a comment with a participant to carry
    // a snapshot, so neither may sprout a title from the roster — which is the
    // one place a live lookup would be tempting, because the turn names an
    // agent that IS in the roster.
    const t = anchored("t1", { start: 0, end: 3, quote: "abc" });
    t.thread.comments = [...t.thread.comments, agentComment("c2", "arch", "Developer")];
    mount({
      threads: [t],
      pendingTurns: [
        {
          id: "turn-1",
          agentId: "id-arch",
          nickname: "arch",
          // The rail filters turns to their own thread (CTA-FR-QXIG).
          origin: artifactCommentOrigin("t1"),
          triggerCommentId: "t1-c1",
          state: "running",
          failure: null,
          retryPermitted: false,
          startedAt: "2026-02-01T00:02:00Z",
          endedAt: null,
        } as unknown as AgentTurn,
      ],
    });
    const card = screen.getByTestId("comment-thread-t1");
    // Exactly one: the delivered comment's. The pending contribution beside it
    // is attributed to the same agent and carries none.
    expect(
      within(card)
        .getAllByTestId("comment-agent-title")
        .map((e) => e.textContent),
    ).toEqual(["Developer"]);
    expect(card).toHaveTextContent("Thinking…");
  });
});

// ---------------------------------------------------------------------------
// CTA-FR-IAKP: who an untagged reply reaches, in the composer's placeholder
// ---------------------------------------------------------------------------

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

/** A conversation whose newest human comment names `@arch` and `@sec`. */
function addressed(entry: AnchoredThread): AnchoredThread {
  entry.thread.comments = [
    {
      ...entry.thread.comments[0],
      body: "@arch @sec please review",
    },
  ];
  return entry;
}

describe("CTA-FR-IAKP: the composer names its recipients in its placeholder", () => {
  const roster = [projectAgent("arch", "a1"), projectAgent("sec", "a2")];

  it("names them on an anchored card and a discussion in the same tab", () => {
    // CTA-FR-IAKP, CTA-FR-IGBO, CTA-FR-YGMB's first clause: each placeholder reads the same, and no other
    // line of either card names a recipient.
    mount({
      threads: [addressed(anchored("t1", { start: 0, end: 3, quote: "abc" }))],
      discussions: [addressed(discussion("d-1"))],
      anchorTops: { t1: 0 },
      agents: roster,
    });
    for (const id of ["t1", "d-1"]) {
      const card = screen.getByTestId(`comment-thread-${id}`);
      const composer = within(card).getByLabelText(`Reply to thread ${id}`);
      expect(composer.getAttribute("placeholder")).toBe(
        "Reply to @arch, @sec…",
      );
      // CTA-FR-YGMB: nowhere else. The card's own rendered text names the agents
      // only where the author wrote them — in the comment body.
      const reply = card.querySelector(".comment-card__reply");
      expect(reply).not.toBeNull();
      expect(reply!.textContent ?? "").not.toContain("@arch");
    }
  });

  it("reads `Reply…` in a conversation with no active agent at all", () => {
    // CTA-FR-IAKP, CTA-FR-IGBO, CTA-FR-YGMB's last clause / CTA-FR-XTZC, CTA-FR-DWCK: an ordinary state rather than an
    // error — the composer is not disabled.
    mount({
      threads: [anchored("t1", { start: 0, end: 3, quote: "abc" })],
      anchorTops: { t1: 0 },
      agents: roster,
    });
    const composer = screen.getByLabelText("Reply to thread t1");
    expect(composer.getAttribute("placeholder")).toBe("Reply…");
    expect(composer).toBeEnabled();
  });

  it("discloses the untruncated list nowhere, and grows the field for nobody", () => {
    // CTA-FR-IAKP, CTA-FR-IGBO, CTA-FR-YGMB's second clause. The clipping itself is the field's
    // (`text-overflow: ellipsis` on the placeholder, which jsdom does not lay
    // out); what a test can hold is that nothing *else* carries the list and
    // that the field is still the one-line auto-growing composer.
    const many = ["arch", "sec", "scribe", "archivist", "reviewer"].map(
      (nickname, i) => projectAgent(nickname, `a${i}`),
    );
    const entry = anchored("t1", { start: 0, end: 3, quote: "abc" });
    entry.thread.comments = [
      {
        ...entry.thread.comments[0],
        body: "@arch @sec @scribe @archivist @reviewer please review",
      },
    ];
    mount({ threads: [entry], anchorTops: { t1: 0 }, agents: many });
    const composer = screen.getByLabelText("Reply to thread t1");
    expect(composer.getAttribute("placeholder")).toBe(
      "Reply to @arch, @sec, @scribe, @archivist, @reviewer…",
    );
    // The accessible name names the composer exactly as it does in a
    // conversation with no active agent at all, and the untruncated list is
    // disclosed nowhere — not on the field, not on anything around it, and not
    // through a description the field points at.
    expect(composer.getAttribute("aria-label")).toBe("Reply to thread t1");
    expect(composer.getAttribute("aria-describedby")).toBeNull();
    const card = screen.getByTestId("comment-thread-t1");
    for (const element of Array.from(card.querySelectorAll("[title]"))) {
      expect(element.getAttribute("title") ?? "").not.toContain("@");
    }
    // The clipping itself is `.comment-card__composer::placeholder`'s, asserted
    // in `src/test/style-invariants.test.ts` — jsdom lays nothing out, so a
    // render here can hold only that nothing *else* carries the list.
  });
});

// ---------------------------------------------------------------------------
// CTA-FR-FBJR / CTA-FR-IWOJ: what a pending contribution says the agent is doing
// ---------------------------------------------------------------------------

function running(activeToolCalls: AgentTurn["activeToolCalls"]): AgentTurn {
  return {
    id: "turn-1",
    agentId: "a1",
    nickname: "arch",
    origin: artifactCommentOrigin("t1"),
    triggerCommentId: "t1-c1",
    state: "running",
    failure: null,
    retryPermitted: false,
    imagesOmitted: false,
    startedAt: "2026-02-01T00:02:00Z",
    endedAt: null,
    activeToolCalls,
  };
}

describe("CTA-FR-FBJR: the pending contribution's activity status", () => {
  function status(turn: AgentTurn): string {
    mount({
      threads: [anchored("t1", { start: 0, end: 3, quote: "abc" })],
      anchorTops: { t1: 0 },
      pendingTurns: [turn],
    });
    return screen.getByTestId("comment-pending-status").textContent ?? "";
  }

  it("reads Thinking… for a turn with no tool call active in it", () => {
    // CTA-FR-FBJR, CTA-FR-IHOB's first clause.
    expect(status(running([]))).toBe("Thinking…");
  });

  it("reads the tool's own status while a call is active", () => {
    // CTA-FR-FBJR, CTA-FR-IHOB: `search_specifications` names what the agent is doing.
    expect(
      status(
        running([
          { id: "call-1", tool: "search_specifications", activationSeq: 1 },
        ]),
      ),
    ).toBe("Searching related specifications…");
  });

  it("reads Working… for a tool the vocabulary does not name", () => {
    // CTA-FR-FBJR, CTA-FR-IHOB's last clause: never an empty line.
    expect(
      status(
        running([
          { id: "call-1", tool: "a_tool_this_surface_was_never_taught", activationSeq: 1 },
        ]),
      ),
    ).toBe("Working…");
  });

  it("reads the most recently activated call, whatever order the list arrives in", () => {
    // CTA-FR-IWOJ, CTA-FR-KWOF: the later activation is the one spoken for, and the order is
    // the turn's own rather than the events'.
    expect(
      status(
        running([
          { id: "call-1", tool: "read_file", activationSeq: 1 },
          { id: "call-2", tool: "search_skills", activationSeq: 2 },
        ]),
      ),
    ).toBe("Searching available skills…");
    cleanup();
    expect(
      status(
        running([
          { id: "call-2", tool: "search_skills", activationSeq: 2 },
          { id: "call-1", tool: "read_file", activationSeq: 1 },
        ]),
      ),
    ).toBe("Searching available skills…");
  });

  it("reads the same on a discussion card as on an anchored one", () => {
    // CMT-FR-55: a discussion's card "renders pending contributions reading the
    // same activity statuses". One `ThreadCard` serves both, and this is what
    // holds that true if the two ever stop sharing it.
    mount({
      discussions: [discussion("d-1")],
      pendingTurns: [
        {
          ...running([
            { id: "call-1", tool: "search_drafts", activationSeq: 1 },
          ]),
          origin: draftDiscussionOrigin("d-1", "d1"),
        },
      ],
    });
    const card = screen.getByTestId("comment-thread-d-1");
    expect(
      within(card).getByTestId("comment-pending-status"),
    ).toHaveTextContent("Searching drafts…");
  });

  it("changes nothing else about the contribution", () => {
    // CTA-FR-PCNW: the shape, the position, the agent it is attributed to, and
    // its cancel control are the same whichever status it reads (CTA-FR-XMCQ).
    mount({
      threads: [anchored("t1", { start: 0, end: 3, quote: "abc" })],
      anchorTops: { t1: 0 },
      pendingTurns: [
        running([
          { id: "call-1", tool: "openrouter:web_search", activationSeq: 1 },
        ]),
      ],
    });
    const pending = screen.getByTestId("comment-pending");
    expect(pending).toHaveTextContent("Searching the web…");
    expect(pending).toHaveTextContent("arch");
    expect(
      within(pending).getByTestId("comment-pending-cancel"),
    ).toBeInTheDocument();
    // Nothing of the call itself: no argument, no result, no tool name.
    expect(pending.textContent ?? "").not.toContain("openrouter");
  });
});

describe("CTA-FR-IVNG: transitions between statuses", () => {
  it("hands the line to the latest call still active, blank at no moment", () => {
    // CTA-FR-IVNG, CTA-FR-JQUU: a `read_file` call and a later `openrouter:web_search` call,
    // then the search finishes, then `read_file` refuses.
    const anchoredThread = anchored("t1", { start: 0, end: 3, quote: "abc" });
    const rendered = render(
      view({
        threads: [anchoredThread],
        anchorTops: { t1: 0 },
        pendingTurns: [
          running([
            { id: "call-1", tool: "read_file", activationSeq: 1 },
            { id: "call-2", tool: "openrouter:web_search", activationSeq: 2 },
          ]),
        ],
      }),
    );
    const line = () =>
      screen.getByTestId("comment-pending-status").textContent ?? "";
    expect(line()).toBe("Searching the web…");

    // The web search finishes: at once, and never blank.
    rendered.rerender(
      view({
        threads: [anchoredThread],
        anchorTops: { t1: 0 },
        pendingTurns: [
          running([{ id: "call-1", tool: "read_file", activationSeq: 1 }]),
        ],
      }),
    );
    expect(line()).toBe("Reading a project file…");

    // The `read_file` call refuses: the turn waits on its next model response.
    rendered.rerender(
      view({
        threads: [anchoredThread],
        anchorTops: { t1: 0 },
        pendingTurns: [running([])],
      }),
    );
    expect(line()).toBe("Thinking…");

    expect(screen.getByTestId("comment-pending-cancel")).toBeInTheDocument();
  });

  it("cancels from the contribution exactly as it does with no call active", () => {
    // CTA-FR-IVNG, CTA-FR-JQUU's last clause. Driven while a call IS active, which is the
    // case the status could have broken: the control is CTA-FR-XMCQ's and reads
    // nothing from the status beside it.
    const cancelled: string[] = [];
    render(
      view({
        threads: [anchored("t1", { start: 0, end: 3, quote: "abc" })],
        anchorTops: { t1: 0 },
        pendingTurns: [
          running([
            { id: "call-1", tool: "openrouter:web_search", activationSeq: 1 },
          ]),
        ],
        onCancelTurn: (id) => cancelled.push(id),
      }),
    );
    expect(screen.getByTestId("comment-pending-status")).toHaveTextContent(
      "Searching the web…",
    );
    fireEvent.click(screen.getByTestId("comment-pending-cancel"));
    expect(cancelled).toEqual(["turn-1"]);
  });
});

describe("CTA-FR-NMNE: a composer that OPENS a conversation names no recipient", () => {
  it("keeps its own placeholder where the tab already has active agents", () => {
    // CTA-FR-NMNE carves this case out: there is no conversation yet for an agent
    // to be active in, so the draft card's composer names what is to be
    // discussed instead and lists nobody.
    mount({
      threads: [addressed(anchored("t1", { start: 0, end: 3, quote: "abc" }))],
      anchorTops: { t1: 0 },
      agents: [projectAgent("arch", "a1"), projectAgent("sec", "a2")],
      draftQuote: "a passage worth arguing about",
    });
    const opening = screen.getByLabelText("New comment");
    expect(opening.getAttribute("placeholder")).toBe("Comment…");
    const draft = screen.getByTestId("comment-draft");
    expect(draft.textContent ?? "").not.toContain("@arch");
  });
});
