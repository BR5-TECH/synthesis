/**
 * Edges and the dependency cycle (FLO-FR-15 to FLO-FR-17, FLO-FR-CEVI,
 * FLO-FR-GZCH, FLO-FR-ZEPI, FLO-FR-20, FGV-FR-PMTY). The local cycle check
 * must agree with the backend's `WaitGraph`
 * (`src-tauri/src/flow_validation/model.rs`) on every plan.
 */
import { describe, expect, it } from "vitest";

import {
  connect,
  connectRefusal,
  cycleClosedBy,
  dependencyCycleEdges,
  edgeBetween,
  removeEdge,
  reverseEdge,
  setEdgeType,
  waitsFor,
  type FlowDocument,
  type FlowEditResult,
} from "./flowDocument";
import { blocks, item, relates, samplePlan, withItems } from "../test/flowPlanFixtures";

function ok(r: FlowEditResult): FlowDocument {
  if (!r.ok) throw new Error(`expected the edit to be accepted, got ${r.refusal.code}`);
  return r.doc;
}

function refusal(r: FlowEditResult) {
  if (r.ok) throw new Error("expected the edit to be refused");
  return r.refusal;
}

describe("connect (FLO-FR-15 to FLO-FR-17, FLO-FR-CEVI, FLO-FR-ZEPI)", () => {
  it("FLO-FR-CEVI: a connection creates a blocks edge from the item the drag started on", () => {
    const before = samplePlan();
    const r = connect(before, "t1", "t2");
    if (!r.ok) throw new Error(r.refusal.code);
    expect(r.edgeId).toBe("e1");
    expect(r.doc.edges).toEqual([{ id: "e1", from: "t1", to: "t2", type: "blocks" }]);
    expect(before.edges).toEqual([]);
    expect(connectRefusal(before, "t1", "t2")).toBeNull();
  });

  it("FLO-FR-15: an edge joins items in different containers, and items with containers", () => {
    expect(ok(connect(samplePlan(), "t1", "t3")).edges).toHaveLength(1);
    expect(ok(connect(samplePlan(), "f1", "f2")).edges).toHaveLength(1);
    expect(ok(connect(samplePlan(), "x1", "ep1")).edges).toHaveLength(1);
  });

  it("FLO-FR-16: a connection from an item to itself is refused", () => {
    const doc = samplePlan();
    expect(refusal(connect(doc, "t1", "t1"))).toEqual({ code: "self" });
  });

  it("FLO-FR-17: a second edge between two items is refused in either direction and type", () => {
    const doc = samplePlan([blocks("e1", "t1", "t2"), relates("e2", "t3", "x1")]);
    expect(refusal(connect(doc, "t1", "t2"))).toEqual({ code: "duplicate_edge", edgeIds: ["e1"] });
    expect(refusal(connect(doc, "t2", "t1"))).toEqual({ code: "duplicate_edge", edgeIds: ["e1"] });
    expect(refusal(connect(doc, "x1", "t3"))).toEqual({ code: "duplicate_edge", edgeIds: ["e2"] });
    expect(edgeBetween(doc, "x1", "t3")?.id).toBe("e2");
    expect(edgeBetween(doc, "t1", "t3")).toBeUndefined();
  });

  it("FLO-FR-ZEPI: a connection between an item and its ancestor or descendant is refused", () => {
    const doc = samplePlan();
    expect(refusal(connect(doc, "t1", "f1")).code).toBe("edge_within_lineage");
    expect(refusal(connect(doc, "m1", "t3")).code).toBe("edge_within_lineage");
    expect(refusal(connect(doc, "ep1", "f2")).code).toBe("edge_within_lineage");
  });

  it("FLO-FR-15: a connection to an item that is not there is refused", () => {
    expect(refusal(connect(samplePlan(), "t1", "nope")).code).toBe("unknown_item");
    expect(refusal(connect(samplePlan(), "nope", "t1")).code).toBe("unknown_item");
  });

  it("FLO-FR-CEVI: a new edge id never repeats one in use", () => {
    const doc = samplePlan([blocks("e2", "t1", "t2")]);
    const r = connect(doc, "t3", "x1");
    if (!r.ok) throw new Error(r.refusal.code);
    expect(r.edgeId).toBe("e3");
  });
});

describe("the dependency cycle (FLO-FR-GZCH, FGV-FR-PMTY)", () => {
  it("FLO-FR-GZCH: a connection that closes a cycle of leaves is refused and names the items", () => {
    // Two items carry one edge at most (FLO-FR-17), so the shortest cycle a
    // connection can close runs through three items.
    const doc = samplePlan([blocks("e1", "t1", "t2"), blocks("e2", "t2", "x1")]);
    expect(refusal(connect(doc, "x1", "t1"))).toEqual({
      code: "dependency_cycle",
      itemIds: ["x1", "t2", "t1"],
    });
  });

  it("FLO-FR-GZCH: a longer cycle names every item it runs through", () => {
    const doc = samplePlan([blocks("e1", "t1", "t2"), blocks("e2", "t2", "t3")]);
    const r = refusal(connect(doc, "t3", "t1"));
    expect(r.code).toBe("dependency_cycle");
    expect([...r.itemIds!].sort()).toEqual(["t1", "t2", "t3"]);
  });

  it("FGV-FR-PMTY: a blocks edge from a container makes each of its leaves a blocker", () => {
    // f1 blocks t3: t3 waits for t1 and t2. Then t3 blocks t1 is a cycle.
    const doc = samplePlan([blocks("e1", "f1", "t3")]);
    expect(refusal(connect(doc, "t3", "t1"))).toEqual({
      code: "dependency_cycle",
      itemIds: ["t3", "f1", "t1"],
    });
  });

  it("FGV-FR-PMTY, FLO-FR-39: a blocks edge into a container blocks everything in it", () => {
    // t3 blocks f1: t1 and t2 wait for t3. Then t1 blocks t3 is a cycle.
    const doc = samplePlan([blocks("e1", "t3", "f1")]);
    expect(refusal(connect(doc, "t1", "t3"))).toEqual({
      code: "dependency_cycle",
      itemIds: ["t1", "f1", "t3"],
    });
    expect(waitsFor(doc, "t1", "t3")).toBe(true);
    expect(waitsFor(doc, "t3", "t1")).toBe(false);
  });

  it("FGV-FR-PMTY: a chain that leaves a container and comes back into it is no cycle", () => {
    // t2 waits for t3, which waits for t1. A model that reads f1 as one node
    // would see f1 -> t3 -> f1 and refuse it.
    const doc = samplePlan([blocks("e1", "t1", "t3")]);
    const next = ok(connect(doc, "t3", "t2"));
    expect(dependencyCycleEdges(next)).toEqual([]);
    expect(waitsFor(next, "t2", "t1")).toBe(true);
  });

  it("FGV-FR-PMTY: an empty container counts as a leaf, so a ring of empty containers is a cycle", () => {
    const doc = withItems(
      samplePlan([blocks("e1", "fa", "fb"), blocks("e2", "fb", "fc")]),
      item("fa", "feature", { parentId: "m1" }),
      item("fb", "feature", { parentId: "m1" }),
      item("fc", "feature", { parentId: "m1" }),
    );
    expect(refusal(connect(doc, "fc", "fa"))).toEqual({
      code: "dependency_cycle",
      itemIds: ["fc", "fb", "fa"],
    });
  });

  it("FGV-FR-PMTY: a ring through an empty container is a cycle", () => {
    const doc = withItems(
      samplePlan([blocks("e1", "t1", "fe"), blocks("e2", "fe", "x1")]),
      item("fe", "feature", { parentId: "m1" }),
    );
    expect(refusal(connect(doc, "x1", "t1")).code).toBe("dependency_cycle");
  });

  it("FGV-FR-PMTY: containers that hold leaves form a cycle through what they block", () => {
    // t3 waits for f1's leaves, x1 waits for t3, and t1 would wait for x1.
    const doc = samplePlan([blocks("e1", "f1", "f2"), blocks("e2", "f2", "x1")]);
    expect(refusal(connect(doc, "x1", "t1")).code).toBe("dependency_cycle");
  });

  it("FLO-FR-GZCH: a relates edge never forms a cycle", () => {
    const doc = samplePlan([blocks("e1", "t1", "t2"), blocks("e2", "t2", "x1"), relates("e3", "x1", "t1")]);
    expect(dependencyCycleEdges(doc)).toEqual([]);
  });

  it("FGV-FR-PMTY: dependencyCycleEdges names every blocks edge on a cycle, and no other", () => {
    // e0 leads into the cycle and is not on it; e4 is a relates edge.
    const doc = samplePlan([
      blocks("e0", "t3", "t1"),
      blocks("e1", "t1", "t2"),
      blocks("e2", "t2", "x1"),
      blocks("e3", "x1", "t1"),
      relates("e4", "f2", "f1"),
    ]);
    expect(dependencyCycleEdges(doc)).toEqual(["e1", "e2", "e3"]);
  });

  it("FGV-FR-PMTY: two containers whose leaves block each other give a cycle on both edges", () => {
    const doc = samplePlan([blocks("e1", "f1", "t3"), blocks("e2", "f2", "t1")]);
    expect(dependencyCycleEdges(doc)).toEqual(["e1", "e2"]);
  });

  it("FGV-FR-PMTY, FGV-FR-11, FGV-FR-KBLT: like the backend, a duplicate pair or a lineage edge is not read as a cycle", () => {
    // The backend reports these as duplicate_edge and edge_within_lineage, and
    // gives neither edge to the wait graph.
    expect(dependencyCycleEdges(samplePlan([blocks("e1", "t1", "t2"), blocks("e2", "t2", "t1")]))).toEqual([]);
    expect(dependencyCycleEdges(samplePlan([blocks("e1", "f1", "t1")]))).toEqual([]);
    expect(dependencyCycleEdges(samplePlan([blocks("e1", "t1", "nope")]))).toEqual([]);
  });

  it("FGV-FR-PMTY: a wait runs through every container above a leaf", () => {
    // x1 blocks ep1, so t3 (in f2, in ep1) waits for x1.
    const into = samplePlan([blocks("e1", "x1", "ep1")]);
    expect(waitsFor(into, "t3", "x1")).toBe(true);
    expect(refusal(connect(into, "t3", "x1"))).toEqual({
      code: "dependency_cycle",
      itemIds: ["t3", "ep1", "x1"],
    });
    // ep1 blocks x1, so x1 waits for t3 through two levels of completion.
    const from = samplePlan([blocks("e1", "ep1", "x1")]);
    expect(refusal(connect(from, "x1", "t3"))).toEqual({
      code: "dependency_cycle",
      itemIds: ["x1", "ep1", "t3"],
    });
  });

  it("FOA-FR-XOAR: an item does not wait for itself, and an unknown item waits for nothing", () => {
    const doc = samplePlan([blocks("e1", "t1", "t2")]);
    expect(waitsFor(doc, "t1", "t1")).toBe(false);
    expect(waitsFor(doc, "nope", "t1")).toBe(false);
    expect(waitsFor(doc, "t2", "nope")).toBe(false);
  });

  it("FGV-FR-PMTY: dependencyCycleEdges sees a cycle that runs through a container", () => {
    const doc = samplePlan([blocks("e1", "t3", "f1"), blocks("e2", "t2", "t3")]);
    expect(dependencyCycleEdges(doc)).toEqual(["e1", "e2"]);
  });

  it("FGV-FR-PMTY: cycleClosedBy gives null for an edge that closes no cycle, and for unknown items", () => {
    const doc = samplePlan([blocks("e1", "t1", "t2")]);
    expect(cycleClosedBy(doc, "t1", "t3")).toBeNull();
    expect(cycleClosedBy(doc, "t1", "nope")).toBeNull();
    // A self edge is the shortest cycle; connect refuses it as `self` first.
    expect(cycleClosedBy(doc, "t1", "t1")).toEqual(["t1"]);
  });
});

describe("setEdgeType and reverseEdge (FLO-FR-CEVI, FLO-FR-GZCH)", () => {
  it("FLO-FR-CEVI: a blocks edge becomes a relates edge, and back", () => {
    const doc = samplePlan([blocks("e1", "t1", "t2")]);
    const relating = ok(setEdgeType(doc, "e1", "relates"));
    expect(relating.edges[0]).toEqual({ id: "e1", from: "t1", to: "t2", type: "relates" });
    expect(ok(setEdgeType(relating, "e1", "blocks")).edges[0].type).toBe("blocks");
    expect(ok(setEdgeType(doc, "e1", "blocks"))).toBe(doc);
  });

  it("FLO-FR-CEVI, FLO-FR-GZCH: a change to blocks that closes a cycle is refused and leaves the edge as it was", () => {
    const doc = samplePlan([blocks("e1", "t1", "t2"), blocks("e2", "t2", "x1"), relates("e3", "x1", "t1")]);
    const r = refusal(setEdgeType(doc, "e3", "blocks"));
    expect(r).toEqual({ code: "dependency_cycle", itemIds: ["x1", "t2", "t1"] });
    expect(doc.edges[2].type).toBe("relates");
  });

  it("FLO-FR-CEVI: a blocks edge is reversed", () => {
    const doc = samplePlan([blocks("e1", "t1", "t2")]);
    expect(ok(reverseEdge(doc, "e1")).edges).toEqual([
      { id: "e1", from: "t2", to: "t1", type: "blocks" },
    ]);
  });

  it("FLO-FR-CEVI, FLO-FR-GZCH: a reversal that closes a cycle is refused and leaves the edge as it was", () => {
    // t1 blocks t2 blocks t3, and t1 blocks t3. Reversing e3 makes t1 wait for t3.
    const doc = samplePlan([blocks("e1", "t1", "t2"), blocks("e2", "t2", "t3"), blocks("e3", "t1", "t3")]);
    const r = refusal(reverseEdge(doc, "e3"));
    expect(r.code).toBe("dependency_cycle");
    expect([...r.itemIds!].sort()).toEqual(["t1", "t2", "t3"]);
    expect(doc.edges[2]).toEqual(blocks("e3", "t1", "t3"));
  });

  it("FLO-FR-CEVI: only a blocks edge has a direction to reverse", () => {
    const doc = samplePlan([relates("e1", "t1", "t2")]);
    expect(refusal(reverseEdge(doc, "e1")).code).toBe("not_blocks");
  });

  it("FLO-FR-CEVI: an unknown edge is refused", () => {
    expect(refusal(setEdgeType(samplePlan(), "nope", "relates")).code).toBe("unknown_edge");
    expect(refusal(reverseEdge(samplePlan(), "nope")).code).toBe("unknown_edge");
  });
});

describe("removeEdge (FLO-FR-20)", () => {
  it("FLO-FR-20: only the edge goes; both items and their other edges stay", () => {
    const doc = samplePlan([blocks("e1", "t1", "t2"), blocks("e2", "t2", "t3")]);
    const next = removeEdge(doc, "e1");
    expect(next.edges.map((e) => e.id)).toEqual(["e2"]);
    expect(next.items).toBe(doc.items);
    expect(removeEdge(next, "e1")).toBe(next);
  });
});
