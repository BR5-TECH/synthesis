import { describe, expect, it } from "vitest";
import {
  applyEdit,
  canCreateUnder,
  canMove,
  deleteOutcome,
  moveTargets,
  newNodeId,
} from "./organize";
import { buildTree, indexChildren, specChildren } from "./tree";
import type { IndexNode, SpecificationIndex } from "./types";
import { smallIndex } from "../../test/specMapFixtures";

const find = (index: SpecificationIndex, id: string): IndexNode =>
  buildTree(index).nodes.get(id)!;
const childIds = (index: SpecificationIndex, id: string) =>
  indexChildren(find(index, id)).map((n) => n.id);
const specCodes = (index: SpecificationIndex, id: string) =>
  specChildren(find(index, id)).map((s) => s.code);

describe("creating and editing index nodes", () => {
  it("SMO-FR-NXDL: a creation appends a childless node as the last child, or as the last root", () => {
    const base = smallIndex();
    const under = applyEdit(base, {
      kind: "create",
      parentId: "f1",
      node: { id: "n1", label: "  planning ", summary: "Work ahead." },
    });
    expect(childIds(under, "f1")).toEqual(["g1", "g2", "n1"]);
    expect(find(under, "n1")).toEqual({ id: "n1", label: "planning", summary: "Work ahead.", children: [] });

    const root = applyEdit(base, {
      kind: "create",
      parentId: null,
      node: { id: "n2", label: "later", summary: "" },
    });
    expect(root.roots.map((r) => r.id)).toEqual(["d1", "d2", "n2"]);
    // Pure: the index handed in is unchanged.
    expect(childIds(base, "f1")).toEqual(["g1", "g2"]);
  });

  it("SMO-FR-UAKE, SMO-FR-HCQN: no index node is created under a leaf group, and a name is required", () => {
    const base = smallIndex();
    const tree = buildTree(base);
    expect(canCreateUnder(tree, "f1")).toBe(true);
    expect(canCreateUnder(tree, "g1")).toBe(false);
    expect(
      applyEdit(base, { kind: "create", parentId: "g1", node: { id: "n", label: "x", summary: "" } }),
    ).toBe(base);
    expect(
      applyEdit(base, { kind: "create", parentId: "f1", node: { id: "n", label: "   ", summary: "" } }),
    ).toBe(base);
    expect(tree.nodes.has(newNodeId(tree))).toBe(false);
  });

  it("SMO-FR-PZEI: an edit changes the label and the summary of one node", () => {
    const edited = applyEdit(smallIndex(), {
      kind: "edit",
      nodeId: "g2",
      label: "renamed",
      summary: "New summary.",
    });
    expect(find(edited, "g2").label).toBe("renamed");
    expect(find(edited, "g2").summary).toBe("New summary.");
    expect(specCodes(edited, "g2")).toEqual(["A3"]);
  });
});

describe("moving nodes (SMO-FR-WGOF)", () => {
  it("SMO-FR-WGOF: a spec drops only on a leaf group", () => {
    const tree = buildTree(smallIndex());
    const a1 = { kind: "spec", code: "A1" } as const;
    expect(canMove(tree, a1, "g4")).toBe(true);
    expect(canMove(tree, a1, "f1")).toBe(false);
    expect(canMove(tree, a1, null)).toBe(false);
  });

  it("SMO-FR-WGOF: an index node drops only one level up, and a root node only among roots", () => {
    const tree = buildTree(smallIndex());
    const g1 = { kind: "index", id: "g1" } as const;
    expect(canMove(tree, g1, "f2")).toBe(true);
    expect(canMove(tree, g1, "d1")).toBe(false);
    expect(canMove(tree, g1, "g2")).toBe(false);
    expect(canMove(tree, g1, null)).toBe(false);
    expect(canMove(tree, { kind: "index", id: "d2" }, null)).toBe(true);
    expect(canMove(tree, { kind: "index", id: "d2" }, "d1")).toBe(false);
  });

  it("SMO-FR-WGOF, SMD-FR-QPAM: a planned chip drops on any index node", () => {
    const planned = applyEdit(smallIndex(), {
      kind: "attachDraft",
      nodeId: "g1",
      draft: { draftId: "dr", name: "Untitled" },
    });
    const tree = buildTree(planned);
    const ref = { kind: "planned", draftId: "dr" } as const;
    expect(canMove(tree, ref, "d2")).toBe(true);
    expect(canMove(tree, ref, "f3")).toBe(true);
    expect(canMove(tree, ref, null)).toBe(false);
    const moved = applyEdit(planned, { kind: "move", ref, parentId: "d2", index: 0 });
    expect(find(moved, "d2").planned).toEqual([{ draftId: "dr", name: "Untitled" }]);
    expect(find(moved, "g1").planned).toBeUndefined();
  });

  it("SMO-FR-ARQV: a drop on the current parent reorders, and a drop elsewhere inserts at the position", () => {
    const base = smallIndex();
    const reordered = applyEdit(base, {
      kind: "move",
      ref: { kind: "index", id: "g2" },
      parentId: "f1",
      index: 0,
    });
    expect(childIds(reordered, "f1")).toEqual(["g2", "g1"]);

    const inserted = applyEdit(base, {
      kind: "move",
      ref: { kind: "spec", code: "A1" },
      parentId: "g4",
      index: 1,
    });
    expect(specCodes(inserted, "g4")).toEqual(["C1", "A1", "C2"]);
    expect(specCodes(inserted, "g1")).toEqual(["A2"]);
  });

  it("SMO-FR-WGOF: a refused move returns the same index", () => {
    const base = smallIndex();
    expect(
      applyEdit(base, { kind: "move", ref: { kind: "spec", code: "A1" }, parentId: "f1", index: 0 }),
    ).toBe(base);
  });

  it("SMO-FR-SLNC: Move to lists every valid new parent, and moves to the last child", () => {
    const base = smallIndex();
    const tree = buildTree(base);
    expect(moveTargets(tree, "g1")).toEqual(["f2", "f3"]);
    expect(moveTargets(tree, "f3")).toEqual(["d1"]);
    expect(moveTargets(tree, "d1")).toEqual([]);
    const moved = applyEdit(base, {
      kind: "move",
      ref: { kind: "index", id: "g1" },
      parentId: "f3",
      index: indexChildren(find(base, "f3")).length,
    });
    expect(childIds(moved, "f3")).toEqual(["g4", "g1"]);
  });
});

describe("deleting index nodes", () => {
  it("SMO-FR-OBRF: the children move to the end of the previous sibling", () => {
    const base = applyEdit(smallIndex(), {
      kind: "attachDraft",
      nodeId: "g2",
      draft: { draftId: "p", name: "Plan" },
    });
    expect(deleteOutcome(buildTree(base), "g2")).toEqual({ allowed: true, siblingId: "g1" });
    const deleted = applyEdit(base, { kind: "delete", nodeId: "g2" });
    expect(childIds(deleted, "f1")).toEqual(["g1"]);
    expect(specCodes(deleted, "g1")).toEqual(["A1", "A2", "A3"]);
    expect(find(deleted, "g1").planned).toEqual([{ draftId: "p", name: "Plan" }]);
  });

  it("SMO-FR-OBRF: a first child gives its children to the start of the next sibling", () => {
    const base = smallIndex();
    expect(deleteOutcome(buildTree(base), "g1")).toEqual({ allowed: true, siblingId: "g2" });
    const deleted = applyEdit(base, { kind: "delete", nodeId: "g1" });
    expect(specCodes(deleted, "g2")).toEqual(["A1", "A2", "A3"]);
  });

  it("SMO-FR-GQTS: a node with no sibling is deleted only when it has no children", () => {
    const base = smallIndex();
    expect(deleteOutcome(buildTree(base), "g3")).toEqual({ allowed: false, siblingId: null });
    expect(applyEdit(base, { kind: "delete", nodeId: "g3" })).toBe(base);

    const emptied = applyEdit(
      applyEdit(base, { kind: "move", ref: { kind: "spec", code: "B1" }, parentId: "g1", index: 0 }),
      { kind: "attachDraft", nodeId: "g3", draft: { draftId: "q", name: "Q" } },
    );
    expect(deleteOutcome(buildTree(emptied), "g3")).toEqual({ allowed: true, siblingId: null });
    const deleted = applyEdit(emptied, { kind: "delete", nodeId: "g3" });
    expect(childIds(deleted, "f2")).toEqual([]);
    // The planned drafts move to the parent.
    expect(find(deleted, "f2").planned).toEqual([{ draftId: "q", name: "Q" }]);
  });

  it("SMO-FR-GQTS: an empty root node with no sibling discards its placements", () => {
    const base: SpecificationIndex = {
      ...smallIndex(),
      roots: [{ id: "solo", label: "solo", summary: "", children: [], planned: [{ draftId: "z", name: "Z" }] }],
    };
    const deleted = applyEdit(base, { kind: "delete", nodeId: "solo" });
    expect(deleted.roots).toEqual([]);
    expect(buildTree(deleted).planned.size).toBe(0);
  });
});

describe("draft placements", () => {
  it("SMD-FR-OYLC: attaching a draft places it on one node only", () => {
    const once = applyEdit(smallIndex(), {
      kind: "attachDraft",
      nodeId: "f1",
      draft: { draftId: "d-1", name: "Untitled" },
    });
    expect(find(once, "f1").planned).toEqual([{ draftId: "d-1", name: "Untitled" }]);
    const again = applyEdit(once, {
      kind: "attachDraft",
      nodeId: "d2",
      draft: { draftId: "d-1", name: "Untitled" },
    });
    expect(find(again, "f1").planned).toBeUndefined();
    expect(find(again, "d2").planned).toHaveLength(1);
  });

  it("SMD-FR-LKVU: a rename follows the draft, and a detach removes its chip", () => {
    const placed = applyEdit(smallIndex(), {
      kind: "attachDraft",
      nodeId: "g4",
      draft: { draftId: "d-2", name: "Untitled" },
    });
    const renamed = applyEdit(placed, { kind: "renameDraft", draftId: "d-2", name: "Planning" });
    expect(find(renamed, "g4").planned).toEqual([{ draftId: "d-2", name: "Planning" }]);
    const detached = applyEdit(renamed, { kind: "detachDraft", draftId: "d-2" });
    expect(buildTree(detached).planned.size).toBe(0);
    expect(applyEdit(detached, { kind: "detachDraft", draftId: "d-2" })).toBe(detached);
  });
});
