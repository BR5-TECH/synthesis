/**
 * The semantic edits of a GitHub plan (MDL-FR-XJBV, MDL-FR-PUCM).
 */
import { describe, expect, it } from "vitest";

import {
  addFlowReference,
  addFootprintEntry,
  addItem,
  addItemArtifact,
  connect,
  planEditsBetween,
  removeEdge,
  renameItem,
  reparentItem,
  reverseEdge,
  setDue,
  setEdgeType,
  setItemDescription,
  setItemDraft,
  setItemKind,
  touchedItems,
  type FlowDocument,
  type FlowEditResult,
} from "./flowDocument";
import { blocks, item, plan, relates, samplePlan } from "../test/flowPlanFixtures";

function ok(r: FlowEditResult): FlowDocument {
  if (!r.ok) throw new Error(`expected the edit to be accepted, got ${r.refusal.code}`);
  return r.doc;
}

const stamp = (id: string) => `at-${id}`;
const edits = (before: FlowDocument, after: FlowDocument) =>
  planEditsBetween(before, after, stamp);

describe("planEditsBetween (MDL-FR-XJBV)", () => {
  it("MDL-FR-XJBV: an add is a create_item in its container", () => {
    const before = samplePlan();
    const added = addItem(before, "task", "f1");
    if (!added.ok) throw new Error("not added");
    expect(edits(before, added.doc)).toEqual([
      { kind: "create_item", itemKind: "task", name: "New task", containerId: "f1" },
    ]);
    const top = addItem(before, "milestone");
    if (!top.ok) throw new Error("not added");
    expect(edits(before, top.doc)[0]).toMatchObject({ containerId: null, itemKind: "milestone" });
  });

  it("MDL-FR-XJBV: a rename, a description, and a due date carry the item's updated_at", () => {
    const before = samplePlan();
    expect(edits(before, renameItem(before, "t1", "Tokenize"))).toEqual([
      { kind: "rename", itemId: "t1", name: "Tokenize", updatedAt: "at-t1" },
    ]);
    expect(edits(before, setItemDescription(before, "t1", "**Now**"))).toEqual([
      { kind: "set_description", itemId: "t1", description: "**Now**", updatedAt: "at-t1" },
    ]);
    const dated = ok(setDue(before, "m1", "2027-03-31"));
    expect(edits(before, dated)).toEqual([
      { kind: "set_due", itemId: "m1", due: "2027-03-31", updatedAt: "at-m1" },
    ]);
    expect(edits(dated, ok(setDue(dated, "m1", null)))).toEqual([
      { kind: "set_due", itemId: "m1", due: null, updatedAt: "at-m1" },
    ]);
  });

  it("MDL-FR-XJBV: a cleared description is an empty one", () => {
    const before = setItemDescription(samplePlan(), "t1", "Old");
    expect(edits(before, setItemDescription(before, "t1", ""))).toEqual([
      { kind: "set_description", itemId: "t1", description: "", updatedAt: "at-t1" },
    ]);
  });

  it("MDL-FR-XJBV: a drop into another container is a move, to null at the top level", () => {
    const before = samplePlan();
    expect(edits(before, ok(reparentItem(before, "t1", "f2")))).toEqual([
      { kind: "move", itemId: "t1", containerId: "f2", updatedAt: "at-t1" },
    ]);
    expect(edits(before, ok(reparentItem(before, "t1")))).toEqual([
      { kind: "move", itemId: "t1", containerId: null, updatedAt: "at-t1" },
    ]);
  });

  it("MDL-FR-XJBV: a kind change to a container unlinks the draft and clears the footprint first", () => {
    const linked = ok(setItemDraft(samplePlan(), "x1", "dr_1"));
    const before = ok(addFootprintEntry(linked, "x1", "src/"));
    const after = ok(setItemKind(before, "x1", "feature"));
    expect(edits(before, after)).toEqual([
      { kind: "unlink_draft", itemId: "x1" },
      { kind: "set_footprint", itemId: "x1", footprint: [] },
      { kind: "set_kind", itemId: "x1", itemKind: "feature", updatedAt: "at-x1" },
    ]);
  });

  it("MDL-FR-XJBV: references, a footprint, and a draft link are overlay edits with the whole list", () => {
    const before = addItemArtifact(samplePlan(), "t1", "docs/a.md");
    expect(edits(before, addItemArtifact(before, "t1", "docs/b.md"))).toEqual([
      { kind: "set_references", itemId: "t1", artifactIds: ["docs/a.md", "docs/b.md"] },
    ]);
    expect(edits(before, ok(addFootprintEntry(before, "t1", "src/")))).toEqual([
      { kind: "set_footprint", itemId: "t1", footprint: ["src/"] },
    ]);
    const linked = ok(setItemDraft(before, "t1", "dr_7"));
    expect(edits(before, linked)).toEqual([{ kind: "link_draft", itemId: "t1", draftId: "dr_7" }]);
    expect(edits(linked, ok(setItemDraft(linked, "t1", null)))).toEqual([
      { kind: "unlink_draft", itemId: "t1" },
    ]);
  });

  it("MDL-FR-XJBV: the plan's references are one set_plan_references edit", () => {
    const before = samplePlan();
    expect(edits(before, addFlowReference(before, "docs/hld.md"))).toEqual([
      { kind: "set_plan_references", references: ["docs/hld.md"] },
    ]);
  });

  it("MDL-FR-XJBV: a connection is an add_edge with the blocked end's updated_at", () => {
    const before = samplePlan();
    const r = connect(before, "t1", "t2");
    if (!r.ok) throw new Error("not connected");
    expect(edits(before, r.doc)).toEqual([
      { kind: "add_edge", from: "t1", to: "t2", type: "blocks", updatedAt: "at-t2" },
    ]);
  });

  it("MDL-FR-XJBV: a change of type or a reversal removes the edge and adds the new one", () => {
    const before = samplePlan([blocks("e1", "t1", "t2")]);
    expect(edits(before, ok(setEdgeType(before, "e1", "relates")))).toEqual([
      { kind: "remove_edge", from: "t1", to: "t2", type: "blocks", updatedAt: "at-t2" },
      { kind: "add_edge", from: "t1", to: "t2", type: "relates", updatedAt: "at-t2" },
    ]);
    expect(edits(before, ok(reverseEdge(before, "e1")))).toEqual([
      { kind: "remove_edge", from: "t1", to: "t2", type: "blocks", updatedAt: "at-t2" },
      { kind: "add_edge", from: "t2", to: "t1", type: "blocks", updatedAt: "at-t1" },
    ]);
  });

  it("MDL-FR-XJBV: removing an edge is a remove_edge", () => {
    const before = samplePlan([relates("e1", "t1", "x1")]);
    expect(edits(before, removeEdge(before, "e1"))).toEqual([
      { kind: "remove_edge", from: "t1", to: "x1", type: "relates", updatedAt: "at-x1" },
    ]);
  });

  it("MDL-FR-XQGD: a removal is not an edit here, because the host decides it", () => {
    const before = plan([item("t1", "task")]);
    expect(edits(before, { ...before, items: [] })).toEqual([]);
  });

  it("MDL-FR-XJBV: the same plan gives no edit", () => {
    const before = samplePlan();
    expect(edits(before, before)).toEqual([]);
  });
});

describe("touchedItems (MDL-FR-PUCM)", () => {
  it("MDL-FR-PUCM: the changed item and each end of a changed edge carry the pending mark", () => {
    const before = samplePlan([blocks("e1", "t1", "t2")]);
    expect(touchedItems(before, renameItem(before, "t3", "Card form"))).toEqual(["t3"]);
    expect(touchedItems(before, removeEdge(before, "e1")).sort()).toEqual(["t1", "t2"]);
    const added = addItem(before, "task", "f1");
    if (!added.ok) throw new Error("not added");
    expect(touchedItems(before, added.doc)).toEqual([added.itemId]);
  });
});
