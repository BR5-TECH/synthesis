/**
 * The Flow document, version 3: its deserialization, its serialization, and
 * the hierarchy and geometry helpers the canvas reads (FLO-FR-03 to
 * FLO-FR-05, FLO-FR-21, FLO-FR-42, FLO NFR on round-trips, FGV-FR-01,
 * FGV-FR-05, FGV-FR-07).
 */
import { describe, expect, it } from "vitest";

import {
  absolutePosition,
  ancestorsOf,
  CARD_H,
  CARD_W,
  childrenOf,
  containerBounds,
  emptyFlow,
  holdsAnything,
  inLineage,
  innermostContainerAt,
  isAncestor,
  itemBounds,
  leavesUnder,
  parseFlowDocument,
  permitsParent,
  serializeFlowDocument,
  subtreeOf,
  type FlowDocument,
} from "./flowDocument";
import {
  item,
  layoutFor,
  placedAsStored,
  plan,
  sampleLayout,
  samplePlan,
  storedLayout,
  withItems,
} from "../test/flowPlanFixtures";

function parsed(body: string): FlowDocument {
  const r = parseFlowDocument(body);
  if (!r.ok) throw new Error(`expected a plan, got: ${r.error}`);
  return r.doc;
}

function parseError(body: string): string {
  const r = parseFlowDocument(body);
  if (r.ok) throw new Error("expected a parse error");
  return r.error;
}

/**
 * A body that uses every field of the schema, in the order the editor writes
 * it. A byte-exact round-trip of it proves the writer's key order and its
 * formatting.
 */
const FULL_BODY = `{
  "version": 3,
  "name": "Payments v2",
  "description": "Card payments for the EU launch.",
  "references": [
    "docs/payments-hld.md"
  ],
  "items": [
    {
      "id": "m1",
      "kind": "milestone",
      "name": "Beta",
      "description": "Accept **Visa**.",
      "artifactIds": [
        "specifications/core/PAY-payments.md"
      ],
      "due": "2027-03-31"
    },
    {
      "id": "f1",
      "kind": "feature",
      "name": "Card capture",
      "parentId": "m1"
    },
    {
      "id": "t1",
      "kind": "task",
      "name": "Tokenize card numbers",
      "parentId": "f1",
      "draftId": "dr_7f3a",
      "footprint": [
        "src-tauri/src/payments/",
        "specifications/core/PAY-payments.md"
      ]
    },
    {
      "id": "x1",
      "kind": "defect",
      "name": ""
    }
  ],
  "edges": [
    {
      "id": "e1",
      "from": "t1",
      "to": "x1",
      "type": "blocks"
    },
    {
      "id": "e2",
      "from": "x1",
      "to": "f1",
      "type": "relates"
    }
  ]
}
`;

describe("deserialization (FLO-FR-03 / FLO-FR-04 / FLO-FR-05)", () => {
  it("FLO-FR-04, FGV-FR-05: an empty or whitespace body is the empty plan", () => {
    expect(parsed("")).toEqual(emptyFlow());
    expect(parsed("  \n\t ")).toEqual(emptyFlow());
  });

  it("FLO-FR-05, FGV-FR-07: a version 1 or a version 2 document is an unknown version", () => {
    const body = JSON.stringify({ version: 1, nodes: [], edges: [] });
    expect(parseError(body)).toMatch(/unknown Flow version 1 \(expected 3\)/);
    expect(parseError('{"version":2,"items":[],"edges":[]}')).toMatch(
      /unknown Flow version 2 \(expected 3\)/,
    );
  });

  it("FLO-FR-05, FGV-FR-07: a document without a version is an unknown version", () => {
    expect(parseError('{"items":[],"edges":[]}')).toMatch(/unknown Flow version null/);
  });

  it("FLO-FR-05: a body that is not JSON, or not an object, is an error", () => {
    expect(parseError("{ not json")).toMatch(/not valid JSON/);
    expect(parseError("[1, 2]")).toMatch(/expected an object/);
  });

  it("FLO-FR-05, FGV-FR-07: a version given as a string is an unknown version", () => {
    expect(parseError('{"version":"3","items":[],"edges":[]}')).toMatch(/unknown Flow version "3"/);
  });

  it("FLO-FR-05: a field of the wrong type is an error", () => {
    const t = (extra: Record<string, unknown>) =>
      JSON.stringify({
        version: 3,
        items: [{ id: "t1", kind: "task", name: "T", ...extra }],
        edges: [],
      });
    expect(parseError(t({ name: 4 }))).toMatch(/name must be a string/);
    expect(parseError(t({ id: "" }))).toMatch(/item 0 has no id/);
    expect(parseError(t({ footprint: [1] }))).toMatch(/footprint must be a list of strings/);
  });

  it("FLO-FR-21, FGV-FR-01: an item reads no position, size, or local-only flag", () => {
    const doc = parsed(
      JSON.stringify({
        version: 3,
        items: [{ id: "t1", kind: "task", name: "T", position: { x: 1, y: 2 }, localOnly: true }],
      }),
    );
    expect(doc.items[0]).toEqual({ id: "t1", kind: "task", name: "T" });
  });

  it("FGV-FR-01: a sync section is not part of the document", () => {
    const doc = parsed(JSON.stringify({ version: 3, items: [], sync: { tracker: "github" } }));
    expect("sync" in doc).toBe(false);
  });

  it("FLO-FR-05: items or edges that are not lists are an error", () => {
    expect(parseError('{"version":3,"items":{},"edges":[]}')).toMatch(
      /`items` must be an array/,
    );
    expect(parseError('{"version":3,"items":[],"edges":"x"}')).toMatch(
      /`edges` must be an array/,
    );
  });

  it("FLO-FR-05: an item with no kind of the five, or an edge with no type, is an error", () => {
    const badKind = { version: 3, items: [{ id: "a", kind: "node", name: "" }] };
    expect(parseError(JSON.stringify(badKind))).toMatch(/item "a" has no item kind/);
    const badEdge = {
      version: 3,
      items: [],
      edges: [{ id: "e1", from: "a", to: "b", type: "next" }],
    };
    expect(parseError(JSON.stringify(badEdge))).toMatch(/edge "e1" has no type/);
  });

  it("FGV-FR-01: absent items, edges, name and references read as empty", () => {
    expect(parsed('{"version":3}')).toEqual(emptyFlow());
  });

  it("FGV-FR-01: an empty list or an empty description reads as absent", () => {
    const doc = parsed(
      JSON.stringify({
        version: 3,
        name: "P",
        description: "",
        references: [],
        items: [{ id: "t1", kind: "task", name: "T", artifactIds: [], footprint: [] }],
        edges: [],
      }),
    );
    expect(doc.description).toBeUndefined();
    expect(doc.references).toEqual([]);
    expect(doc.items[0]).toEqual({ id: "t1", kind: "task", name: "T" });
  });

  it("FLO-FR-03: every field of the schema is read", () => {
    const doc = parsed(FULL_BODY);
    expect(doc.name).toBe("Payments v2");
    expect(doc.references).toEqual(["docs/payments-hld.md"]);
    expect(doc.items.map((i) => i.kind)).toEqual(["milestone", "feature", "task", "defect"]);
    expect(doc.items[0]).toMatchObject({
      due: "2027-03-31",
      artifactIds: ["specifications/core/PAY-payments.md"],
    });
    expect("parentId" in doc.items[0]).toBe(false);
    expect(doc.items[2]).toMatchObject({
      draftId: "dr_7f3a",
      footprint: ["src-tauri/src/payments/", "specifications/core/PAY-payments.md"],
    });
    expect(doc.edges.map((e) => e.type)).toEqual(["blocks", "relates"]);
    expect("parentId" in doc.items[3]).toBe(false);
  });
});

describe("serialization (FLO-FR-03, FLO NFR on round-trips)", () => {
  it("FLO-FR-03, FLO NFR: a document that is opened and not edited writes back byte for byte", () => {
    expect(serializeFlowDocument(parsed(FULL_BODY))).toBe(FULL_BODY);
  });

  it("FLO NFR: the key order is the schema's, whatever order the object was built in", () => {
    const scrambled = {
      edges: [{ type: "blocks" as const, to: "b", from: "a", id: "e1" }],
      items: [
        {
          due: "2027-01-01",
          parentId: "m0",
          name: "F",
          kind: "feature" as const,
          id: "f1",
        },
      ],
      references: ["hld.md"],
      name: "N",
      version: 3 as const,
    };
    expect(serializeFlowDocument(scrambled)).toBe(
      `${JSON.stringify(
        {
          version: 3,
          name: "N",
          references: ["hld.md"],
          items: [{ id: "f1", kind: "feature", name: "F", parentId: "m0", due: "2027-01-01" }],
          edges: [{ id: "e1", from: "a", to: "b", type: "blocks" }],
        },
        null,
        2,
      )}\n`,
    );
  });

  it("FLO-FR-25, FLO-FR-YDWT: the name, items and edges are always written; empty references are not", () => {
    const body = serializeFlowDocument(emptyFlow());
    expect(body).toBe('{\n  "version": 3,\n  "name": "",\n  "items": [],\n  "edges": []\n}\n');
  });

  it("FLO-FR-25: an empty description and empty lists are left out", () => {
    const doc = plan([item("t1", "task", { artifactIds: [], footprint: [] })], [], {
      description: "",
    });
    const out = JSON.parse(serializeFlowDocument(doc));
    expect(out.description).toBeUndefined();
    expect(Object.keys(out.items[0])).toEqual(["id", "kind", "name"]);
  });

  it("FLO NFR: the written body ends in one newline and uses two-space indent", () => {
    const body = serializeFlowDocument(emptyFlow());
    expect(body.endsWith("}\n")).toBe(true);
    expect(body.split("\n")[1]).toBe('  "version": 3,');
  });
});

describe("hierarchy helpers (FLO-FR-42, FGV-FR-YHFX, FGV-FR-KBLT)", () => {
  const doc = samplePlan();

  it("FLO-FR-42: ancestorsOf gives the chain of containers, the nearest first", () => {
    expect(ancestorsOf(doc, "t3")).toEqual(["f2", "ep1", "m1"]);
    expect(ancestorsOf(doc, "m1")).toEqual([]);
    expect(ancestorsOf(doc, "nope")).toEqual([]);
  });

  it("FGV-FR-13: a chain of parents that closes on itself ends", () => {
    const loop = plan([
      item("f1", "feature", { parentId: "f2" }),
      item("f2", "feature", { parentId: "f1" }),
    ]);
    expect(ancestorsOf(loop, "f1")).toEqual(["f2"]);
    expect(subtreeOf(loop, "f1")).toEqual(new Set(["f1", "f2"]));
  });

  it("FLO-FR-45: subtreeOf holds the item and everything under it", () => {
    expect(subtreeOf(doc, "ep1")).toEqual(new Set(["ep1", "f2", "t3"]));
    expect(subtreeOf(doc, "t1")).toEqual(new Set(["t1"]));
    expect(subtreeOf(doc, "nope").size).toBe(0);
  });

  it("FGV-FR-PMTY: leavesUnder gives the tasks and defects in document order", () => {
    expect(leavesUnder(doc, "m1")).toEqual(["t1", "t2", "t3", "x1"]);
    expect(leavesUnder(doc, "t2")).toEqual(["t2"]);
    const empty = withItems(doc, item("f9", "feature", { parentId: "m1" }));
    expect(leavesUnder(empty, "f9")).toEqual([]);
  });

  it("FLO-FR-45: childrenOf and holdsAnything read the direct members", () => {
    expect(childrenOf(doc, "m1").map((i) => i.id)).toEqual(["f1", "ep1", "x1"]);
    expect(holdsAnything(doc, "f1")).toBe(true);
    expect(holdsAnything(doc, "t1")).toBe(false);
  });

  it("FGV-FR-KBLT: isAncestor and inLineage see ancestors at any depth, in both orders", () => {
    expect(isAncestor(doc, "m1", "t3")).toBe(true);
    expect(isAncestor(doc, "t3", "m1")).toBe(false);
    expect(inLineage(doc, "t3", "ep1")).toBe(true);
    expect(inLineage(doc, "ep1", "t3")).toBe(true);
    expect(inLineage(doc, "t1", "t3")).toBe(false);
    expect(inLineage(doc, "f1", "ep1")).toBe(false);
  });

  it("FGV-FR-YHFX: a milestone sits at the top level, every other kind at the top level or in a higher rank", () => {
    expect(permitsParent("milestone", null)).toBe(true);
    expect(permitsParent("milestone", "milestone")).toBe(false);
    expect(permitsParent("milestone", "epic")).toBe(false);
    expect(permitsParent("epic", null)).toBe(true);
    expect(permitsParent("epic", "milestone")).toBe(true);
    expect(permitsParent("epic", "epic")).toBe(false);
    expect(permitsParent("epic", "feature")).toBe(false);
    expect(permitsParent("feature", null)).toBe(true);
    expect(permitsParent("feature", "milestone")).toBe(true);
    expect(permitsParent("feature", "epic")).toBe(true);
    expect(permitsParent("feature", "feature")).toBe(false);
    for (const leaf of ["task", "defect"] as const) {
      expect(permitsParent(leaf, null)).toBe(true);
      expect(permitsParent(leaf, "milestone")).toBe(true);
      expect(permitsParent(leaf, "epic")).toBe(true);
      expect(permitsParent(leaf, "feature")).toBe(true);
      expect(permitsParent(leaf, "task")).toBe(false);
      expect(permitsParent(leaf, "defect")).toBe(false);
    }
  });
});

describe("geometry helpers (FLO-FR-21, FLO-FR-42, FLO-FR-44)", () => {
  const doc = samplePlan();
  const layout = placedAsStored(sampleLayout());

  it("FLO-FR-21: absolutePosition adds the position of every container", () => {
    expect(absolutePosition(doc, layout, "t3")).toEqual({ x: 520, y: 240 });
    expect(absolutePosition(doc, layout, "m1")).toEqual({ x: 20, y: 60 });
    expect(absolutePosition(doc, layout, "nope")).toEqual({ x: 0, y: 0 });
  });

  it("FLO-FR-44: containerBounds gives a container's canvas rectangle and null for a leaf", () => {
    expect(containerBounds(doc, layout, "f2")).toEqual({ x: 500, y: 180, width: 400, height: 220 });
    expect(containerBounds(doc, layout, "t1")).toBeNull();
    expect(containerBounds(doc, layout, "nope")).toBeNull();
  });

  it("FLO-FR-44: itemBounds gives a leaf the card extent", () => {
    expect(itemBounds(doc, layout, "t1")).toEqual({ x: 60, y: 180, width: CARD_W, height: CARD_H });
  });

  it("FLO-FR-42: innermostContainerAt gives the deepest container that holds the point", () => {
    expect(innermostContainerAt(doc, layout, { x: 530, y: 250 })).toBe("f2");
    expect(innermostContainerAt(doc, layout, { x: 45, y: 125 })).toBe("f1");
    expect(innermostContainerAt(doc, layout, { x: 25, y: 65 })).toBe("m1");
    expect(innermostContainerAt(doc, layout, { x: 5, y: 5 })).toBeUndefined();
    expect(innermostContainerAt(doc, layout, { x: -10, y: -10 })).toBeUndefined();
  });

  it("FLO-FR-42: a point on the border of a container is inside it", () => {
    expect(innermostContainerAt(doc, layout, { x: 500, y: 180 })).toBe("f2");
    expect(innermostContainerAt(doc, layout, { x: 1020, y: 760 })).toBe("m1");
    expect(innermostContainerAt(doc, layout, { x: 1021, y: 760 })).toBeUndefined();
  });

  it("FLO-FR-42: an excluded container and every container under it are not the answer", () => {
    expect(innermostContainerAt(doc, layout, { x: 530, y: 250 }, new Set(["f2"]))).toBe("ep1");
    expect(innermostContainerAt(doc, layout, { x: 530, y: 250 }, new Set(["ep1"]))).toBe("m1");
  });

  it("FLO-FR-42: where two sibling containers overlap, the first in document order wins", () => {
    const pair = plan([item("m1", "milestone"), item("m2", "milestone")]);
    const placed = layoutFor(
      pair,
      storedLayout({
        m1: { x: 0, y: 0, width: 100, height: 100 },
        m2: { x: 50, y: 50, width: 100, height: 100 },
      }),
    );
    expect(innermostContainerAt(pair, placed, { x: 75, y: 75 })).toBe("m1");
  });

  it("FLO-FR-42: a leaf card is never the container at a point", () => {
    // t1's card covers (60,180)–(230,276); f1 holds it.
    expect(innermostContainerAt(doc, layout, { x: 100, y: 200 })).toBe("f1");
  });
});
