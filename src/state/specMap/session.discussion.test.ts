/**
 * What the map session keeps of the Discussion column (SMP-FR-WNLR,
 * SMP-FR-AUFA, SMP-FR-SWCL, SMP-FR-IHFM, SMP-FR-EIUP, SMP-FR-DUUU,
 * SMP-FR-QHBV).
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import { SpecMapSessionStore } from "./session";
import { columnFraction, mapThreadKey, sameMapNode } from "./discussion";
import { columnWidth } from "../../components/MapDiscussion/Pane";
import { deliveryNodeOf, issueItemId, specNodeOf } from "./nodeRefs";
import type { SpecMapOps } from "../../api/specMap";
import { smallIndex } from "../../test/specMapFixtures";
import { ITEM_NODE, SPEC_NODE } from "../../test/mapPlanFixtures";
import type { DeliverySource } from "../../types/delivery";

vi.mock("../../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
}));

const NONE: DeliverySource = { kind: "none", reason: "no_source" };

function makeStore() {
  const ops = {
    load: vi.fn<SpecMapOps["load"]>(async () => smallIndex()),
    getDeliverySource: vi.fn<SpecMapOps["getDeliverySource"]>(async () => NONE),
  };
  return new SpecMapSessionStore(ops);
}

let store: SpecMapSessionStore;

beforeEach(() => {
  store = makeStore();
});

describe("the state of the column (SMP-FR-WNLR, SMP-FR-AUFA, SMP-FR-SWCL)", () => {
  it("SMP-FR-WNLR, SMP-FR-WYZA: the column is shown on the first open of a map session, on the first tab", () => {
    const { discussion } = store.snapshot();
    expect(discussion).toMatchObject({ visible: true, threadId: null, composing: false, node: null });
  });

  it("SMP-FR-WNLR: the toggle shows and hides the column", () => {
    store.setDiscussionVisible(false);
    expect(store.snapshot().discussion.visible).toBe(false);
    store.setDiscussionVisible(true);
    expect(store.snapshot().discussion.visible).toBe(true);
  });

  it("SMP-FR-SWCL: the split opens at canvas 70, and the accelerator cycles the three presets", () => {
    expect(store.snapshot().discussion.ratio).toBe("doc70");
    store.cycleDiscussionRatio();
    expect(store.snapshot().discussion.ratio).toBe("even");
    store.cycleDiscussionRatio();
    expect(store.snapshot().discussion.ratio).toBe("chat70");
    store.cycleDiscussionRatio();
    expect(store.snapshot().discussion.ratio).toBe("doc70");
    // A dragged split enters the cycle at its head.
    store.setDiscussionRatio(0.42);
    store.cycleDiscussionRatio();
    expect(store.snapshot().discussion.ratio).toBe("doc70");
  });

  it("SMP-FR-SWCL: a split that does not change notifies no one", () => {
    const version = store.getVersion();
    store.setDiscussionRatio("doc70");
    expect(store.getVersion()).toBe(version);
  });

  it("SMP-FR-SWCL: the column takes the share of the body that the canvas side leaves", () => {
    expect(columnWidth(columnFraction("doc70"))).toBe("30%");
    expect(columnWidth(columnFraction("even"))).toBe("50%");
    expect(columnWidth(columnFraction("chat70"))).toBe("70%");
    expect(columnWidth(columnFraction(2 / 3))).toBe("33.3%");
    // The drag limits of DDS-FR-PNXR hold.
    expect(columnWidth(columnFraction(0.1))).toBe("75%");
    expect(columnWidth(columnFraction(0.9))).toBe("25%");
  });

  it("SMP-FR-AUFA: the session holds the visibility, the split, the active thread, and the disclosure, and closing the Map tab keeps them", () => {
    store.setDiscussionVisible(false);
    store.setDiscussionRatio("chat70");
    store.selectDiscussionThread("t2");
    store.setDiscussionShowResolved(true);
    store.resetView();
    expect(store.snapshot().discussion).toMatchObject({
      visible: false,
      ratio: "chat70",
      threadId: "t2",
      showResolved: true,
    });
  });

  it("SMP-FR-AUFA: a project switch or a change of the active worktree discards them with the session", () => {
    store.setDiscussionVisible(false);
    store.setDiscussionRatio("even");
    store.selectDiscussionThread("t2");
    store.setDiscussionShowResolved(true);
    store.discuss(SPEC_NODE);
    store.setDeliveryNode(ITEM_NODE);
    store.clear();
    const { discussion, deliveryNode } = store.snapshot();
    expect(discussion).toMatchObject({
      visible: true,
      ratio: "doc70",
      threadId: null,
      composing: false,
      node: null,
      showResolved: false,
    });
    expect(deliveryNode).toBeNull();
  });

  it("SMP-FR-AUFA: a write that changes nothing notifies no one", () => {
    store.selectDiscussionThread("t1");
    store.setDiscussionShowResolved(true);
    const version = store.getVersion();
    store.selectDiscussionThread("t1");
    store.setDiscussionShowResolved(true);
    store.cancelDiscussionComposer();
    expect(store.getVersion()).toBe(version);
  });
});

describe("the thread the column reads (SMP-FR-EIUP, SMP-FR-IHFM, SMP-FR-DUUU, SMP-FR-QHBV)", () => {
  it("SMP-FR-EIUP, SMP-FR-KHKC: choosing a tab reads its thread and leaves any opening composer", () => {
    store.discuss(ITEM_NODE);
    store.selectDiscussionThread("t3");
    expect(store.snapshot().discussion).toMatchObject({ threadId: "t3", composing: false, node: null });
    store.composeDiscussion(null);
    expect(store.snapshot().discussion).toMatchObject({ threadId: "t3", composing: true, node: null });
    store.cancelDiscussionComposer();
    expect(store.snapshot().discussion.composing).toBe(false);
  });

  it("SMP-FR-EIUP: two references name the same node by lens, kind, and id, and the label takes no part", () => {
    expect(sameMapNode(SPEC_NODE, { ...SPEC_NODE, label: "other" })).toBe(true);
    expect(sameMapNode(SPEC_NODE, { ...SPEC_NODE, kind: "index" })).toBe(false);
    expect(sameMapNode(null, null)).toBe(true);
    expect(sameMapNode(SPEC_NODE, null)).toBe(false);
    expect(mapThreadKey(null)).toBe("map-thread");
    expect(mapThreadKey(SPEC_NODE)).not.toBe(mapThreadKey(ITEM_NODE));
  });

  it("SMP-FR-EIUP: selecting the node of a thread selects it on the canvas of its lens", async () => {
    await store.load();
    store.showMapNode({ lens: "specifications", kind: "spec", id: "A1", label: "a1 label" });
    expect(store.snapshot().lens).toBe("specifications");
    expect(store.snapshot().selection).toEqual({ kind: "spec", code: "A1" });
    store.showMapNode({ lens: "delivery", kind: "item", id: "issue-47", label: "Decline" });
    expect(store.snapshot().lens).toBe("delivery");
    expect(store.snapshot().deliveryReveal).toMatchObject({ itemId: "issue-47" });
  });

  it("SMP-FR-EIUP: a node that the index no longer holds changes nothing", async () => {
    await store.load();
    const before = store.snapshot().selection;
    store.showMapNode({ lens: "specifications", kind: "spec", id: "GONE", label: "gone" });
    expect(store.snapshot().selection).toEqual(before);
  });

  it("SMP-FR-IHFM: Discuss shows the column on the composer of the node, and raises the focus and find signals each time", () => {
    store.setDiscussionVisible(false);
    store.discuss(SPEC_NODE);
    const first = store.snapshot().discussion;
    expect(first).toMatchObject({ visible: true, composing: true, node: SPEC_NODE });
    store.discuss(SPEC_NODE);
    expect(store.snapshot().discussion.focusSignal).toBe(first.focusSignal + 1);
    expect(store.snapshot().discussion.discussSignal).toBe(first.discussSignal + 1);
  });

  it("SMP-FR-DUUU: the node selected on the Delivery canvas is held for Use selected node", () => {
    store.setDeliveryNode(ITEM_NODE);
    expect(store.snapshot().deliveryNode).toEqual(ITEM_NODE);
    store.setDeliveryNode(null);
    expect(store.snapshot().deliveryNode).toBeNull();
  });

  it("SMP-FR-DUUU: the references of both lenses carry lens, kind, id, and label", async () => {
    await store.load();
    expect(deliveryNodeOf({ id: "m1", kind: "milestone", name: "GA" })).toEqual({
      lens: "delivery",
      kind: "milestone",
      id: "m1",
      label: "GA",
    });
    expect(deliveryNodeOf({ id: "i1", kind: "task", name: "" })).toMatchObject({ kind: "item", label: "i1" });
    const tree = store.tree();
    expect(tree).not.toBeNull();
    expect(specNodeOf(tree!, { kind: "spec", code: "A1" })).toMatchObject({ lens: "specifications", kind: "spec", id: "A1" });
    expect(specNodeOf(tree!, { kind: "index", id: "d1" })).toMatchObject({ kind: "index", id: "d1" });
    expect(specNodeOf(tree!, null)).toBeNull();
    expect(issueItemId(47)).toBe("issue-47");
  });

  it("SMP-FR-QHBV: revealing a Map thread shows the column and selects the thread", () => {
    store.setDiscussionVisible(false);
    store.discuss(ITEM_NODE);
    store.revealThread("t4", false);
    expect(store.snapshot().discussion).toMatchObject({
      visible: true,
      threadId: "t4",
      composing: false,
      node: null,
      showResolved: false,
    });
  });

  it("SMP-FR-QHBV: a resolved thread is revealed with the disclosure expanded, and an open one keeps the disclosure as it was", () => {
    store.revealThread("t4", true);
    expect(store.snapshot().discussion.showResolved).toBe(true);
    store.revealThread("t5", false);
    expect(store.snapshot().discussion.showResolved).toBe(true);
  });
});
