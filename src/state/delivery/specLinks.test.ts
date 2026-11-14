/**
 * The derivation of the links between delivery items and spec nodes
 * (`../../../specifications/ui/SDL-specification-delivery-links.md`).
 */
import { describe, expect, it } from "vitest";

import { item, plan } from "../../test/flowPlanFixtures";
import { node, openspecIndex, smallIndex, spec } from "../../test/specMapFixtures";
import { buildTree } from "../specMap/tree";
import {
  deriveSpecLinks,
  footprintCovers,
  holdsUnplanned,
  isUnplanned,
  itemsLinking,
  NO_LINKS,
  planWorkFiles,
} from "./specLinks";

const tree = () => buildTree(smallIndex());
const A1 = "specifications/ui/A1-a1.md";
const A2 = "specifications/core/A2-a2.md";
const C1 = "specifications/ui/C1-c1.md";

describe("the spec links of an item (SDL-FR-TWOV, SDL-FR-OCPB)", () => {
  it("SDL-FR-TWOV, SDL-FR-OCPB: a reference links the spec node whose project path is the reference's artifact id", () => {
    const doc = plan([item("t1", "task", { artifactIds: [A1, "docs/hld.md"] })]);
    expect(deriveSpecLinks(doc, tree()).byItem.get("t1")).toEqual(["A1"]);
  });

  it("SDL-FR-OCPB: a path relative to specifications/ links nothing in the demo index", () => {
    const doc = plan([item("t1", "task", { artifactIds: ["ui/A1-a1.md"] })]);
    expect(deriveSpecLinks(doc, tree()).byItem.has("t1")).toBe(false);
  });

  it("SDL-FR-TWOV: a footprint entry that covers the spec file links it; a folder covers every file below it", () => {
    const doc = plan([
      item("t1", "task", { footprint: [A2] }),
      item("t2", "task", { footprint: ["specifications/ui/"] }),
      item("t3", "task", { footprint: ["specifications/u"] }),
    ]);
    const links = deriveSpecLinks(doc, tree());
    expect(links.byItem.get("t1")).toEqual(["A2"]);
    expect(links.byItem.get("t2")).toEqual(["A1", "A3", "C1", "C2"]);
    expect(links.byItem.has("t3")).toBe(false);
  });

  it("SDL-FR-OCPB: in an openspec index the spec file is the project path itself, and each requirement in it links", () => {
    const index = openspecIndex();
    const t = buildTree(index);
    const first = [...t.specs.values()][0];
    const doc = plan([item("t1", "task", { artifactIds: [first.path] })]);
    const linked = deriveSpecLinks(doc, t).byItem.get("t1") ?? [];
    const samePath = [...t.specs.values()].filter((s) => s.path === first.path).map((s) => s.code);
    expect(linked).toEqual(samePath);
  });

  it("FOA-FR-XRBA: paths compare segment by segment and are case-sensitive", () => {
    expect(footprintCovers("specifications/ui/", "specifications/ui/A1-a1.md")).toBe(true);
    expect(footprintCovers("specifications/ui/", "specifications/uix/A1.md")).toBe(false);
    expect(footprintCovers("specifications/UI/", "specifications/ui/A1-a1.md")).toBe(false);
    expect(footprintCovers(A1, A1)).toBe(true);
    expect(footprintCovers(A1, `${A1}x`)).toBe(false);
  });
});

describe("a container's links (SDL-FR-QUUA)", () => {
  it("SDL-FR-QUUA: a container links its own spec nodes and those of every item under it, once each, in index order", () => {
    const doc = plan([
      item("m1", "milestone", { artifactIds: [C1] }),
      item("f1", "feature", { parentId: "m1" }),
      item("t1", "task", { parentId: "f1", artifactIds: [A2] }),
      item("t2", "task", { parentId: "f1", artifactIds: [A1, C1] }),
    ]);
    const links = deriveSpecLinks(doc, tree());
    expect(links.byItem.get("f1")).toEqual(["A1", "A2", "C1"]);
    expect(links.byItem.get("m1")).toEqual(["A1", "A2", "C1"]);
    expect(links.bySpec.get("C1")).toEqual(["m1", "f1", "t2"]);
  });
});

describe("the items of a subject (SDL-FR-VAGU)", () => {
  it("SDL-FR-VAGU: a spec node lists each item that links it, and an index node each item that links a spec node under it, once each, in plan order", () => {
    const doc = plan([
      item("t2", "task", { artifactIds: [A2] }),
      item("t1", "task", { artifactIds: [A1] }),
      item("t3", "task", { artifactIds: [A1, A2] }),
    ]);
    const t = tree();
    const links = deriveSpecLinks(doc, t);
    expect(itemsLinking(links, t, doc, { kind: "spec", code: "A1" })).toEqual(["t1", "t3"]);
    expect(itemsLinking(links, t, doc, { kind: "index", id: "g1" })).toEqual(["t2", "t1", "t3"]);
    expect(itemsLinking(links, t, doc, { kind: "index", id: "d2" })).toEqual([]);
    expect(itemsLinking(links, t, doc, { kind: "change", key: "x" })).toEqual([]);
  });
});

describe("Plan work (SDL-FR-CFHH)", () => {
  it("SDL-FR-CFHH: a spec node gives its file, and a leaf group the file of each spec node, once each", () => {
    const t = tree();
    expect(planWorkFiles(t, { kind: "spec", code: "A1" })).toEqual([A1]);
    expect(planWorkFiles(t, { kind: "index", id: "g1" })).toEqual([A1, A2]);
    expect(planWorkFiles(t, { kind: "spec", code: "NONE" })).toEqual([]);
  });

  it("SDL-FR-CFHH: two requirements of one file give one reference to the file", () => {
    const t = buildTree(openspecIndex());
    expect(planWorkFiles(t, { kind: "index", id: "capability:auth" })).toEqual(["openspec/specs/auth/spec.md"]);
    expect(planWorkFiles(t, { kind: "spec", code: "auth#2" })).toEqual(["openspec/specs/auth/spec.md"]);
  });
});

describe("the unplanned filter (SDL-FR-KXCY)", () => {
  it("SDL-FR-KXCY: a gap or drafted spec node that no item links is unplanned; a linked one or a built one is not", () => {
    const doc = plan([item("t1", "task", { artifactIds: [A2] })]);
    const t = tree();
    const links = deriveSpecLinks(doc, t);
    expect(isUnplanned(links, "A2", "gap")).toBe(false);
    expect(isUnplanned(links, "B1", "drafted")).toBe(true);
    expect(isUnplanned(links, "A1", "verified")).toBe(false);
    expect(holdsUnplanned(links, t, "f2")).toBe(true);
    expect(holdsUnplanned(links, t, "g1")).toBe(false);
    expect(holdsUnplanned(links, t, "d2")).toBe(false);
  });
});

describe("a new derivation (SDL-FR-NXXQ)", () => {
  it("SDL-FR-NXXQ: no plan, no index, or an empty plan derives no link", () => {
    expect(deriveSpecLinks(null, tree())).toBe(NO_LINKS);
    expect(deriveSpecLinks(plan([item("t1", "task", { artifactIds: [A1] })]), null)).toBe(NO_LINKS);
    expect(deriveSpecLinks(plan([]), tree())).toBe(NO_LINKS);
  });

  it("SDL-FR-NXXQ: a changed reference or index derives new links", () => {
    const before = deriveSpecLinks(plan([item("t1", "task", { artifactIds: [A1] })]), tree());
    const after = deriveSpecLinks(plan([item("t1", "task", { artifactIds: [C1] })]), tree());
    expect(before.byItem.get("t1")).toEqual(["A1"]);
    expect(after.byItem.get("t1")).toEqual(["C1"]);
    const other = buildTree({
      ...smallIndex(),
      roots: [node("d9", [node("f9", [node("g9", [spec("A1", "gap")])])])],
    });
    expect(deriveSpecLinks(plan([item("t1", "task", { artifactIds: [C1] })]), other).byItem.size).toBe(0);
  });
});
