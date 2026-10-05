/**
 * Reviewing a proposal inside the document
 * (`../../specifications/ui/DCR-draft-change-review.md`,
 * `../../specifications/ui/DDS-draft-discussion.md` DDS-FR-ZRPT).
 *
 * These are the parts of the review that are logic rather than layout: which
 * change is under review, what a decision invokes, what a refusal leaves
 * standing, and the guard on the accelerators that decide.
 */
import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import { ReviewBar } from "./DraftDiscussion";
import { undecoratedByDecision } from "./DraftDiscussion/hunkDecorations";
import { GutterMap } from "./DraftDiscussion/GutterMap";
import { HunkChip } from "./DraftDiscussion/HunkChip";
import { decisionMessage } from "./DraftDiscussion/messages";
import { HUNK_ATTR, hunkLabel, type DecoratedHunk } from "./DraftDiscussion/hunkDecorations";
import { stepTo, useHunkNavigation } from "./DraftDiscussion/useHunkNavigation";
import { useScrollToHunk } from "./DraftDiscussion/useScrollToHunk";
import {
  draftDiscussionState,
  releaseHunkFocus,
  resetDraftDiscussions,
  setFocusedHunk,
  useDraftDiscussion,
} from "../state/draftDiscussion";
import { PROPOSAL_ERRORS } from "../types";
import type { DraftChangeProposal } from "../types";

const DRAFT = "d1";

function hunk(over: Partial<DecoratedHunk> = {}): DecoratedHunk {
  return {
    id: "h1",
    kind: "replace",
    state: "pending",
    from: 4,
    to: 20,
    after: "the better line",
    lost: false,
    position: 1,
    total: 3,
    agent: "@arch",
    editable: true,
    ...over,
  };
}

function proposal(over: Partial<DraftChangeProposal> = {}): DraftChangeProposal {
  return {
    id: "p1",
    draftId: DRAFT,
    path: "prompt.md",
    agent: { kind: "agent", agentId: "a1", handle: "arch", model: "m" },
    rationale: "The opening buries the point.",
    threadId: "t1",
    commentId: "c1",
    state: "pending",
    candidateEdited: false,
    legacy: false,
    hunkCount: 3,
    counts: { pending: 3, accepted: 0, rejected: 0, discussing: 0 },
    ledger: [
      { id: "h1", kind: "replace", state: "pending", edited: false, revision: 0 },
      { id: "h2", kind: "add", state: "pending", edited: false, revision: 0 },
      { id: "h3", kind: "del", state: "pending", edited: false, revision: 0 },
    ],
    createdAt: "2026-01-01T00:00:00Z",
    ...over,
  };
}

beforeEach(() => {
  invokeMock.mockReset();
  resetDraftDiscussions();
});

afterEach(cleanup);

// ---------------------------------------------------------------------------
// The review bar (DCR-FR-11, DCR-FR-16, DCR-FR-CXZG)
// ---------------------------------------------------------------------------

describe("the review bar (DCR-FR-11)", () => {
  it("DCR-FR-11: names the change under review and how many the proposal holds", () => {
    render(
      <ReviewBar
        proposal={proposal()}
        position={2}
        busy={false}
        error={null}
        conflict={null}
        standing="The draft has not changed."
        canPrevious
        canNext
        undrawn={false}
        onPrevious={vi.fn()}
        onNext={vi.fn()}
        onAcceptAll={vi.fn()}
        onRejectAll={vi.fn()}
      />,
    );
    // By name, not by position: the bar stands at the foot of the column and
    // is read upward, so which status comes first is a layout decision.
    expect(screen.getByTestId("review-counter")).toHaveTextContent(
      "reviewing change 2 of 3",
    );
    // DCR-FR-27: and beside it, whether the draft has moved yet.
    expect(screen.getByTestId("review-standing")).toHaveTextContent(
      "The draft has not changed.",
    );
  });

  it("DCR-FR-11: carries the two move controls beside the counter, each naming its accelerator", async () => {
    const onPrevious = vi.fn();
    const onNext = vi.fn();
    render(
      <ReviewBar
        proposal={proposal()}
        position={2}
        busy={false}
        error={null}
        conflict={null}
        standing="The draft has not changed."
        canPrevious
        canNext
        undrawn={false}
        onPrevious={onPrevious}
        onNext={onNext}
        onAcceptAll={vi.fn()}
        onRejectAll={vi.fn()}
      />,
    );
    // The accelerator is named on the control, so it is learned from the thing
    // that does the same job rather than having to be known first.
    const previous = screen.getByRole("button", { name: /Previous/ });
    const next = screen.getByRole("button", { name: /Next/ });
    expect(previous).toHaveTextContent("⌥↑");
    expect(next).toHaveTextContent("⌥↓");
    await userEvent.click(previous);
    await userEvent.click(next);
    expect(onPrevious).toHaveBeenCalledTimes(1);
    expect(onNext).toHaveBeenCalledTimes(1);
  });

  it("DCR-FR-11: a bar with nowhere to move keeps both controls, unavailable", () => {
    render(
      <ReviewBar
        proposal={proposal()}
        position={1}
        busy={false}
        error={null}
        conflict={null}
        standing="The draft has not changed."
        canPrevious={false}
        canNext={false}
        undrawn={false}
        onPrevious={vi.fn()}
        onNext={vi.fn()}
        onAcceptAll={vi.fn()}
        onRejectAll={vi.fn()}
      />,
    );
    // A move rounds both ends, so this is not the last change or the first —
    // it is a proposal holding the one change the review is already on. Still
    // rendered, so the bar keeps its shape and the counter beside it does not
    // move under the pointer.
    expect(screen.getByRole("button", { name: /Previous/ })).toBeDisabled();
    expect(screen.getByRole("button", { name: /Next/ })).toBeDisabled();
  });

  it("DCR-FR-11, DCR-FR-CXZG: carries both whole-proposal actions, and neither once nothing is undecided", async () => {
    const onAcceptAll = vi.fn();
    const onRejectAll = vi.fn();
    const { rerender } = render(
      <ReviewBar
        proposal={proposal()}
        position={1}
        busy={false}
        error={null}
        conflict={null}
        standing="The draft has not changed."
        canPrevious
        canNext
        undrawn={false}
        onPrevious={vi.fn()}
        onNext={vi.fn()}
        onAcceptAll={onAcceptAll}
        onRejectAll={onRejectAll}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: "Accept all" }));
    await userEvent.click(screen.getByRole("button", { name: "Reject all" }));
    expect(onAcceptAll).toHaveBeenCalledTimes(1);
    expect(onRejectAll).toHaveBeenCalledTimes(1);

    // DCR-FR-CXZG: nothing left undecided, so neither whole-proposal action
    // offers to decide anything. That the bar goes entirely once the proposal
    // is resolved is DCR-FR-17, and is exercised through the tab in
    // `NewArtifactWorkspace.review.test.tsx` — the bar cannot show its own
    // absence.
    rerender(
      <ReviewBar
        proposal={proposal({
          counts: { pending: 0, accepted: 3, rejected: 0, discussing: 0 },
        })}
        position={0}
        busy={false}
        error={null}
        conflict={null}
        standing="The draft has not changed."
        canPrevious
        canNext
        undrawn={false}
        onPrevious={vi.fn()}
        onNext={vi.fn()}
        onAcceptAll={onAcceptAll}
        onRejectAll={onRejectAll}
      />,
    );
    expect(screen.getByRole("button", { name: "Accept all" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Reject all" })).toBeDisabled();
  });

  it("DCR-FR-16: a refused decision states its reason, announced, beside the actions that retry it", () => {
    render(
      <ReviewBar
        proposal={proposal()}
        position={1}
        busy={false}
        error="The draft file could not be written. Nothing was changed."
        conflict={null}
        standing="The draft has not changed."
        canPrevious
        canNext
        undrawn={false}
        onPrevious={vi.fn()}
        onNext={vi.fn()}
        onAcceptAll={vi.fn()}
        onRejectAll={vi.fn()}
      />,
    );
    expect(screen.getByRole("alert")).toHaveTextContent(
      "The draft file could not be written. Nothing was changed.",
    );
    // Retried without retyping anything: the actions are still live.
    expect(screen.getByRole("button", { name: "Accept all" })).toBeEnabled();
  });

  it("DCR-FR-16: a decision in flight turns both whole-proposal actions off", () => {
    render(
      <ReviewBar
        proposal={proposal()}
        position={1}
        busy
        error={null}
        conflict={null}
        standing="The draft has not changed."
        canPrevious
        canNext
        undrawn={false}
        onPrevious={vi.fn()}
        onNext={vi.fn()}
        onAcceptAll={vi.fn()}
        onRejectAll={vi.fn()}
      />,
    );
    expect(screen.getByRole("button", { name: "Accept all" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Reject all" })).toBeDisabled();
  });
});

// ---------------------------------------------------------------------------
// The action chip (DCR-FR-12, DCR-FR-31)
// ---------------------------------------------------------------------------

describe("the change under review (DCR-FR-12, DCR-FR-31)", () => {
  function renderChip(over: Partial<DecoratedHunk> = {}, busy = false) {
    const host = document.createElement("div");
    document.body.append(host);
    // The chip is placed from the decorated element, which is what reserves the
    // space it sits over.
    const marked = document.createElement("div");
    marked.setAttribute("data-hunk", over.id ?? "h1");
    host.append(marked);
    const hostRef = { current: host };
    const onAccept = vi.fn();
    const onReject = vi.fn();
    const onDiscuss = vi.fn();
    render(
      <HunkChip
        hunk={hunk(over)}
        hostRef={hostRef}
        busy={busy}
        onAccept={onAccept}
        onReject={onReject}
        onDiscuss={onDiscuss}
      />,
    );
    return { onAccept, onReject, onDiscuss };
  }

  /**
   * DCR-FR-12: which element the chip is placed against.
   *
   * jsdom measures nothing, so the rects are given. What is checked is the
   * choice: the chip goes over the block the change draws of its own — the head
   * of the proposed text, or a deletion's own foot — and never over the struck
   * words, which is what it must not cover.
   */
  function placedAgainst(kind: DecoratedHunk["kind"], ownClass: string | null) {
    const host = document.createElement("div");
    document.body.append(host);
    const rect = (top: number, height: number) => () =>
      ({ top, bottom: top + height, left: 40, right: 400, height, width: 360, x: 40, y: top, toJSON: () => ({}) }) as DOMRect;
    const struck = document.createElement("div");
    struck.setAttribute("data-hunk", "h1");
    struck.getBoundingClientRect = rect(100, 60);
    host.append(struck);
    if (ownClass !== null) {
      const own = document.createElement("div");
      own.setAttribute("data-hunk", "h1");
      own.className = ownClass;
      own.getBoundingClientRect = rect(200, 80);
      host.append(own);
    }
    host.getBoundingClientRect = rect(0, 500);
    render(
      <HunkChip
        hunk={hunk({ id: "h1", kind })}
        hostRef={{ current: host }}
        busy={false}
        onAccept={vi.fn()}
        onReject={vi.fn()}
        onDiscuss={vi.fn()}
      />,
    );
    return screen.getByTestId("hunk-chip");
  }

  it("DCR-FR-12: a replacement's chip goes at the head of its proposed text", () => {
    // Against the struck text it would sit on the first line the author is
    // deciding about, which is the whole point of the reserved space.
    expect(placedAgainst("replace", "hunk hunk--add")).toHaveStyle({ top: "200px" });
  });

  it("DCR-FR-12: a deletion's chip goes in the block the deletion holds open", () => {
    expect(placedAgainst("del", "hunk hunk--del-foot")).toHaveStyle({ top: "200px" });
  });

  it("DCR-FR-12: with no block of its own it falls back to the foot of the struck text", () => {
    // A change the surface drew only a strike-through for still has to carry a
    // chip somewhere, and the foot of the passage is the one edge no line is on.
    expect(placedAgainst("del", null)).toHaveStyle({ top: "126px" });
  });

  it("DCR-FR-12: carries Accept, Reject and Discuss, and names the change's place in the proposal", async () => {
    const { onAccept, onReject, onDiscuss } = renderChip();
    const chip = screen.getByTestId("hunk-chip");
    expect(chip).toHaveTextContent("1 / 3");
    await userEvent.click(screen.getByRole("button", { name: /Accept/ }));
    await userEvent.click(screen.getByRole("button", { name: /Reject/ }));
    await userEvent.click(screen.getByRole("button", { name: /Discuss/ }));
    expect(onAccept).toHaveBeenCalledTimes(1);
    expect(onReject).toHaveBeenCalledTimes(1);
    expect(onDiscuss).toHaveBeenCalledTimes(1);
  });

  it("DCR-FR-31: a change the prompt no longer holds cannot be accepted, and can still be rejected", () => {
    // Rejecting is the only act left that clears such a change, so disabling
    // both would leave the proposal undecidable.
    renderChip({ lost: true });
    expect(screen.getByRole("button", { name: /Accept/ })).toBeDisabled();
    expect(screen.getByRole("button", { name: /Reject/ })).toBeEnabled();
    expect(screen.getByRole("status")).toHaveTextContent(
      "no longer in the prompt",
    );
  });

  it("DCR-FR-16: a decision in flight turns every one of the change's actions off", () => {
    renderChip({}, true);
    expect(screen.getByRole("button", { name: /Accept/ })).toBeDisabled();
    expect(screen.getByRole("button", { name: /Reject/ })).toBeDisabled();
    expect(screen.getByRole("button", { name: /Discuss/ })).toBeDisabled();
  });

  it("DCR-FR-10, DCR-FR-28: a change reached by keyboard is identified without its decoration", () => {
    // The decoration says nothing to a screen reader, so the name carries the
    // change's place, its kind, and who proposed it.
    expect(hunkLabel(hunk())).toBe(
      "Change 1 of 3, replacement proposed by @arch",
    );
    expect(hunkLabel(hunk({ kind: "add", position: 2 }))).toContain("insertion");
    expect(hunkLabel(hunk({ kind: "del" }))).toContain("deletion");
    expect(hunkLabel(hunk({ lost: true }))).toContain(
      "no longer in the prompt",
    );
  });
});

// ---------------------------------------------------------------------------
// The gutter map (DCR-FR-BQNL)
// ---------------------------------------------------------------------------

describe("the gutter map (DCR-FR-BQNL)", () => {
  it("DCR-FR-BQNL: marks every change by kind, so one outside the viewport is still discoverable", async () => {
    const onSelect = vi.fn();
    render(
      <GutterMap
        hunks={[
          hunk({ id: "h1", kind: "add", from: 0 }),
          hunk({ id: "h2", kind: "del", from: 50 }),
          hunk({ id: "h3", kind: "replace", from: 100 }),
        ]}
        focused="h2"
        extent={100}
        onSelect={onSelect}
      />,
    );
    const map = screen.getByTestId("hunk-gutter-map");
    const marks = map.querySelectorAll(".dds-gutter__mark");
    // DCR-FR-28: real controls with names, so the map is a route a keyboard can
    // take rather than a set of coloured spans only a pointer can reach.
    expect(within(map).getAllByRole("button")).toHaveLength(3);
    expect(marks[1].getAttribute("aria-label")).toContain("Change 1 of 3");
    expect(marks).toHaveLength(3);
    expect(marks[0].getAttribute("data-kind")).toBe("add");
    expect(marks[1].getAttribute("data-kind")).toBe("del");
    expect(marks[2].getAttribute("data-kind")).toBe("replace");
    // Placed by where the change is in the document, so the map reads as the
    // document does.
    expect((marks[0] as HTMLElement).style.top).toBe("0%");
    expect((marks[2] as HTMLElement).style.top).toBe("100%");
    expect(marks[1].getAttribute("data-focused")).toBe("true");

    fireEvent.click(marks[2]);
    expect(onSelect).toHaveBeenCalledWith("h3");
  });

  it("DCR-FR-BQNL: a proposal with nothing left to show draws no track at all", () => {
    render(
      <GutterMap hunks={[]} focused={null} extent={100} onSelect={vi.fn()} />,
    );
    expect(screen.queryByTestId("hunk-gutter-map")).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// The keyboard model (DCR-FR-30)
// ---------------------------------------------------------------------------

describe("moving between changes and deciding one (DCR-FR-30)", () => {
  function Harness({
    onAccept,
    onReject,
    active = true,
  }: {
    onAccept: (id: string) => void;
    onReject: (id: string) => void;
    active?: boolean;
  }) {
    // The change the review is on, exactly as the tab derives it: the store's
    // own focus, and the first change until the author has moved.
    const focused = useDraftDiscussion(DRAFT).focusedHunkId ?? "h1";
    useHunkNavigation({
      draftId: DRAFT,
      hunks: [hunk({ id: "h1" }), hunk({ id: "h2" }), hunk({ id: "h3" })],
      focused,
      active,
      onAccept,
      onReject,
    });
    return null;
  }

  it("DCR-FR-30: the move keys walk the changes in order and wrap at both ends", () => {
    render(<Harness onAccept={vi.fn()} onReject={vi.fn()} />);

    // The review opens on the first change — the bar says so and the chip is
    // drawn there — so the first press **moves**. Landing on the change the
    // author is already looking at would read as a key that does nothing, which
    // is exactly how it reads.
    act(() => {
      fireEvent.keyDown(window, { key: "ArrowDown", altKey: true });
    });
    expect(draftDiscussionState(DRAFT).focusedHunkId).toBe("h2");

    act(() => {
      fireEvent.keyDown(window, { key: "ArrowDown", altKey: true });
    });
    expect(draftDiscussionState(DRAFT).focusedHunkId).toBe("h3");

    // Round the end rather than stopping: the review is a ring of changes, and
    // stopping would strand the author at whichever end they walked to.
    act(() => {
      fireEvent.keyDown(window, { key: "ArrowDown", altKey: true });
    });
    expect(draftDiscussionState(DRAFT).focusedHunkId).toBe("h1");
    act(() => {
      fireEvent.keyDown(window, { key: "ArrowUp", altKey: true });
    });
    expect(draftDiscussionState(DRAFT).focusedHunkId).toBe("h3");
  });

  it("DCR-FR-30: the decide keys act while hunk navigation holds focus", () => {
    const onAccept = vi.fn();
    const onReject = vi.fn();
    render(<Harness onAccept={onAccept} onReject={onReject} />);

    act(() => setFocusedHunk(DRAFT, "h2"));
    act(() => {
      fireEvent.keyDown(window, { key: "Enter" });
    });
    expect(onAccept).toHaveBeenCalledWith("h2");

    act(() => {
      fireEvent.keyDown(window, { key: "Backspace" });
    });
    expect(onReject).toHaveBeenCalledWith("h2");
  });

  it("DCR-FR-30: the decide keys do nothing while the caret is in the prose", () => {
    // The editing surface binds both keys already. Unguarded, this accelerator
    // would decide a change every time the author started a new paragraph —
    // which is the one failure the guard exists to prevent.
    const onAccept = vi.fn();
    const onReject = vi.fn();
    render(<Harness onAccept={onAccept} onReject={onReject} />);

    act(() => setFocusedHunk(DRAFT, "h2"));
    act(() => releaseHunkFocus(DRAFT));

    act(() => {
      fireEvent.keyDown(window, { key: "Enter" });
      fireEvent.keyDown(window, { key: "Backspace" });
    });
    expect(onAccept).not.toHaveBeenCalled();
    expect(onReject).not.toHaveBeenCalled();

    // The move keys are still live, because moving decides nothing — and it is
    // the way back into the review from the prose.
    act(() => {
      fireEvent.keyDown(window, { key: "ArrowDown", altKey: true });
    });
    expect(draftDiscussionState(DRAFT).hunkFocusHeld).toBe(true);
  });

  it("DCR-FR-23: a draft with nothing to review binds neither key", () => {
    const onAccept = vi.fn();
    const onReject = vi.fn();
    render(<Harness onAccept={onAccept} onReject={onReject} active={false} />);

    act(() => setFocusedHunk(DRAFT, "h2"));
    act(() => {
      fireEvent.keyDown(window, { key: "Enter" });
      fireEvent.keyDown(window, { key: "ArrowDown", altKey: true });
    });
    expect(onAccept).not.toHaveBeenCalled();
    expect(draftDiscussionState(DRAFT).focusedHunkId).toBe("h2");
  });

  it("DCR-FR-30: a decide key carrying a modifier belongs to whatever claimed it", () => {
    // Cmd+Enter posts a message, and this must not also accept a change.
    const onAccept = vi.fn();
    render(<Harness onAccept={onAccept} onReject={vi.fn()} />);
    act(() => setFocusedHunk(DRAFT, "h1"));
    act(() => {
      fireEvent.keyDown(window, { key: "Enter", metaKey: true });
    });
    expect(onAccept).not.toHaveBeenCalled();
  });
});

// ---------------------------------------------------------------------------
// What a refusal says (DCR-FR-16, DCR-FR-31)
// ---------------------------------------------------------------------------

describe("what a refused decision says (DCR-FR-16)", () => {
  it("DCR-FR-16, DCR-FR-31: a lost change says what happened and what is left to do about it", () => {
    const said = decisionMessage(PROPOSAL_ERRORS.anchorLost);
    expect(said).toContain("no longer in the prompt");
    // The route out is named, because rejecting is the only act that clears it.
    expect(said).toContain("Reject");
  });

  it("DCR-FR-16: every typed refusal says whether the draft changed", () => {
    expect(decisionMessage(PROPOSAL_ERRORS.writeFailed)).toContain(
      "Nothing was changed",
    );
    expect(decisionMessage(PROPOSAL_ERRORS.hunkAlreadyDecided)).toContain(
      "already been decided",
    );
    expect(decisionMessage(PROPOSAL_ERRORS.hunkNotFound)).toContain(
      "no longer part of the proposal",
    );
    expect(decisionMessage(PROPOSAL_ERRORS.acceptanceInProgress)).toContain(
      "still being applied",
    );
    // An error nobody typed reaches the author verbatim rather than being
    // replaced by a guess about what it meant.
    expect(decisionMessage("kaboom")).toBe("kaboom");
  });
});

// ---------------------------------------------------------------------------
// What a decision leaves standing (DCR-FR-20, DCR-FR-WRJP)
// ---------------------------------------------------------------------------

describe("what one decision leaves standing (DCR-FR-20, DCR-FR-WRJP)", () => {
  it("DCR-FR-20: a change accepted or rejected leaves the document, and every other change stays", () => {
    // Acceptance is per change: deciding one must leave the rest exactly where
    // they were, which is what the author is looking at while they decide the
    // next one.
    expect(undecoratedByDecision(hunk({ state: "accepted" }))).toBe(true);
    expect(undecoratedByDecision(hunk({ state: "rejected" }))).toBe(true);
    expect(undecoratedByDecision(hunk({ state: "pending" }))).toBe(false);
    // A change held for discussion is undecided, so it is still drawn and still
    // holds the draft's one pending slot (DCP-FR-PWSF).
    expect(undecoratedByDecision(hunk({ state: "discussing" }))).toBe(false);
  });

  it("DCR-FR-WRJP: a revision replaces the change's decoration in place and leaves it undecided", () => {
    // The change is the same change — every reply addressed to it still
    // addresses it — so what must not move is its id, its place in the
    // proposal, and the fact that it is still the author's to decide.
    //
    // Exercised through the decoration's own naming and its draw rule rather
    // than by spreading an object over itself: a revision that quietly ended
    // the review, renumbered the proposal, or marked the change decided would
    // pass any test that only compares its own copy.
    const before = hunk({ id: "h2", position: 2, total: 3, after: "first try" });
    const revised = hunk({
      id: "h2",
      position: 2,
      total: 3,
      after: "second try",
      state: "pending",
    });

    expect(hunkLabel(revised)).toBe(hunkLabel(before));
    expect(hunkLabel(revised)).toContain("Change 2 of 3");
    // Still drawn, because it is still undecided: a revision is an offer, not a
    // decision.
    expect(undecoratedByDecision(revised)).toBe(false);
    // And what changed is the text it proposes, which is what the widget draws.
    expect(revised.after).not.toBe(before.after);
  });
});

describe("where a move starts from (DCR-FR-30)", () => {
  const three = [hunk({ id: "h1" }), hunk({ id: "h2" }), hunk({ id: "h3" })];

  it("DCR-FR-30: a review that has not been moved yet steps off the change it is on", () => {
    // The review opens **on** the first change — the bar says so and the chip
    // is drawn there — so a move starts from it. Starting from nowhere spends
    // the first press arriving where the author already is, which reads as a
    // control that does nothing at all.
    expect(stepTo(three, null, 1)).toBe("h2");
    expect(stepTo(three, null, -1)).toBe("h3");
  });

  it("DCR-FR-30: a move rounds both ends rather than stopping at them", () => {
    expect(stepTo(three, "h3", 1)).toBe("h1");
    expect(stepTo(three, "h1", -1)).toBe("h3");
  });

  it("DCR-FR-30: a proposal holding nothing to decide has nowhere to move", () => {
    expect(stepTo([], null, 1)).toBeNull();
  });
});

describe("the document following the review (DCR-FR-30)", () => {
  /** A host whose marked change reports a position jsdom will not compute. */
  function host(hunkIds: string[]): HTMLElement {
    const el = document.createElement("div");
    for (const id of hunkIds) {
      const mark = document.createElement("p");
      mark.setAttribute(HUNK_ATTR, id);
      el.appendChild(mark);
    }
    el.scrollTo = vi.fn() as unknown as HTMLElement["scrollTo"];
    return el;
  }

  function Harness({
    proposalId,
    focused,
    // Not `ref`: React reads that name as a ref to attach rather than as a
    // value to pass on, and the hook would be handed nothing.
    hostRef,
  }: {
    proposalId: string | null;
    focused: string | null;
    hostRef: React.RefObject<HTMLElement | null>;
  }) {
    useScrollToHunk(hostRef, proposalId, focused);
    return null;
  }

  it("DCR-FR-30: the change a review opens on does not move the document", () => {
    const el = host(["h1", "h2"]);
    const ref = { current: el };
    render(<Harness proposalId="p1" focused="h1" hostRef={ref} />);
    // A tab that opened with a proposal standing is already showing the author
    // where they left the document.
    expect(el.scrollTo).not.toHaveBeenCalled();
  });

  it("DCR-FR-30: moving the review to another change moves the document to it", () => {
    const el = host(["h1", "h2"]);
    const ref = { current: el };
    const { rerender } = render(<Harness proposalId="p1" focused="h1" hostRef={ref} />);
    rerender(<Harness proposalId="p1" focused="h2" hostRef={ref} />);
    expect(el.scrollTo).toHaveBeenCalledTimes(1);
  });

  it("DCR-FR-30: a second proposal on the same tab opens without moving the document", () => {
    // A draft may hold another proposal once the first is resolved (DCP-FR-04)
    // and the surface is not rebuilt for it. Read as a move from the last
    // change of the proposal before it, the first change of the new one would
    // yank a document nobody asked to move.
    const el = host(["h1", "h2", "h9"]);
    const ref = { current: el };
    const { rerender } = render(<Harness proposalId="p1" focused="h1" hostRef={ref} />);
    rerender(<Harness proposalId="p1" focused="h2" hostRef={ref} />);
    expect(el.scrollTo).toHaveBeenCalledTimes(1);
    rerender(<Harness proposalId="p2" focused="h9" hostRef={ref} />);
    expect(el.scrollTo).toHaveBeenCalledTimes(1);
    // And a move **within** the new proposal moves the document again.
    rerender(<Harness proposalId="p2" focused="h1" hostRef={ref} />);
    expect(el.scrollTo).toHaveBeenCalledTimes(2);
  });

  it("DCR-FR-31: a change the document could not place scrolls nowhere and throws nothing", () => {
    const el = host(["h1"]);
    const ref = { current: el };
    const { rerender } = render(<Harness proposalId="p1" focused="h1" hostRef={ref} />);
    // Rejecting is how a lost change is cleared, and the author reaches it
    // through the bar rather than by the document arriving at it.
    rerender(<Harness proposalId="p1" focused="lost" hostRef={ref} />);
    expect(el.scrollTo).not.toHaveBeenCalled();
  });
});

describe("where a move is unavailable (DCR-FR-11, DCR-FR-30)", () => {
  it("DCR-FR-11, DCR-FR-30: a proposal holding one undecided change has nowhere to move, by the key as by the control", () => {
    // The bar and the accelerators must say the same thing. A bar offering no
    // move beside a key that quietly re-sets the focus is two rules, and the
    // author learns the wrong one from whichever they try first.
    expect(stepTo([hunk({ id: "only" })], "only", 1)).toBeNull();
    expect(stepTo([hunk({ id: "only" })], "only", -1)).toBeNull();
    expect(stepTo([hunk({ id: "only" })], null, 1)).toBeNull();
  });
});

describe("a change the review cannot draw (DCR-FR-31)", () => {
  it("DCR-FR-31: the bar says the change under review is not shown, and keeps its actions", () => {
    // The silence this replaces is what let a placement defect reach an author
    // twice: the bar counted five changes, the page drew one, and moving to
    // the others changed nothing on screen — a defect wearing the clothes of an
    // ordinary state.
    render(
      <ReviewBar
        proposal={proposal()}
        position={2}
        busy={false}
        error={null}
        conflict={null}
        standing="The draft has not changed."
        canPrevious
        canNext
        undrawn
        onPrevious={vi.fn()}
        onNext={vi.fn()}
        onAcceptAll={vi.fn()}
        onRejectAll={vi.fn()}
      />,
    );
    expect(screen.getByText(/not shown in the document/i)).toBeInTheDocument();
    // Still decidable: the backend can decide a change this surface cannot draw,
    // and rejecting is how the author clears it.
    expect(screen.getByRole("button", { name: "Accept all" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Reject all" })).toBeEnabled();
  });

  it("DCR-FR-31: a change the review draws says nothing", () => {
    render(
      <ReviewBar
        proposal={proposal()}
        position={2}
        busy={false}
        error={null}
        conflict={null}
        standing="The draft has not changed."
        canPrevious
        canNext
        undrawn={false}
        onPrevious={vi.fn()}
        onNext={vi.fn()}
        onAcceptAll={vi.fn()}
        onRejectAll={vi.fn()}
      />,
    );
    expect(screen.queryByText(/not shown in the document/i)).toBeNull();
  });
});
