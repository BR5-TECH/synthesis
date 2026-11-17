/**
 * What the plan proposals show on the board (`DBR-delivery-board.md`
 * DBR-FR-AGIN, DBR-FR-WKNI, DBR-FR-PMGS, DBR-FR-GFGG): the ghost items, the
 * edits that never carry them, and the markers and the ghost card a card
 * draws.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, renderHook, screen } from "@testing-library/react";
import type { ReactNode } from "react";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(() => Promise.reject(new Error("no backend"))) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("../../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
  flushLogs: vi.fn(),
}));

import type { FlowDocument, FlowItem } from "../../state/flowDocument";
import { ghostItemId } from "../../state/mapPlan/derive";
import { MapPlanReviewStore } from "../../state/mapPlan/reviewStore";
import { createRow, modifyRow, proposal } from "../../test/mapPlanFixtures";
import { fakeHost } from "../../test/mapPlanReviewHarness";
import type { MapPlanProposal, TrackerFacts } from "../../types";
import { BoardCard } from "../Flow/boardItems";
import type { FlowPlanChange } from "../Flow/binding";
import type { ItemFacts, ItemHandlers } from "../Flow/items";
import { MapPlanContext, type MapPlanHost } from "../MapPlanProposal";
import { useProposalBoard, withoutGhosts } from "./useProposalBoard";

const DOC: FlowDocument = {
  version: 3,
  name: "Plan",
  references: [],
  edges: [],
  items: [
    { id: "milestone-2", kind: "milestone", name: "M2" },
    { id: "issue-18", kind: "feature", name: "Model serving", parentId: "milestone-2" },
    { id: "issue-16", kind: "feature", name: "Slack", parentId: "milestone-2" },
  ],
};

const PROPOSAL = proposal([
  modifyRow("m", 18, "Model serving", [{ field: "title", before: "a", after: "b" }]),
  modifyRow("w", 16, "Slack", [{ field: "close", before: "open", after: "closed", reason: "not_planned" }]),
  createRow("a", "Retry limits", {}, { milestone: { number: 2, title: "M2" }, type: "Feature" }),
  createRow("b", "Loose", {}, { milestone: { number: 9, title: "Gone" } }),
]);

let store: MapPlanReviewStore;

/** A store that holds `p` as the proposals of the active thread. */
async function storeWith(p: MapPlanProposal) {
  const s = new MapPlanReviewStore();
  const internal = s as unknown as { state: { proposals: Map<string, MapPlanProposal[]> } };
  act(() => {
    internal.state.proposals = new Map([[p.discussionId, [p]]]);
    s.setActiveDiscussion(p.discussionId);
  });
  return s;
}

function hook(
  doc: FlowDocument | null,
  host: MapPlanHost | null = fakeHost({ kindOfType: (t) => (t ? "feature" : null) }),
  tracker: TrackerFacts[] = [],
) {
  const wrapper = ({ children }: { children: ReactNode }) => (
    <MapPlanContext.Provider value={{ store, host }}>{children}</MapPlanContext.Provider>
  );
  return renderHook(() => useProposalBoard(doc, tracker), { wrapper });
}

beforeEach(async () => {
  store = await storeWith(PROPOSAL);
});

afterEach(() => {
  store.dispose();
  cleanup();
});

describe("useProposalBoard (DBR-FR-AGIN, DBR-FR-WKNI)", () => {
  it("DBR-FR-AGIN: marks the changed issue `proposed` and the withdrawn issue `withdrawn`", () => {
    const { result } = hook(DOC);
    expect(result.current.proposal.marks.get("issue-18")).toBe("proposed");
    expect(result.current.proposal.marks.get("issue-16")).toBe("withdrawn");
  });

  it("DBR-FR-WKNI: puts a ghost item in its milestone, or with no milestone where the plan has none", () => {
    const { result } = hook(DOC);
    const items = result.current.document!.items;
    expect(items.slice(0, 2)).toEqual([
      { id: ghostItemId("p1", "a"), kind: "feature", name: "Retry limits", parentId: "milestone-2" },
      { id: ghostItemId("p1", "b"), kind: "task", name: "Loose" },
    ]);
    expect(result.current.proposal.ghosts.get(ghostItemId("p1", "a"))).toEqual({ kindKnown: true });
    expect(result.current.proposal.ghosts.get(ghostItemId("p1", "b"))).toEqual({ kindKnown: false });
    expect(DOC.items).toHaveLength(3);
  });

  it("DBR-FR-WKNI: with no plan there are marks and no ghost item", () => {
    const { result } = hook(null);
    expect(result.current.document).toBeNull();
    expect(result.current.proposal.marks.size).toBe(2);
    expect(result.current.proposal.ghosts.size).toBe(0);
  });

  it("DBR-FR-PMGS: the reviewed row names the item that takes the selected ring", () => {
    const { result } = hook(DOC);
    act(() => store.openReview("t1", "p1"));
    expect(result.current.proposal.reviewed).toBe("issue-18");
    act(() => store.select("a"));
    expect(result.current.proposal.reviewed).toBe(ghostItemId("p1", "a"));
  });
});

describe("item ids from the tracker facts (DBR-FR-AGIN, DBR-FR-PMGS)", () => {
  const fact = (itemId: string, kind: "issue" | "milestone", number: number): TrackerFacts => ({
    itemId,
    ref: { kind, number, url: "" },
    state: "open",
    closeReason: null,
    updatedAt: "",
    status: null,
    claimEligible: false,
    externalBlockers: [],
  });
  const doc: FlowDocument = {
    ...DOC,
    items: [
      { id: "m-two", kind: "milestone", name: "M2" },
      { id: "t18", kind: "feature", name: "Model serving", parentId: "m-two" },
      { id: "t16", kind: "feature", name: "Slack", parentId: "m-two" },
    ],
  };
  const tracker = [fact("m-two", "milestone", 2), fact("t18", "issue", 18), fact("t16", "issue", 16)];

  it("DBR-FR-AGIN, DBR-FR-WKNI, DBR-FR-PMGS: marks, the ghost's milestone, and the ring use the plan's own item ids", () => {
    const { result } = hook(doc, undefined, tracker);
    expect(result.current.proposal.marks.get("t18")).toBe("proposed");
    expect(result.current.proposal.marks.get("t16")).toBe("withdrawn");
    expect(result.current.proposal.marks.has("issue-18")).toBe(false);
    expect(result.current.document!.items[0].parentId).toBe("m-two");
    act(() => store.openReview("t1", "p1"));
    expect(result.current.proposal.reviewed).toBe("t18");
  });
});

describe("withoutGhosts (DBR-FR-WKNI)", () => {
  const ghost = ghostItemId("p1", "a");
  const ghosts = new Map([[ghost, { kindKnown: true }]]);
  const change: FlowPlanChange = {
    edits: [
      { kind: "move", itemId: "issue-18", containerId: "milestone-2", updatedAt: "x" },
      { kind: "move", itemId: ghost, containerId: "milestone-2", updatedAt: "x" },
    ],
    next: { ...DOC, items: [{ id: ghost, kind: "feature", name: "G" }, ...DOC.items] },
    itemIds: [ghost, "issue-18"],
  };

  it("DBR-FR-WKNI: an edit never names a ghost card, and the next plan holds none", () => {
    const out = withoutGhosts(change, ghosts);
    expect(out.edits).toEqual([change.edits[0]]);
    expect(out.next.items.map((i) => i.id)).toEqual(DOC.items.map((i) => i.id));
    expect(out.itemIds).toEqual(["issue-18"]);
  });

  it("DBR-FR-WKNI: with no ghost card the change passes as it is", () => {
    expect(withoutGhosts(change, new Map())).toBe(change);
  });
});

describe("the card (DBR-FR-GFGG, DBR-FR-AGIN, DBR-FR-WKNI)", () => {
  const handlers: ItemHandlers = {
    onDragStart: vi.fn(),
    onPointerUp: vi.fn(),
    onConnectStart: vi.fn(),
    onRename: vi.fn(),
    onStartRename: vi.fn(),
    onEndRename: vi.fn(),
    onSelectItems: vi.fn(),
    onOpenTracker: vi.fn(),
    onOpenReference: vi.fn(),
  };
  const base: ItemFacts = { blockers: [], overlap: null, pending: false, references: [] };
  const card = (item: FlowItem, facts: ItemFacts, selected = false) =>
    render(
      <BoardCard
        item={item}
        at={{ x: 0, y: 0 }}
        size={{ width: 200, height: 80 }}
        facts={facts}
        selected={selected}
        magnet={false}
        lifted={false}
        editingName={false}
        inert={false}
        handlers={handlers}
        renameOnEnd={false}
      />,
    );

  it("DBR-FR-GFGG, DBR-FR-AGIN: the proposal mark stands after pending and before the draft marker", () => {
    card({ id: "issue-18", kind: "task", name: "T", draftId: "d1" }, { ...base, pending: true, proposal: "proposed" });
    const marks = [...screen.getByTestId("flow-item-issue-18").querySelectorAll(".dm-card__markers > *")];
    expect(marks.map((m) => m.textContent)).toEqual(["pending", "proposed", "draft"]);
    expect(screen.getByTestId("flow-marker-proposal")).toHaveClass("badge--accent");
  });

  it("DBR-FR-AGIN: a withdrawn issue takes the danger badge", () => {
    card({ id: "issue-16", kind: "task", name: "T" }, { ...base, proposal: "withdrawn" });
    expect(screen.getByTestId("flow-marker-proposal")).toHaveClass("badge--danger");
    expect(screen.getByTestId("flow-marker-proposal")).toHaveTextContent("withdrawn");
  });

  it("DBR-FR-WKNI, DBR-FR-PMGS: a ghost card is dashed, says `new` and `added`, takes the ring, and cannot be dragged", () => {
    const id = ghostItemId("p1", "a");
    card({ id, kind: "feature", name: "Retry limits" }, { ...base, ghost: { kindKnown: true } }, true);
    const ghost = screen.getByTestId(`flow-item-${id}`);
    expect(ghost).toHaveClass("dm-card--ghost");
    expect(ghost).toHaveAttribute("data-selected", "true");
    expect(ghost).toHaveTextContent("new");
    expect(screen.getByTestId("flow-marker-added")).toHaveClass("badge--ok");
    expect(screen.getByTestId("flow-kind-chip")).toBeInTheDocument();
    ghost.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
    expect(handlers.onDragStart).not.toHaveBeenCalled();
    expect(ghost.querySelector(".flow-port")).toBeNull();
  });

  it("DBR-FR-WKNI: a ghost card of a kind the plan does not name shows no kind chip", () => {
    card({ id: "g", kind: "task", name: "Loose" }, { ...base, ghost: { kindKnown: false } });
    expect(screen.queryByTestId("flow-kind-chip")).toBeNull();
  });
});
