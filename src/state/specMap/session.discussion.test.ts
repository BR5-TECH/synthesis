/**
 * What the map session keeps of the Discussion column (SMP-FR-WNLR,
 * SMP-FR-AUFA, SMP-FR-SWCL, SMP-FR-IHFM, SMP-FR-EIUP, SMP-FR-DUUU,
 * SMP-FR-QHBV).
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import { SpecMapSessionStore } from "./session";
import {
  DISCUSSION_WIDTH_DEFAULT,
  DISCUSSION_WIDTH_MAX,
  DISCUSSION_WIDTH_MIN,
  clampDiscussionWidth,
  mapThreadKey,
  sameMapNode,
} from "./discussion";
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
  it("SMP-FR-WNLR: the column is shown on the first open of a map session, on the Map thread", () => {
    const { discussion } = store.snapshot();
    expect(discussion.visible).toBe(true);
    expect(discussion.node).toBeNull();
  });

  it("SMP-FR-WNLR: the toggle shows and hides the column", () => {
    store.setDiscussionVisible(false);
    expect(store.snapshot().discussion.visible).toBe(false);
    store.setDiscussionVisible(true);
    expect(store.snapshot().discussion.visible).toBe(true);
  });

  it("SMP-FR-SWCL: the column is 360px wide by default and is held between 280px and 560px", () => {
    expect(store.snapshot().discussion.width).toBe(DISCUSSION_WIDTH_DEFAULT);
    expect(DISCUSSION_WIDTH_DEFAULT).toBe(360);
    store.setDiscussionWidth(100);
    expect(store.snapshot().discussion.width).toBe(280);
    store.setDiscussionWidth(900);
    expect(store.snapshot().discussion.width).toBe(560);
    store.setDiscussionWidth(420.4);
    expect(store.snapshot().discussion.width).toBe(420);
    expect(clampDiscussionWidth(DISCUSSION_WIDTH_MIN - 1)).toBe(DISCUSSION_WIDTH_MIN);
    expect(clampDiscussionWidth(DISCUSSION_WIDTH_MAX + 1)).toBe(DISCUSSION_WIDTH_MAX);
  });

  it("SMP-FR-AUFA: the session holds the visibility, the width, and the active thread, and closing the Map tab keeps them", () => {
    store.setDiscussionVisible(false);
    store.setDiscussionWidth(500);
    store.setDiscussionNode(SPEC_NODE);
    store.resetView();
    expect(store.snapshot().discussion).toMatchObject({ visible: false, width: 500, node: SPEC_NODE });
  });

  it("SMP-FR-AUFA: a project switch or a change of the active worktree discards them with the session", () => {
    store.setDiscussionVisible(false);
    store.setDiscussionWidth(500);
    store.setDiscussionNode(SPEC_NODE);
    store.setDeliveryNode(ITEM_NODE);
    store.clear();
    const { discussion, deliveryNode } = store.snapshot();
    expect(discussion).toMatchObject({ visible: true, width: 360, node: null });
    expect(deliveryNode).toBeNull();
  });
});

describe("the thread the column reads (SMP-FR-EIUP, SMP-FR-IHFM, SMP-FR-DUUU, SMP-FR-QHBV)", () => {
  it("SMP-FR-EIUP: choosing a thread reads it, and null is the Map thread", () => {
    store.setDiscussionNode(ITEM_NODE);
    expect(store.snapshot().discussion.node).toEqual(ITEM_NODE);
    store.setDiscussionNode(null);
    expect(store.snapshot().discussion.node).toBeNull();
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

  it("SMP-FR-IHFM: Discuss shows the column, reads the thread of the node, and raises the focus signal each time", () => {
    store.setDiscussionVisible(false);
    store.discuss(SPEC_NODE);
    const first = store.snapshot().discussion;
    expect(first).toMatchObject({ visible: true, node: SPEC_NODE });
    store.discuss(SPEC_NODE);
    expect(store.snapshot().discussion.focusSignal).toBe(first.focusSignal + 1);
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

  it("SMP-FR-QHBV: revealing a Map thread shows the column and selects the thread of the node, or the Map thread for null", () => {
    store.setDiscussionVisible(false);
    store.revealThread(ITEM_NODE);
    expect(store.snapshot().discussion).toMatchObject({ visible: true, node: ITEM_NODE });
    store.revealThread(null);
    expect(store.snapshot().discussion.node).toBeNull();
  });
});
