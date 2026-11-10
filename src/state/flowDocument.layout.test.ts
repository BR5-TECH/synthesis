/**
 * Position, size, membership, and the tracker connection (FLO-FR-21,
 * FLO-FR-42, FLO-FR-44, FLO-FR-OAOZ, FLO-FR-AKMV, FLO-FR-TYEZ, FLO-FR-ULEM).
 */
import { describe, expect, it } from "vitest";

import {
  CARD_H,
  CARD_W,
  connectSync,
  findItem,
  minContainerSize,
  MIN_CONTAINER_SIZE,
  moveItem,
  resizeItem,
  setSyncLink,
  syncLinkOf,
  type FlowDocument,
  type FlowEditResult,
} from "./flowDocument";
import {
  blocks,
  issueBase,
  issueRef,
  item,
  milestoneRef,
  relates,
  samplePlan,
  withItems,
} from "../test/flowPlanFixtures";

function ok(r: FlowEditResult): FlowDocument {
  if (!r.ok) throw new Error(`expected the edit to be accepted, got ${r.refusal.code}`);
  return r.doc;
}

function refusal(r: FlowEditResult) {
  if (r.ok) throw new Error("expected the edit to be refused");
  return r.refusal;
}

describe("moveItem (FLO-FR-21, FLO-FR-42, FLO-FR-OAOZ)", () => {
  it("FLO-FR-21: a move in the same container stores the new position in that container's coordinates", () => {
    const doc = ok(moveItem(samplePlan(), "t1", { x: 100, y: 200 }, "f1"));
    expect(findItem(doc, "t1")).toMatchObject({ parentId: "f1", position: { x: 60, y: 80 } });
  });

  it("FLO-FR-42: a drop into another container changes the membership and rebases the position", () => {
    const doc = ok(moveItem(samplePlan(), "t1", { x: 600, y: 250 }, "f2"));
    expect(findItem(doc, "t1")).toMatchObject({ parentId: "f2", position: { x: 100, y: 70 } });
  });

  it("FLO-FR-21: moving a container moves what it holds and changes one position", () => {
    const before = samplePlan();
    const doc = ok(moveItem(before, "f1", { x: 140, y: 120 }, "m1"));
    expect(findItem(doc, "f1")?.position).toEqual({ x: 120, y: 60 });
    expect(findItem(doc, "t1")).toBe(findItem(before, "t1"));
  });

  it("FLO-FR-42: a drop of a milestone on open canvas keeps it at the top level", () => {
    const doc = ok(moveItem(samplePlan(), "m1", { x: 5, y: 6 }));
    expect(findItem(doc, "m1")).toMatchObject({ position: { x: 5, y: 6 } });
    expect("parentId" in findItem(doc, "m1")!).toBe(false);
  });

  it("FLO-FR-42: a drop changes no edge", () => {
    const edges = [blocks("e1", "t1", "t3"), relates("e2", "x1", "t1")];
    const doc = ok(moveItem(samplePlan(edges), "t1", { x: 600, y: 250 }, "f2"));
    expect(doc.edges).toEqual(edges);
  });

  it("FLO-FR-OAOZ: a drop into a container the kind does not permit is refused and names the permitted containers", () => {
    expect(refusal(moveItem(samplePlan(), "t1", { x: 0, y: 0 }))).toEqual({
      code: "invalid_parent_kind",
      permitted: ["feature"],
    });
    expect(refusal(moveItem(samplePlan(), "t1", { x: 0, y: 0 }, "m1"))).toEqual({
      code: "invalid_parent_kind",
      permitted: ["feature"],
    });
    expect(refusal(moveItem(samplePlan(), "f2", { x: 0, y: 0 }))).toEqual({
      code: "invalid_parent_kind",
      permitted: ["milestone", "epic"],
    });
    expect(refusal(moveItem(samplePlan(), "x1", { x: 0, y: 0 }, "t1")).code).toBe(
      "invalid_parent_kind",
    );
  });

  it("FLO-FR-OAOZ: a defect may join a feature, an epic, or a milestone", () => {
    for (const parent of ["f1", "ep1", "m1"]) {
      expect(moveItem(samplePlan(), "x1", { x: 600, y: 300 }, parent).ok, parent).toBe(true);
    }
  });

  it("FGV-FR-13: a container cannot go into itself or into an item under it", () => {
    expect(refusal(moveItem(samplePlan(), "ep1", { x: 0, y: 0 }, "ep1")).code).toBe(
      "into_own_subtree",
    );
    expect(refusal(moveItem(samplePlan(), "m1", { x: 0, y: 0 }, "f1")).code).toBe(
      "into_own_subtree",
    );
  });

  it("FLO-FR-OAOZ, FGV-FR-KBLT: a drop that makes an edge join an item to its ancestor is refused", () => {
    // x1 blocks f2; with x1 in f2, the edge joins x1 to its own container.
    const doc = samplePlan([blocks("e1", "x1", "f2"), blocks("e2", "t1", "t2")]);
    expect(refusal(moveItem(doc, "x1", { x: 520, y: 200 }, "f2"))).toEqual({
      code: "edge_within_lineage",
      edgeIds: ["e1"],
    });
  });

  it("FLO-FR-OAOZ, FGV-FR-KBLT: a container dropped into another refuses an edge to its new ancestor", () => {
    // t1 relates to ep1; with f1 in ep1, t1 is under ep1 too.
    const doc = samplePlan([relates("e1", "t1", "ep1")]);
    expect(refusal(moveItem(doc, "f1", { x: 500, y: 300 }, "ep1"))).toEqual({
      code: "edge_within_lineage",
      edgeIds: ["e1"],
    });
  });

  it("FGV-FR-PMTY: a drop that gives the plan a dependency cycle is refused and names its items", () => {
    // t3 blocks f1, and x1 blocks t3. With x1 in f1, x1 waits for t3, which waits for x1.
    const doc = samplePlan([blocks("e1", "t3", "f1"), blocks("e2", "x1", "t3")]);
    expect(refusal(moveItem(doc, "x1", { x: 60, y: 300 }, "f1"))).toEqual({
      code: "dependency_cycle",
      itemIds: ["f1", "t3", "x1"],
      edgeIds: ["e1", "e2"],
    });
  });

  it("FGV-FR-PMTY: a container dropped into a blocked container that it blocks is refused", () => {
    // x1 blocks ep1 and t1 blocks x1. With f1 in ep1, t1 waits for x1, which waits for t1.
    const doc = samplePlan([blocks("e1", "x1", "ep1"), blocks("e2", "t1", "x1")]);
    expect(refusal(moveItem(doc, "f1", { x: 500, y: 300 }, "ep1"))).toEqual({
      code: "dependency_cycle",
      itemIds: ["t1", "ep1", "x1"],
      edgeIds: ["e1", "e2"],
    });
  });

    it("FLO-FR-26: a drop at the same place in the same container changes nothing", () => {
    const doc = samplePlan();
    expect(ok(moveItem(doc, "t1", { x: 60, y: 180 }, "f1"))).toBe(doc);
  });

  it("FLO-FR-42: an unknown item or container is refused", () => {
    expect(refusal(moveItem(samplePlan(), "nope", { x: 0, y: 0 })).code).toBe("unknown_item");
    expect(refusal(moveItem(samplePlan(), "t1", { x: 0, y: 0 }, "nope")).code).toBe(
      "unknown_parent",
    );
  });
});

describe("minContainerSize and resizeItem (FLO-FR-44)", () => {
  it("FLO-FR-44: a container's floor is the bounds of its members plus the padding", () => {
    // t2 at (220,60) is the right and bottom member: 220+170+16, 60+96+16.
    expect(minContainerSize(samplePlan(), "f1")).toEqual({
      width: 220 + CARD_W + 16,
      height: 60 + CARD_H + 16,
    });
    // A member container counts with its own size: ep1 at (460,60) 500×400.
    expect(minContainerSize(samplePlan(), "m1")).toEqual({ width: 976, height: 512 });
  });

  it("FLO-FR-44: a member at a negative position does not lower the floor", () => {
    const doc = withItems(
      samplePlan(),
      item("f9", "feature", { parentId: "m1" }),
      item("t9", "task", { parentId: "f9", position: { x: -40, y: -60 } }),
    );
    expect(minContainerSize(doc, "f9")).toEqual(MIN_CONTAINER_SIZE);
  });

    it("FLO-FR-44: an empty container's floor is the minimum container size", () => {
    const doc = withItems(samplePlan(), item("f9", "feature", { parentId: "m1" }));
    expect(minContainerSize(doc, "f9")).toEqual(MIN_CONTAINER_SIZE);
    expect(minContainerSize(doc, "t1")).toEqual({ width: CARD_W, height: CARD_H });
  });

  it("FLO-FR-44: a resize below the members' bounds stops at them", () => {
    const doc = resizeItem(samplePlan(), "f1", { width: 100, height: 100 });
    expect(findItem(doc, "f1")?.size).toEqual(minContainerSize(samplePlan(), "f1"));
  });

  it("FLO-FR-44: a resize above the floor is rounded to whole pixels", () => {
    const doc = resizeItem(samplePlan(), "f1", { width: 500.4, height: 300.6 });
    expect(findItem(doc, "f1")?.size).toEqual({ width: 500, height: 301 });
  });

  it("FLO-FR-44: a resize to the same size, of a leaf, or of an unknown item changes nothing", () => {
    const doc = samplePlan();
    expect(resizeItem(doc, "f1", { width: 420, height: 300 })).toBe(doc);
    expect(resizeItem(doc, "t1", { width: 999, height: 999 })).toBe(doc);
    expect(resizeItem(doc, "nope", { width: 999, height: 999 })).toBe(doc);
  });
});

describe("the tracker connection (FLO-FR-AKMV, FLO-FR-TYEZ, FLO-FR-ULEM)", () => {
  it("FLO-FR-AKMV: connecting adds a GitHub sync section with no links, once", () => {
    const doc = connectSync(samplePlan());
    expect(doc.sync).toEqual({ tracker: "github", links: [] });
    expect(connectSync(doc)).toBe(doc);
  });

  it("FLO-FR-TYEZ: a link takes the tracker's name and description and stores the base", () => {
    const doc = connectSync(samplePlan());
    const base = issueBase("Card capture", "Capture the card.");
    const next = ok(setSyncLink(doc, "f1", issueRef(42), base));
    expect(findItem(next, "f1")).toMatchObject({
      name: "Card capture",
      description: "Capture the card.",
    });
    expect(next.sync?.links).toEqual([{ itemId: "f1", ref: issueRef(42), base }]);
    expect(syncLinkOf(next, "f1")?.ref.number).toBe(42);
    expect(syncLinkOf(next, "f2")).toBeUndefined();
  });

  it("FLO-FR-TYEZ: an empty tracker description leaves the item with none", () => {
    const doc = connectSync(samplePlan());
    const described = { ...doc, items: doc.items.map((i) => (i.id === "t1" ? { ...i, description: "old" } : i)) };
    const next = ok(setSyncLink(described, "t1", issueRef(7), issueBase("Tokenize")));
    expect("description" in findItem(next, "t1")!).toBe(false);
  });

  it("FLO-FR-TYEZ, FGV-FR-LWPD: a milestone links a tracker milestone and nothing else", () => {
    const doc = connectSync(samplePlan());
    const base = { name: "Beta", description: "", state: "open" as const, due: null };
    expect(ok(setSyncLink(doc, "m1", milestoneRef(3), base)).sync?.links).toHaveLength(1);
    expect(refusal(setSyncLink(doc, "m1", issueRef(3), issueBase("Beta"))).code).toBe(
      "tracker_kind_mismatch",
    );
    expect(refusal(setSyncLink(doc, "f1", milestoneRef(3), base)).code).toBe(
      "tracker_kind_mismatch",
    );
  });

  it("FGV-FR-LWPD: a reference that another item links is refused and names that item", () => {
    const doc = ok(setSyncLink(connectSync(samplePlan()), "f1", issueRef(42), issueBase("A")));
    expect(refusal(setSyncLink(doc, "f2", issueRef(42), issueBase("A")))).toEqual({
      code: "duplicate_tracker_link",
      itemIds: ["f1"],
    });
    // The same number in another repository is another reference.
    expect(setSyncLink(doc, "f2", issueRef(42, "billing"), issueBase("B")).ok).toBe(true);
  });

  it("FLO-FR-TYEZ, FGV-FR-TLMV: a link to an ignored tracker item takes it off the ignored list", () => {
    const doc = { ...connectSync(samplePlan()) };
    doc.sync = { tracker: "github", links: [], ignored: [issueRef(47), issueRef(48)] };
    const next = ok(setSyncLink(doc, "f1", issueRef(47), issueBase("A")));
    expect(next.sync?.ignored).toEqual([issueRef(48)]);
    const last = ok(setSyncLink(next, "f2", issueRef(48), issueBase("B")));
    // An empty list is absent (FGV-FR-01).
    expect(last.sync && "ignored" in last.sync).toBe(false);
  });

  it("FLO-FR-TYEZ: an item with a link, a Flow with no sync section, or an unknown item is refused", () => {
    const doc = ok(setSyncLink(connectSync(samplePlan()), "f1", issueRef(42), issueBase("A")));
    expect(refusal(setSyncLink(doc, "f1", issueRef(43), issueBase("B"))).code).toBe(
      "already_linked",
    );
    expect(refusal(setSyncLink(samplePlan(), "f1", issueRef(42), issueBase("A"))).code).toBe(
      "not_synced",
    );
    expect(refusal(setSyncLink(doc, "nope", issueRef(44), issueBase("C"))).code).toBe(
      "unknown_item",
    );
  });
});
