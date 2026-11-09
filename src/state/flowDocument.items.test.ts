/**
 * The edit operations on items and on the Flow's own fields (FLO-FR-07 to
 * FLO-FR-13, FLO-FR-25, FLO-FR-YDWT, FLO-FR-45, FLO-FR-HGIY, FLO-FR-NRBV,
 * FLO-FR-RMVR, FLO-FR-ZEJU, FLO-FR-XAPK, FLO-FR-TZSU, FLO-FR-OAOZ).
 */
import { describe, expect, it } from "vitest";

import {
  addFlowReference,
  addFootprintEntry,
  addItem,
  addItemArtifact,
  allowedKinds,
  DEFAULT_CONTAINER_SIZES,
  emptyFlow,
  findItem,
  isCalendarDate,
  isFootprintPath,
  itemLinkingDraft,
  kindChangeDrops,
  removalScope,
  removeFlowReference,
  removeFootprintEntry,
  removeItem,
  removeItemArtifact,
  renameItem,
  setDue,
  setFlowDescription,
  setFlowName,
  setItemDescription,
  setItemDraft,
  setItemKind,
  setLocalOnly,
  type FlowDocument,
  type FlowEditResult,
} from "./flowDocument";
import {
  blocks,
  ids,
  issueBase,
  issueRef,
  item,
  plan,
  relates,
  samplePlan,
  withItems,
} from "../test/flowPlanFixtures";

function ok(r: FlowEditResult): FlowDocument {
  if (!r.ok) throw new Error(`expected the edit to be accepted, got ${r.refusal.code}`);
  return r.doc;
}

/** The document and the new item's id of an accepted `addItem`. */
function added(r: FlowEditResult<{ itemId: string }>) {
  if (!r.ok) throw new Error(`expected the item to be added, got ${r.refusal.code}`);
  return { doc: r.doc, itemId: r.itemId };
}

function refusal(r: FlowEditResult) {
  if (r.ok) throw new Error("expected the edit to be refused");
  return r.refusal;
}

describe("addItem (FLO-FR-07, FLO-FR-42, FLO-FR-OAOZ)", () => {
  it("FLO-FR-07: a new deliverable gets its default name and default size", () => {
    const { doc, itemId } = added(addItem(emptyFlow(), "deliverable", { x: 40, y: 50 }));
    expect(itemId).toBe("d1");
    expect(doc.items).toEqual([
      {
        id: "d1",
        kind: "deliverable",
        name: "New deliverable",
        position: { x: 40, y: 50 },
        size: { width: 760, height: 480 },
      },
    ]);
    // The size is a copy, so an edit to it cannot change the default.
    expect(doc.items[0].size).not.toBe(DEFAULT_CONTAINER_SIZES.deliverable);
  });

  it("FLO-FR-07, FLO-FR-42: a task added in a feature stores its position in the feature's coordinates", () => {
    const { doc, itemId } = added(addItem(samplePlan(), "task", { x: 100, y: 200 }, "f1"));
    expect(findItem(doc, itemId)).toEqual({
      id: "t4",
      kind: "task",
      name: "New task",
      parentId: "f1",
      position: { x: 60, y: 80 },
    });
  });

  it("FLO-FR-07: each kind gets its own default name, and only a container gets a size", () => {
    const base = samplePlan();
    const cases = [
      ["milestone", "d1", "New milestone", true],
      ["epic", "m1", "New epic", true],
      ["feature", "ep1", "New feature", true],
      ["defect", "ep1", "New defect", false],
    ] as const;
    for (const [kind, parent, name, sized] of cases) {
      const { doc, itemId } = added(addItem(base, kind, { x: 0, y: 0 }, parent));
      const placed = findItem(doc, itemId)!;
      expect(placed.name).toBe(name);
      expect(placed.size !== undefined).toBe(sized);
    }
  });

  it("FLO-FR-OAOZ: a placement in a container the kind does not permit is refused and names the permitted containers", () => {
    expect(refusal(addItem(samplePlan(), "task", { x: 0, y: 0 }))).toEqual({
      code: "invalid_parent_kind",
      permitted: ["feature"],
    });
    expect(refusal(addItem(samplePlan(), "milestone", { x: 0, y: 0 }, "f1"))).toEqual({
      code: "invalid_parent_kind",
      permitted: ["deliverable"],
    });
    expect(refusal(addItem(samplePlan(), "deliverable", { x: 0, y: 0 }, "d1")).code).toBe(
      "invalid_parent_kind",
    );
  });

  it("FLO-FR-OAOZ: a placement in a leaf or in an item that is not there is refused", () => {
    expect(refusal(addItem(samplePlan(), "task", { x: 0, y: 0 }, "t1")).code).toBe(
      "invalid_parent_kind",
    );
    expect(refusal(addItem(samplePlan(), "task", { x: 0, y: 0 }, "nope")).code).toBe(
      "unknown_parent",
    );
  });

  it("FLO-FR-06: a new id never repeats one in use, also after a removal", () => {
    const after = removeItem(samplePlan(), "t2");
    const { doc, itemId } = added(addItem(after, "task", { x: 0, y: 0 }, "f1"));
    // Two tasks are left (t1, t3), so the search starts at t3, which is taken.
    expect(itemId).toBe("t4");
    expect(new Set(ids(doc)).size).toBe(doc.items.length);
  });

  it("FGV-FR-15: a new id never takes the id of a removed item that keeps its sync link", () => {
    const linked = {
      ...samplePlan(),
      sync: {
        tracker: "github" as const,
        links: [{ itemId: "t3", ref: issueRef(3), base: issueBase("T3") }],
      },
    };
    // Two tasks are left, so the search starts at t3, which the link keeps.
    const { itemId } = added(addItem(removeItem(linked, "t3"), "task", { x: 0, y: 0 }, "f1"));
    expect(itemId).toBe("t4");
  });

    it("FLO-FR-07: the input document does not change", () => {
    const before = samplePlan();
    const copy = structuredClone(before);
    addItem(before, "task", { x: 0, y: 0 }, "f1");
    expect(before).toEqual(copy);
  });
});

describe("removeItem (FLO-FR-08, FLO-FR-45)", () => {
  const edges = [
    blocks("e1", "t1", "t2"),
    blocks("e2", "t3", "x1"),
    relates("e3", "f2", "f1"),
    blocks("e4", "ep1", "x1"),
  ];

  it("FLO-FR-08: a leaf goes with every edge it is the source or the target of", () => {
    const doc = removeItem(samplePlan(edges), "t1");
    expect(ids(doc)).not.toContain("t1");
    expect(doc.edges.map((e) => e.id)).toEqual(["e2", "e3", "e4"]);
  });

  it("FLO-FR-45: a container goes with every item under it and all their edges", () => {
    const doc = removeItem(samplePlan(edges), "ep1");
    expect(ids(doc)).toEqual(["d1", "m1", "f1", "t1", "t2", "x1"]);
    expect(doc.edges.map((e) => e.id)).toEqual(["e1"]);
  });

  it("FLO-FR-45: removalScope counts the items under a container and every edge that goes", () => {
    expect(removalScope(samplePlan(edges), "ep1")).toEqual({ itemCount: 2, edgeCount: 3 });
    expect(removalScope(samplePlan(edges), "d1")).toEqual({ itemCount: 8, edgeCount: 4 });
    expect(removalScope(samplePlan(edges), "t2")).toEqual({ itemCount: 0, edgeCount: 1 });
    expect(removalScope(samplePlan(edges), "nope")).toEqual({ itemCount: 0, edgeCount: 0 });
  });

  it("FLO-FR-45: an empty container goes alone, with its edges", () => {
    const doc = withItems(samplePlan(), item("f9", "feature", { parentId: "m1" }));
    expect(removalScope(doc, "f9").itemCount).toBe(0);
    expect(ids(removeItem(doc, "f9"))).not.toContain("f9");
  });

  it("FGV-FR-15: the sync link of a removed item stays as the record of a local removal", () => {
    const doc = samplePlan();
    doc.sync = {
      tracker: "github",
      links: [{ itemId: "t1", ref: issueRef(1), base: issueBase("T1") }],
    };
    expect(removeItem(doc, "f1").sync?.links.map((l) => l.itemId)).toEqual(["t1"]);
  });

  it("FLO-FR-08: an unknown id changes nothing", () => {
    const doc = samplePlan(edges);
    expect(removeItem(doc, "nope")).toBe(doc);
  });
});

describe("names, descriptions and references (FLO-FR-09, FLO-FR-10, FLO-FR-13, FLO-FR-25, FLO-FR-YDWT)", () => {
  it("FLO-FR-09: a name is free text and need not be unique", () => {
    const doc = renameItem(renameItem(samplePlan(), "t1", "Same"), "t2", "Same");
    expect(findItem(doc, "t1")?.name).toBe("Same");
    expect(findItem(doc, "t2")?.name).toBe("Same");
    expect(renameItem(doc, "t1", "Same")).toBe(doc);
    expect(renameItem(doc, "nope", "X")).toBe(doc);
  });

  it("FLO-FR-13: a description is set, and an empty one is absent", () => {
    const set = setItemDescription(samplePlan(), "t1", "Use **tokens**.");
    expect(findItem(set, "t1")?.description).toBe("Use **tokens**.");
    const cleared = setItemDescription(set, "t1", "");
    expect("description" in findItem(cleared, "t1")!).toBe(false);
    expect(setItemDescription(cleared, "t1", "")).toBe(cleared);
  });

  it("FLO-FR-10: an item's references are added in order and never repeat", () => {
    let doc = addItemArtifact(samplePlan(), "f1", "a.md");
    doc = addItemArtifact(doc, "f1", "b.md");
    expect(findItem(doc, "f1")?.artifactIds).toEqual(["a.md", "b.md"]);
    expect(addItemArtifact(doc, "f1", "a.md")).toBe(doc);
    expect(addItemArtifact(doc, "f1", "")).toBe(doc);
    expect(addItemArtifact(doc, "nope", "c.md")).toBe(doc);
  });

  it("FLO-FR-10: each reference is removed on its own, and the last removes the field", () => {
    let doc = addItemArtifact(addItemArtifact(samplePlan(), "t1", "a.md"), "t1", "b.md");
    doc = removeItemArtifact(doc, "t1", "a.md");
    expect(findItem(doc, "t1")?.artifactIds).toEqual(["b.md"]);
    doc = removeItemArtifact(doc, "t1", "b.md");
    expect("artifactIds" in findItem(doc, "t1")!).toBe(false);
    expect(removeItemArtifact(doc, "t1", "b.md")).toBe(doc);
  });

  it("FLO-FR-YDWT: the Flow's references are added in order, never repeat, and are removed one at a time", () => {
    let doc = addFlowReference(samplePlan(), "docs/hld.md");
    doc = addFlowReference(doc, "docs/adr.md");
    expect(doc.references).toEqual(["docs/hld.md", "docs/adr.md"]);
    expect(addFlowReference(doc, "docs/hld.md")).toBe(doc);
    doc = removeFlowReference(doc, "docs/hld.md");
    expect(doc.references).toEqual(["docs/adr.md"]);
    expect(removeFlowReference(doc, "docs/hld.md")).toBe(doc);
  });

  it("FLO-FR-25: the Flow's name is set, and an empty description is absent", () => {
    const named = setFlowName(samplePlan(), "Payments");
    expect(named.name).toBe("Payments");
    expect(setFlowName(named, "Payments")).toBe(named);
    const described = setFlowDescription(named, "EU launch");
    expect(described.description).toBe("EU launch");
    const cleared = setFlowDescription(described, "");
    expect("description" in cleared).toBe(false);
    expect(setFlowDescription(cleared, "")).toBe(cleared);
  });
});

describe("setItemKind (FLO-FR-HGIY, FGV-FR-MZIA)", () => {
  it("FLO-FR-HGIY: a task becomes a defect and keeps its draft and footprint", () => {
    const doc = samplePlan();
    doc.items[3] = { ...doc.items[3], draftId: "dr1", footprint: ["src/"] };
    const next = ok(setItemKind(doc, "t1", "defect"));
    expect(findItem(next, "t1")).toMatchObject({ kind: "defect", draftId: "dr1", footprint: ["src/"] });
  });

  it("FLO-FR-HGIY, FGV-FR-MZIA: a leaf that becomes a container loses its draft and footprint and gets a default size", () => {
    const doc = samplePlan();
    doc.items[8] = { ...doc.items[8], draftId: "dr1", footprint: ["src/"] };
    expect(kindChangeDrops(doc, "x1", "feature")).toEqual(["draftId", "footprint"]);
    const next = ok(setItemKind(doc, "x1", "feature"));
    const x1 = findItem(next, "x1")!;
    expect(x1.kind).toBe("feature");
    expect(x1.draftId).toBeUndefined();
    expect(x1.footprint).toBeUndefined();
    expect(x1.size).toEqual(DEFAULT_CONTAINER_SIZES.feature);
  });

  it("FLO-FR-HGIY: a change to a kind that keeps every field drops nothing", () => {
    expect(kindChangeDrops(samplePlan(), "t1", "defect")).toEqual([]);
    expect(kindChangeDrops(samplePlan(), "nope", "defect")).toEqual([]);
  });

  it("FLO-FR-HGIY, FGV-FR-MZIA: an empty container becomes a leaf and loses its size", () => {
    const doc = withItems(samplePlan(), item("f9", "feature", { parentId: "m1" }));
    const next = ok(setItemKind(doc, "f9", "defect"));
    expect(findItem(next, "f9")).toEqual({
      id: "f9",
      kind: "defect",
      name: "F9",
      parentId: "m1",
      position: { x: 0, y: 0 },
    });
  });

  it("FLO-FR-HGIY: a container that becomes another container keeps its size", () => {
    const doc = withItems(
      samplePlan(),
      item("f9", "feature", { parentId: "m1", size: { width: 333, height: 222 } }),
    );
    expect(findItem(ok(setItemKind(doc, "f9", "epic")), "f9")?.size).toEqual({
      width: 333,
      height: 222,
    });
  });

  it("FLO-FR-HGIY: a container that holds items cannot become a leaf", () => {
    expect(refusal(setItemKind(samplePlan(), "f1", "defect"))).toEqual({
      code: "not_empty",
      itemIds: ["t1", "t2"],
    });
  });

  it("FLO-FR-HGIY: a kind that a member cannot sit in is refused and names the members", () => {
    expect(refusal(setItemKind(samplePlan(), "f1", "epic"))).toEqual({
      code: "member_not_permitted",
      itemIds: ["t1", "t2"],
    });
    expect(refusal(setItemKind(samplePlan(), "ep1", "feature"))).toEqual({
      code: "member_not_permitted",
      itemIds: ["f2"],
    });
  });

  it("FLO-FR-HGIY: a kind that the container does not permit is refused", () => {
    expect(refusal(setItemKind(samplePlan(), "t1", "feature"))).toEqual({
      code: "invalid_parent_kind",
      permitted: ["milestone", "epic"],
    });
    expect(refusal(setItemKind(samplePlan(), "m1", "epic")).code).toBe("invalid_parent_kind");
  });

  it("FLO-FR-HGIY: the same kind changes nothing, and an unknown item is refused", () => {
    const doc = samplePlan();
    expect(ok(setItemKind(doc, "t1", "task"))).toBe(doc);
    expect(refusal(setItemKind(doc, "nope", "task")).code).toBe("unknown_item");
  });

  it("FLO-FR-HGIY: allowedKinds offers only the kinds a change accepts", () => {
    const doc = samplePlan();
    expect(allowedKinds(doc, "x1")).toEqual(["epic", "feature", "defect"]);
    expect(allowedKinds(doc, "t1")).toEqual(["task", "defect"]);
    expect(allowedKinds(doc, "f1")).toEqual(["feature"]);
    expect(allowedKinds(doc, "d1")).toEqual(["deliverable"]);
    expect(allowedKinds(doc, "nope")).toEqual([]);
  });
});

describe("draft links (FLO-FR-NRBV, FLO-FR-RMVR, FGV-FR-CTFX)", () => {
  it("FLO-FR-NRBV: a task stores the draft id it is linked to", () => {
    const doc = ok(setItemDraft(samplePlan(), "t1", "dr_7f3a"));
    expect(findItem(doc, "t1")?.draftId).toBe("dr_7f3a");
    expect(itemLinkingDraft(doc, "dr_7f3a")).toBe("t1");
    expect(itemLinkingDraft(doc, "dr_other")).toBeUndefined();
  });

  it("FLO-FR-RMVR, FGV-FR-CTFX: a draft that another item links is refused and names that item", () => {
    const doc = ok(setItemDraft(samplePlan(), "t1", "dr1"));
    expect(refusal(setItemDraft(doc, "x1", "dr1"))).toEqual({
      code: "draft_already_linked",
      itemIds: ["t1"],
    });
    expect(ok(setItemDraft(doc, "t1", "dr1"))).toBe(doc);
  });

  it("FLO-FR-RMVR: linking another draft replaces the link", () => {
    const doc = ok(setItemDraft(ok(setItemDraft(samplePlan(), "t1", "dr1")), "t1", "dr2"));
    expect(findItem(doc, "t1")?.draftId).toBe("dr2");
    expect(itemLinkingDraft(doc, "dr1")).toBeUndefined();
  });

  it("FLO-FR-RMVR: unlinking an item with no draft, or a container, changes nothing", () => {
    const doc = samplePlan();
    expect(ok(setItemDraft(doc, "f1", null))).toBe(doc);
  });

    it("FLO-FR-RMVR: unlinking removes the field and changes nothing else", () => {
    const doc = ok(setItemDraft(samplePlan(), "t1", "dr1"));
    const unlinked = ok(setItemDraft(doc, "t1", null));
    expect("draftId" in findItem(unlinked, "t1")!).toBe(false);
    expect(ok(setItemDraft(unlinked, "t1", null))).toBe(unlinked);
    expect("draftId" in findItem(ok(setItemDraft(doc, "t1", "")), "t1")!).toBe(false);
  });

  it("FGV-FR-MZIA: a container cannot link a draft, and an unknown item is refused", () => {
    expect(refusal(setItemDraft(samplePlan(), "f1", "dr1")).code).toBe("not_leaf");
    expect(refusal(setItemDraft(samplePlan(), "nope", "dr1")).code).toBe("unknown_item");
  });
});

describe("footprints (FLO-FR-ZEJU, FGV-FR-RQEM)", () => {
  it("FLO-FR-ZEJU: paths are added in order, and a folder keeps its trailing slash", () => {
    let doc = ok(addFootprintEntry(samplePlan(), "t1", "src/payments/"));
    doc = ok(addFootprintEntry(doc, "t1", "README.md"));
    expect(findItem(doc, "t1")?.footprint).toEqual(["src/payments/", "README.md"]);
  });

  it("FGV-FR-RQEM: an empty, absolute, or `..` path is refused", () => {
    for (const bad of ["", "/etc/passwd", "\\share", "C:/x", "c:x", "a/../b", "..", "a\\..\\b"]) {
      expect(isFootprintPath(bad), bad).toBe(false);
      expect(refusal(addFootprintEntry(samplePlan(), "t1", bad)).code).toBe(
        "invalid_footprint_path",
      );
    }
  });

  it("FGV-FR-RQEM: a segment that only starts with two dots is a path like any other", () => {
    expect(isFootprintPath("a/..b")).toBe(true);
    expect(isFootprintPath("...")).toBe(true);
    expect(isFootprintPath("src/.github/")).toBe(true);
  });

  it("FGV-FR-RQEM: a path the footprint holds is refused", () => {
    const doc = ok(addFootprintEntry(samplePlan(), "t1", "src/"));
    expect(refusal(addFootprintEntry(doc, "t1", "src/")).code).toBe("duplicate_footprint");
  });

  it("FGV-FR-MZIA: a container has no footprint, and an unknown item is refused", () => {
    expect(refusal(addFootprintEntry(samplePlan(), "f1", "src/")).code).toBe("not_leaf");
    expect(refusal(addFootprintEntry(samplePlan(), "nope", "src/")).code).toBe("unknown_item");
  });

  it("FLO-FR-ZEJU: paths are removed one at a time, and the last removes the field", () => {
    let doc = ok(addFootprintEntry(samplePlan(), "t1", "a/"));
    doc = ok(addFootprintEntry(doc, "t1", "b/"));
    doc = removeFootprintEntry(doc, "t1", "a/");
    expect(findItem(doc, "t1")?.footprint).toEqual(["b/"]);
    doc = removeFootprintEntry(doc, "t1", "b/");
    expect("footprint" in findItem(doc, "t1")!).toBe(false);
    expect(removeFootprintEntry(doc, "t1", "b/")).toBe(doc);
  });
});

describe("due dates (FLO-FR-XAPK, FGV-FR-RRND)", () => {
  it("FLO-FR-XAPK: a milestone stores a calendar date", () => {
    const doc = ok(setDue(samplePlan(), "m1", "2027-03-31"));
    expect(findItem(doc, "m1")?.due).toBe("2027-03-31");
    expect(ok(setDue(doc, "m1", "2027-03-31"))).toBe(doc);
  });

  it("FGV-FR-RRND: only a real date in the form YYYY-MM-DD is accepted", () => {
    for (const good of ["2028-02-29", "2000-02-29", "2027-12-31", "2027-01-01"]) {
      expect(isCalendarDate(good), good).toBe(true);
    }
    for (const bad of [
      "2027-02-29",
      "1900-02-29",
      "2027-13-01",
      "2027-00-10",
      "2027-04-31",
      "2027-04-00",
      "2027-1-01",
      "27-01-01",
      "2027/01/01",
      "2027-01-01T00:00",
      "tomorrow",
    ]) {
      expect(isCalendarDate(bad), bad).toBe(false);
      expect(refusal(setDue(samplePlan(), "m1", bad)).code).toBe("invalid_due_date");
    }
  });

  it("FLO-FR-XAPK: a due date is cleared with null or an empty string", () => {
    const doc = ok(setDue(samplePlan(), "m1", "2027-03-31"));
    expect("due" in findItem(ok(setDue(doc, "m1", null)), "m1")!).toBe(false);
    expect("due" in findItem(ok(setDue(doc, "m1", "")), "m1")!).toBe(false);
    const plain = samplePlan();
    expect(ok(setDue(plain, "m1", null))).toBe(plain);
  });

  it("FGV-FR-MZIA: only a milestone carries a due date, and an unknown item is refused", () => {
    expect(refusal(setDue(samplePlan(), "f1", "2027-03-31")).code).toBe("not_milestone");
    expect(refusal(setDue(samplePlan(), "t1", "2027-03-31")).code).toBe("not_milestone");
    expect(refusal(setDue(samplePlan(), "nope", "2027-03-31")).code).toBe("unknown_item");
  });
});

describe("local only (FLO-FR-TZSU)", () => {
  it("FLO-FR-TZSU: the flag is set to true, and cleared by removing it", () => {
    const on = setLocalOnly(samplePlan(), "f1", true);
    expect(findItem(on, "f1")?.localOnly).toBe(true);
    expect(setLocalOnly(on, "f1", true)).toBe(on);
    const off = setLocalOnly(on, "f1", false);
    expect("localOnly" in findItem(off, "f1")!).toBe(false);
    expect(setLocalOnly(off, "f1", false)).toBe(off);
  });

  it("FLO-FR-TZSU: a `false` read from a file is cleared by an off", () => {
    const doc = plan([item("d1", "deliverable", { localOnly: false })]);
    expect("localOnly" in findItem(setLocalOnly(doc, "d1", false), "d1")!).toBe(false);
  });
});
