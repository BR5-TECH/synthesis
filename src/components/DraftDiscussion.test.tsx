/**
 * The two-column shape of the New Artifact tab and the reading of its
 * discussion (`../../specifications/ui/DDS-draft-discussion.md`).
 *
 * The split, the ratio and the auto-follow rule are the parts of this surface
 * that are about *position* rather than about content, and position is exactly
 * what a component test can hold still — so they are exercised here rather than
 * in the tab's own suite, which is about what the tab does.
 */
import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
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

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
const listeners = new Map<string, Set<(e: { payload: unknown }) => void>>();
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(
    async (event: string, cb: (e: { payload: unknown }) => void) => {
      const set = listeners.get(event) ?? new Set();
      set.add(cb);
      listeners.set(event, set);
      return () => set.delete(cb);
    },
  ),
}));

import {
  cycleRatio,
  documentFraction,
  gridTemplate,
  parseRatio,
  serialiseRatio,
  RATIO_PRESETS,
  MIN_FRACTION,
  MAX_FRACTION,
  clampFraction,
} from "./DraftDiscussion/ratio";
import { TAIL_SLACK } from "./discussion";
import {
  draftDiscussionState,
  resetDraftDiscussions,
  restoreDraftRatios,
  setDraftDiscussionHidden,
  setDraftRatio,
} from "../state/draftDiscussion";
import {
  getDiscussionSession,
  noteDiscussionArrivals,
  setDiscussionScroll,
} from "../state/discussionSession";
import { resetLayoutPreferencesCache } from "../state/layoutPreferences";
import {
  DRAFT,
  columnElement,
  discussionSaying as thread,
  type ColumnProps,
} from "../test/draftDiscussionFixtures";
import type { Discussion } from "../types";

function column(over: Partial<ColumnProps> = {}) {
  return columnElement(over);
}

function renderColumn(over: Partial<ColumnProps> = {}) {
  return render(column(over));
}

/** Push a new set of props through the column, as an arrival would. */
function rerenderColumn(
  rerender: (ui: React.ReactElement) => void,
  over: Partial<ColumnProps>,
) {
  act(() => {
    rerender(column(over));
  });
}

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation(async () => undefined);
  listeners.clear();
  resetDraftDiscussions();
  resetLayoutPreferencesCache();
});

afterEach(cleanup);

// ---------------------------------------------------------------------------
// The split and its presets (DDS-FR-PNXR)
// ---------------------------------------------------------------------------

describe("the split between the two columns (DDS-FR-PNXR)", () => {
  it("DDS-FR-PNXR: holds three presets and one accelerator cycles them in order", () => {
    // Three named presets rather than a free number alone, because "give the
    // discussion most of the width" is a thing an author asks for by name and
    // should not have to hit with a drag.
    expect(RATIO_PRESETS).toEqual(["doc70", "even", "chat70"]);
    expect(cycleRatio("doc70")).toBe("even");
    expect(cycleRatio("even")).toBe("chat70");
    expect(cycleRatio("chat70")).toBe("doc70");
  });

  it("DDS-FR-PNXR: a dragged split enters the cycle at the head rather than snapping onto a preset", () => {
    // Snapping to the nearest preset would make one keypress mean two different
    // things depending on a pixel the author cannot see.
    expect(cycleRatio(0.61)).toBe("doc70");
    expect(cycleRatio(0.44)).toBe("doc70");
  });

  it("DDS-FR-PNXR: the document column takes more of the width under doc70 than under chat70", () => {
    expect(documentFraction("doc70")).toBeGreaterThan(documentFraction("even"));
    expect(documentFraction("even")).toBeGreaterThan(documentFraction("chat70"));
    // Fractions rather than pixels, so the split keeps its proportion when the
    // window changes size.
    expect(gridTemplate("even")).toContain("0.5fr");
  });

  it("DDS-FR-PNXR: a drag cannot take either column below a readable measure", () => {
    // A column dragged past this stops being a column and becomes a rail, which
    // is the arrangement this surface exists to replace.
    expect(clampFraction(0.01)).toBe(MIN_FRACTION);
    expect(clampFraction(0.99)).toBe(MAX_FRACTION);
    expect(clampFraction(Number.NaN)).toBe(documentFraction("even"));
  });

  it("DDS-FR-PNXR: the chosen ratio is held per draft, and one draft's does not move another's", () => {
    setDraftRatio("d1", "chat70");
    setDraftRatio("d2", 0.62);
    expect(draftDiscussionState("d1").ratio).toBe("chat70");
    expect(draftDiscussionState("d2")).toMatchObject({ ratio: 0.62 });
    // A draft nobody has split opens on the default rather than on the last
    // draft's choice.
    expect(draftDiscussionState("d3").ratio).toBe("even");
  });

  it("DDS-FR-PNXR: a stored ratio round-trips, and one this build does not know is ignored", () => {
    expect(parseRatio(serialiseRatio("chat70"))).toBe("chat70");
    expect(parseRatio(serialiseRatio(0.62))).toBe(0.62);
    // A preset added by a later build must not break a session that reads it.
    expect(parseRatio("tiles")).toBeNull();
    expect(parseRatio(undefined)).toBeNull();
    // And an empty value is nothing rather than the narrowest split there is,
    // which is what `Number("")` would quietly make it.
    expect(parseRatio("")).toBeNull();
    expect(parseRatio("   ")).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// Auto-follow and reading history (DDS-FR-DPRJ, DDS-FR-GKMT, DDS-FR-SVBL)
// ---------------------------------------------------------------------------

describe("reading the discussion (DDS-FR-DPRJ, DDS-FR-GKMT)", () => {
  it("DDS-FR-DPRJ: the column says it is following while it is at the tail", async () => {
    renderColumn();
    expect(await screen.findByTestId("discussion-follow-state")).toHaveTextContent(
      "following",
    );
    expect(screen.queryByTestId("discussion-unread-indicator")).toBeNull();
  });

  it("DDS-FR-GKMT: scrolling away from the tail pauses the following and says so", async () => {
    renderColumn();
    await screen.findByTestId("discussion-follow-state");

    act(() => setDiscussionScroll("disc-1", 0, false));

    expect(screen.getByTestId("discussion-follow-state")).toHaveTextContent(
      "paused · reading history",
    );
  });

  it("DDS-FR-GKMT: messages arriving while the author reads back land under a divider and behind the unread indicator", async () => {
    renderColumn({
      discussions: [thread("disc-1", ["opening", "second", "third"])],
    });
    await screen.findByTestId("discussion-follow-state");

    act(() => {
      setDiscussionScroll("disc-1", 0, false);
      noteDiscussionArrivals("disc-1", ["disc-1-c1", "disc-1-c2"]);
    });

    // The count is what the author needs to decide whether to break off
    // reading, so the control says how many rather than only that some arrived.
    const pill = screen.getByTestId("discussion-unread-indicator");
    expect(pill).toHaveTextContent("2 new messages");
    // The divider marks where they stopped reading, above the first arrival.
    expect(
      screen.getByRole("separator", { name: "2 unread" }),
    ).toHaveTextContent("2 new");
  });

  it("DDS-FR-GKMT: the divider marks where reading stopped, not where the newest message is", () => {
    act(() => setDiscussionScroll("disc-1", 0, false));
    act(() => noteDiscussionArrivals("disc-1", ["c1"]));
    act(() => noteDiscussionArrivals("disc-1", ["c2", "c3"]));

    const held = getDiscussionSession("disc-1");
    expect(held.unreadCount).toBe(3);
    // Still the first one: moving the mark onto the newest arrival would move it
    // away from what the author has not read.
    expect(held.firstUnreadId).toBe("c1");
  });

  it("DDS-FR-DPRJ, DDS-FR-GKMT: returning to the tail resumes following and clears the divider", () => {
    act(() => setDiscussionScroll("disc-1", 0, false));
    act(() => noteDiscussionArrivals("disc-1", ["c1", "c2"]));
    expect(getDiscussionSession("disc-1").unreadCount).toBe(2);

    act(() => setDiscussionScroll("disc-1", 0, true));

    // Positional, never a toggle: coming back to the foot IS having read them.
    expect(getDiscussionSession("disc-1")).toMatchObject({
      atTail: true,
      unreadCount: 0,
      firstUnreadId: null,
    });
  });

  it("DDS-FR-DPRJ: a message arriving while the column is at the tail leaves no unread mark", () => {
    act(() => noteDiscussionArrivals("disc-1", ["c1"]));
    expect(getDiscussionSession("disc-1").unreadCount).toBe(0);
    expect(getDiscussionSession("disc-1").firstUnreadId).toBeNull();
  });

  it("DDS-FR-SVBL: the anchor bar stands only while a proposal stands and the author is reading back", async () => {
    const onActivate = vi.fn();
    renderColumn({
      anchor: {
        label: "@arch · proposal of 3 changes",
        action: "Back to change 2",
        onActivate,
      },
    });
    await screen.findByTestId("discussion-follow-state");

    // At the tail there is nothing the author has scrolled away from, so the
    // bar would name a change that is already on screen.
    expect(screen.queryByTestId("discussion-anchor-bar")).toBeNull();

    act(() => setDiscussionScroll("disc-1", 0, false));

    const bar = screen.getByTestId("discussion-anchor-bar");
    expect(bar).toHaveTextContent("@arch · proposal of 3 changes");
    await userEvent.click(
      within(bar).getByRole("button", { name: "Back to change 2" }),
    );
    expect(onActivate).toHaveBeenCalledTimes(1);
  });

  it("DDS-FR-SVBL: a draft with no standing proposal shows no anchor bar however far the author scrolls", async () => {
    renderColumn({ anchor: null });
    await screen.findByTestId("discussion-follow-state");
    act(() => setDiscussionScroll("disc-1", 0, false));
    expect(screen.queryByTestId("discussion-anchor-bar")).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// The transcript itself (DDS-FR-HGMB, DDS-FR-CLBK)
// ---------------------------------------------------------------------------

describe("the transcript (DDS-FR-HGMB, DDS-FR-CLBK)", () => {
  it("DDS-FR-HGMB: a message is rendered with its author and its body, in the column's own card", async () => {
    renderColumn({ discussions: [thread("disc-1", ["we should split the ontology"])] });
    const column = await screen.findByTestId("draft-discussion-column");
    const card = within(column).getByTestId("comment-thread-disc-1");
    expect(within(card).getByText("we should split the ontology")).toBeInTheDocument();
    expect(within(card).getAllByText("raver119").length).toBeGreaterThan(0);
  });

  it("DDS-FR-CLBK: a long discussion shows its head as one row, which expands in place", async () => {
    const many = Array.from({ length: 40 }, (_, i) => `message ${i}`);
    renderColumn({ discussions: [thread("disc-1", many)] });

    const collapsed = await screen.findByTestId("discussion-earlier");
    expect(collapsed).toHaveTextContent("28 earlier messages");
    // The oldest is not rendered, and the newest is.
    expect(screen.queryByText("message 0")).toBeNull();
    expect(screen.getByText("message 39")).toBeInTheDocument();

    await userEvent.click(collapsed);

    // Expanded in place: the whole discussion is here, and the row is gone.
    await waitFor(() => expect(screen.getByText("message 0")).toBeInTheDocument());
    expect(screen.queryByTestId("discussion-earlier")).toBeNull();
  });

  it("DDS-FR-CLBK: a discussion short enough to read whole is never collapsed", async () => {
    renderColumn({ discussions: [thread("disc-1", ["one", "two", "three"])] });
    await screen.findByTestId("draft-discussion-column");
    expect(screen.queryByTestId("discussion-earlier")).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// Auto-follow at the scroll position (DDS-FR-DPRJ, DDS-FR-GKMT)
// ---------------------------------------------------------------------------

/**
 * jsdom lays nothing out, so every scroller measures zero. These stubs are what
 * make the *positional* half of auto-follow testable at all — without them
 * `atTail()` reads `0 - 0 - 0 <= 24` and is true whatever the author did, which
 * is the one thing this rule is about.
 */
function measure(
  el: HTMLElement,
  { scrollHeight, clientHeight, scrollTop }: {
    scrollHeight: number;
    clientHeight: number;
    scrollTop: number;
  },
) {
  Object.defineProperty(el, "scrollHeight", { value: scrollHeight, configurable: true });
  Object.defineProperty(el, "clientHeight", { value: clientHeight, configurable: true });
  let top = scrollTop;
  Object.defineProperty(el, "scrollTop", {
    configurable: true,
    get: () => top,
    set: (next: number) => {
      top = next;
    },
  });
}

/** The scroller the messages are in, which is the card's own message list. */
function scroller(): HTMLElement {
  const el = document.querySelector(".comment-card__messages");
  if (!el) throw new Error("the discussion column has no message scroller");
  return el as HTMLElement;
}

describe("auto-follow reads the scroll position (DDS-FR-DPRJ, DDS-FR-GKMT)", () => {
  it("DDS-FR-DPRJ: a list scrolled to its foot follows, and one scrolled away does not", async () => {
    renderColumn({ discussions: [thread("disc-1", ["one", "two", "three"])] });
    await screen.findByTestId("draft-discussion-column");
    const el = scroller();

    // At the foot: within the slack, which is what "at the tail" means.
    measure(el, { scrollHeight: 1000, clientHeight: 400, scrollTop: 590 });
    act(() => {
      fireEvent.scroll(el);
    });
    expect(getDiscussionSession("disc-1").atTail).toBe(true);
    expect(screen.getByTestId("discussion-follow-state")).toHaveTextContent(
      "following",
    );

    // Scrolled up: the author is reading back, and the column says so.
    measure(el, { scrollHeight: 1000, clientHeight: 400, scrollTop: 200 });
    act(() => {
      fireEvent.scroll(el);
    });
    expect(getDiscussionSession("disc-1").atTail).toBe(false);
    expect(screen.getByTestId("discussion-follow-state")).toHaveTextContent(
      "paused · reading history",
    );
  });

  it("DDS-FR-DPRJ: the slack is what decides, at the pixel", async () => {
    // A list pinned to its foot is rarely at exactly zero — a fractional device
    // pixel ratio and a growing composer both leave a pixel or two behind. Zero
    // would stop the column following for a distance nobody can see.
    renderColumn();
    await screen.findByTestId("draft-discussion-column");
    const el = scroller();

    measure(el, { scrollHeight: 1000, clientHeight: 400, scrollTop: 600 - TAIL_SLACK });
    act(() => {
      fireEvent.scroll(el);
    });
    expect(getDiscussionSession("disc-1").atTail).toBe(true);

    measure(el, {
      scrollHeight: 1000,
      clientHeight: 400,
      scrollTop: 600 - TAIL_SLACK - 1,
    });
    act(() => {
      fireEvent.scroll(el);
    });
    expect(getDiscussionSession("disc-1").atTail).toBe(false);
  });

  it("DDS-FR-GKMT: a message arriving while the author reads back does not move the view", async () => {
    const { rerender } = renderColumn({
      discussions: [thread("disc-1", ["one", "two"])],
    });
    await screen.findByTestId("draft-discussion-column");
    const el = scroller();
    measure(el, { scrollHeight: 1000, clientHeight: 400, scrollTop: 120 });
    act(() => {
      fireEvent.scroll(el);
    });
    expect(getDiscussionSession("disc-1").atTail).toBe(false);

    rerenderColumn(rerender, {
      discussions: [thread("disc-1", ["one", "two", "three"])],
    });

    // Where they were reading, to the pixel — and the arrival is counted rather
    // than scrolled to.
    expect(el.scrollTop).toBe(120);
    expect(getDiscussionSession("disc-1").unreadCount).toBe(1);
    expect(getDiscussionSession("disc-1").firstUnreadId).toBe("disc-1-c2");
  });

  it("DDS-FR-DPRJ: a message arriving at the tail is scrolled to", async () => {
    const { rerender } = renderColumn({
      discussions: [thread("disc-1", ["one", "two"])],
    });
    await screen.findByTestId("draft-discussion-column");
    const el = scroller();
    measure(el, { scrollHeight: 1000, clientHeight: 400, scrollTop: 600 });
    act(() => {
      fireEvent.scroll(el);
    });

    rerenderColumn(rerender, {
      discussions: [thread("disc-1", ["one", "two", "three"])],
    });

    expect(el.scrollTop).toBe(1000);
    expect(getDiscussionSession("disc-1").unreadCount).toBe(0);
  });

  it("DDS-FR-GKMT, CVP-FR-TWRL: the unread indicator takes the author to the first unread message and clears the divider", async () => {
    const scrollIntoView = vi.fn();
    Object.defineProperty(HTMLElement.prototype, "scrollIntoView", {
      value: scrollIntoView,
      configurable: true,
    });
    const { rerender } = renderColumn({
      discussions: [thread("disc-1", ["one", "two"])],
    });
    await screen.findByTestId("draft-discussion-column");
    const el = scroller();
    measure(el, { scrollHeight: 1000, clientHeight: 400, scrollTop: 120 });
    act(() => {
      fireEvent.scroll(el);
    });
    rerenderColumn(rerender, {
      discussions: [thread("disc-1", ["one", "two", "three"])],
    });
    scrollIntoView.mockClear();

    await userEvent.click(screen.getByTestId("discussion-unread-indicator"));

    // The first arrival is what the author is taken to, and the unread state
    // goes with the visit.
    expect(scrollIntoView).toHaveBeenCalledTimes(1);
    expect(getDiscussionSession("disc-1")).toMatchObject({
      unreadCount: 0,
      firstUnreadId: null,
    });
    expect(screen.queryByTestId("discussion-unread-indicator")).toBeNull();
    expect(screen.queryByRole("separator", { name: /unread/ })).toBeNull();
    // The reading position is the author's: the visit does not claim the tail.
    expect(getDiscussionSession("disc-1").atTail).toBe(false);
    delete (HTMLElement.prototype as { scrollIntoView?: unknown }).scrollIntoView;
  });

  it("DDS-FR-GKMT: the divider is drawn above the first arrival, not above the newest message", async () => {
    const { rerender } = renderColumn({
      discussions: [thread("disc-1", ["one", "two"])],
    });
    await screen.findByTestId("draft-discussion-column");
    const el = scroller();
    measure(el, { scrollHeight: 1000, clientHeight: 400, scrollTop: 120 });
    act(() => {
      fireEvent.scroll(el);
    });

    rerenderColumn(rerender, {
      discussions: [thread("disc-1", ["one", "two", "three"])],
    });
    rerenderColumn(rerender, {
      discussions: [thread("disc-1", ["one", "two", "three", "four"])],
    });

    const divider = screen.getByRole("separator", { name: "2 unread" });
    const third = screen.getByText("three");
    const fourth = screen.getByText("four");
    // The mark is where they stopped reading. Drawn above the newest message it
    // would sit *below* the first thing they have not read, which is the one
    // message the mark exists to point at.
    expect(
      divider.compareDocumentPosition(third) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(
      divider.compareDocumentPosition(fourth) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });
});

// ---------------------------------------------------------------------------
// A column with no transcript (DDS-FR-DPRJ, DDS-FR-GKMT, CMT-FR-17, CMT-FR-56)
// ---------------------------------------------------------------------------

describe("a column showing no transcript (DDS-FR-DPRJ, DDS-FR-GKMT)", () => {
  it("DDS-FR-DPRJ, DDS-FR-GKMT: says nothing about following while there is no transcript", async () => {
    // A badge here would describe a transcript the author cannot see. The
    // author reported exactly that: `paused · reading history` over a column
    // holding no discussion at all.
    renderColumn({ discussions: [] });
    await screen.findByTestId("draft-open-discussion");
    expect(screen.queryByTestId("discussion-follow-state")).toBeNull();
  });

  it("DDS-FR-GKMT, CVP-FR-47: a transcript leaving the column leaves no badge, and keeps its reading position for the way back", async () => {
    // The state the author hit: scroll a discussion up, resolve it, and the
    // column is left paused on a transcript that is gone — with no scroller
    // left, no scroll can ever put it back to the tail. Resolving is how the
    // author reached it, so that is what this does. The badge is about what is
    // on screen, so it goes; the position is the discussion's, so it stays.
    const only = thread("disc-1", ["opening"]);
    const { rerender } = renderColumn({ discussions: [only] });
    await screen.findByTestId("discussion-follow-state");
    act(() => setDiscussionScroll("disc-1", 0, false));
    expect(getDiscussionSession("disc-1").atTail).toBe(false);

    rerenderColumn(rerender, { discussions: [{ ...only, resolved: true }] });

    expect(screen.queryByTestId("discussion-follow-state")).toBeNull();
    expect(screen.queryByTestId("discussion-anchor-bar")).toBeNull();
    // DDS-FR-GKMT: nor the divider, nor the way to unread messages there are not.
    expect(screen.queryByTestId("discussion-unread-indicator")).toBeNull();
    expect(screen.queryByRole("separator", { name: /unread/ })).toBeNull();
    expect(getDiscussionSession("disc-1").atTail).toBe(false);
  });

  it("DDS-FR-GKMT: choosing another discussion opens it rather than counting it as arrivals", async () => {
    // Every message changes at once when the author picks another discussion.
    // Counting those as arrivals would report a whole conversation the author
    // has never been shown as unread, where DDS-FR-GKMT counts a message that
    // ARRIVED.
    const first = thread("disc-1", ["opening"]);
    const second = thread("disc-2", ["a", "b", "c"]);
    const { rerender } = renderColumn({
      discussions: [first, second],
      selectedThreadId: "disc-1",
    });
    await screen.findByTestId("discussion-follow-state");
    act(() => setDiscussionScroll("disc-1", 0, false));

    rerenderColumn(rerender, {
      discussions: [first, second],
      selectedThreadId: "disc-2",
    });

    expect(getDiscussionSession("disc-2").unreadCount).toBe(0);
    expect(getDiscussionSession("disc-2").firstUnreadId).toBeNull();
    // A discussion opened is read from its tail, exactly as the column's first
    // is (DDS-FR-DPRJ).
    expect(getDiscussionSession("disc-2").atTail).toBe(true);
    expect(screen.getByTestId("discussion-follow-state")).toHaveTextContent(
      "following",
    );
  });

  it("CMT-FR-17, CMT-FR-56: a draft whose discussions are all resolved keeps the disclosure that reaches them", async () => {
    // The disclosure is pinned at the foot of the chooser, and the chooser used
    // to stand only while an OPEN discussion existed — so resolving the last
    // one took the way back to it off the surface entirely.
    const only = thread("disc-1", ["opening"]);
    renderColumn({
      discussions: [
        { ...only, resolved: true },
      ],
    });

    const disclosure = await screen.findByRole("button", { name: /1 resolved/ });
    expect(disclosure).toHaveAttribute("aria-expanded", "false");
    await userEvent.click(disclosure);
    expect(await screen.findByTestId("comment-thread-disc-1")).toBeInTheDocument();
  });

  it("CMT-FR-56: a reopened discussion returns to the chooser and leaves the resolved count", async () => {
    const only = thread("disc-1", ["opening"]);
    const resolved = { ...only, resolved: true };
    const { rerender } = renderColumn({ discussions: [resolved] });
    await screen.findByRole("button", { name: /1 resolved/ });
    expect(screen.queryAllByRole("tab")).toHaveLength(0);

    rerenderColumn(rerender, { discussions: [only] });

    expect(screen.queryByRole("button", { name: /1 resolved/ })).toBeNull();
    expect(screen.getAllByRole("tab")).toHaveLength(1);
    await screen.findByTestId("comment-thread-disc-1");
  });

  it("DDS layout: a draft nobody has spoken about is told so", async () => {
    renderColumn({ discussions: [] });
    expect(
      await screen.findByText("Nothing has been said about this draft yet."),
    ).toBeInTheDocument();
    expect(screen.getByTestId("draft-open-discussion")).toBeInTheDocument();
  });

  it("DDS layout: a draft whose discussions are all resolved says that, and where they are", async () => {
    // Resolved is not unsaid — and a column that says nothing at all is a band
    // of empty panel the author reads as a fault. The statement names the state
    // the column is in and where the discussions it is about can be read.
    const only = thread("disc-1", ["opening"]);
    const second = thread("disc-2", ["opening"]);
    const resolve = (d: Discussion) => ({ ...d, resolved: true });
    const { rerender } = renderColumn({ discussions: [resolve(only)] });

    expect(
      await screen.findByText("The discussion about this draft is resolved."),
    ).toBeInTheDocument();
    expect(
      screen.queryByText("Nothing has been said about this draft yet."),
    ).toBeNull();
    expect(screen.getByTestId("draft-open-discussion")).toBeInTheDocument();

    // The count is what the statement is about, so it agrees with the
    // disclosure beside it rather than reading as one discussion whatever the
    // draft holds.
    rerenderColumn(rerender, { discussions: [resolve(only), resolve(second)] });
    expect(
      await screen.findByText("Every discussion about this draft is resolved."),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /2 resolved/ })).toBeInTheDocument();
  });

  it("DDS layout: opening another discussion says that is what it does", async () => {
    renderColumn({ discussions: [thread("disc-1", ["opening"])] });
    await userEvent.click(await screen.findByTestId("discussion-new"));

    expect(await screen.findByTestId("draft-open-discussion")).toBeInTheDocument();
    expect(
      await screen.findByText("This begins another discussion about this draft."),
    ).toBeInTheDocument();
    expect(
      screen.queryByText("Nothing has been said about this draft yet."),
    ).toBeNull();
    // And no follow state, because the transcript left the column with it.
    expect(screen.queryByTestId("discussion-follow-state")).toBeNull();
  });

  it("DDS layout: a column showing no transcript is never empty and silent", async () => {
    // The rule the three statements exist for, asserted as one thing: whichever
    // state the column is in, the space above the composer says what it is.
    const only = thread("disc-1", ["opening"]);
    const resolved = { ...only, resolved: true };
    for (const discussions of [[], [resolved]]) {
      cleanup();
      renderColumn({ discussions });
      const statement = await screen.findByTestId("draft-open-discussion");
      expect(
        within(statement).getByText(/\S/, { selector: ".dds-open__lead" }),
      ).toBeInTheDocument();
      expect(
        within(statement).getByText(/\S/, { selector: ".dds-open__hint" }),
      ).toBeInTheDocument();
    }
  });
});

// ---------------------------------------------------------------------------
// Hiding the discussion column (DDS-FR-XQMF, SNV-FR-08)
// ---------------------------------------------------------------------------

describe("hiding the discussion column (DDS-FR-XQMF, SNV-FR-08)", () => {
  it("DDS-FR-XQMF: the state defaults to shown and is held per draft", () => {
    expect(draftDiscussionState(DRAFT).hidden).toBe(false);
    setDraftDiscussionHidden(DRAFT, true);
    expect(draftDiscussionState(DRAFT).hidden).toBe(true);
    // Hiding one draft's column says nothing about another's.
    expect(draftDiscussionState("d2").hidden).toBe(false);
    setDraftDiscussionHidden(DRAFT, false);
    expect(draftDiscussionState(DRAFT).hidden).toBe(false);
  });

  it("DDS-FR-XQMF: hiding is not narrowing — the split the author chose is kept", () => {
    setDraftRatio(DRAFT, "chat70");
    setDraftDiscussionHidden(DRAFT, true);
    expect(draftDiscussionState(DRAFT).ratio).toBe("chat70");
    setDraftDiscussionHidden(DRAFT, false);
    expect(draftDiscussionState(DRAFT).ratio).toBe("chat70");
  });

  it("SNV-FR-08, DDS-FR-PNXR, DDS-FR-XQMF: both are persisted per draft, in fields the backend names", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_layout_preferences") return {};
      return undefined;
    });
    await act(async () => {
      await restoreDraftRatios("/repo");
    });

    setDraftRatio(DRAFT, "doc70");
    setDraftDiscussionHidden(DRAFT, true);

    await waitFor(() => {
      const saves = invokeMock.mock.calls.filter(
        (c) => c[0] === "save_layout_preferences",
      );
      const last = saves[saves.length - 1]?.[1] as
        | { preferences: Record<string, unknown> }
        | undefined;
      // Byte-for-byte the names `src-tauri/src/layout.rs` declares. A mismatch
      // is not an error — it is dropped on save and absent on load.
      expect(last?.preferences.draftDiscussionRatios).toEqual({ [DRAFT]: "doc70" });
      expect(last?.preferences.draftDiscussionHidden).toEqual({ [DRAFT]: true });
    });
  });

  it("SNV-FR-08, DDS-FR-XQMF: a stored arrangement is restored, and a ratio this build cannot read is ignored", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_layout_preferences") {
        return {
          draftDiscussionRatios: { [DRAFT]: "chat70", d3: "not-a-preset-or-number" },
          draftDiscussionHidden: { [DRAFT]: true },
        };
      }
      return undefined;
    });
    await act(async () => {
      await restoreDraftRatios("/repo");
    });

    expect(draftDiscussionState(DRAFT).ratio).toBe("chat70");
    expect(draftDiscussionState(DRAFT).hidden).toBe(true);
    // A preset a later build wrote is ignored rather than refused, and does not
    // stop the rest of the record being read.
    expect(draftDiscussionState("d3").ratio).toBe("even");
  });

  it("SNV-FR-08, DDS-FR-XQMF: a record written before this field existed opens every draft on two columns", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_layout_preferences") {
        return { draftDiscussionRatios: { [DRAFT]: "doc70" } };
      }
      return undefined;
    });
    await act(async () => {
      await restoreDraftRatios("/repo");
    });

    expect(draftDiscussionState(DRAFT).ratio).toBe("doc70");
    expect(draftDiscussionState(DRAFT).hidden).toBe(false);
  });

  it("SNV-FR-08: one draft's arrangement never drops another's from the record", async () => {
    // The two writes are separate and the field they share is one. A map folded
    // together before the patch rather than inside it would replace the stored
    // map wholesale, so hiding one draft's column would silently take away the
    // split another draft was left at.
    invokeMock.mockImplementation(async (cmd: string) =>
      cmd === "load_layout_preferences" ? {} : undefined,
    );
    await act(async () => {
      await restoreDraftRatios("/repo");
    });

    setDraftDiscussionHidden("d1", true);
    setDraftDiscussionHidden("d2", true);
    setDraftRatio("d1", "doc70");
    setDraftRatio("d2", "chat70");

    await waitFor(() => {
      const saves = invokeMock.mock.calls.filter(
        (c) => c[0] === "save_layout_preferences",
      );
      const last = saves[saves.length - 1]?.[1] as
        | { preferences: Record<string, unknown> }
        | undefined;
      expect(last?.preferences.draftDiscussionHidden).toEqual({
        d1: true,
        d2: true,
      });
      expect(last?.preferences.draftDiscussionRatios).toEqual({
        d1: "doc70",
        d2: "chat70",
      });
    });
  });
});
