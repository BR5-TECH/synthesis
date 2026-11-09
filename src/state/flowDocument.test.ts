/**
 * The Flow document, version 2: its deserialization, its serialization, and
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
import { item, plan, samplePlan, withItems } from "../test/flowPlanFixtures";

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
  "version": 2,
  "name": "Payments v2",
  "description": "Card payments for the EU launch.",
  "references": [
    "docs/payments-hld.md"
  ],
  "items": [
    {
      "id": "d1",
      "kind": "deliverable",
      "name": "Card payments",
      "description": "Accept **Visa**.",
      "artifactIds": [
        "specifications/core/PAY-payments.md"
      ],
      "position": {
        "x": 40,
        "y": 40
      },
      "size": {
        "width": 1200,
        "height": 700
      }
    },
    {
      "id": "m1",
      "kind": "milestone",
      "name": "Beta",
      "parentId": "d1",
      "localOnly": true,
      "due": "2027-03-31",
      "position": {
        "x": 20,
        "y": 60
      },
      "size": {
        "width": 900,
        "height": 520
      }
    },
    {
      "id": "f1",
      "kind": "feature",
      "name": "Card capture",
      "parentId": "m1",
      "position": {
        "x": 20,
        "y": 60
      },
      "size": {
        "width": 420,
        "height": 300
      }
    },
    {
      "id": "t1",
      "kind": "task",
      "name": "Tokenize card numbers",
      "parentId": "f1",
      "localOnly": false,
      "draftId": "dr_7f3a",
      "footprint": [
        "src-tauri/src/payments/",
        "specifications/core/PAY-payments.md"
      ],
      "position": {
        "x": 20.5,
        "y": -60
      }
    },
    {
      "id": "x1",
      "kind": "defect",
      "name": "",
      "parentId": "m1",
      "position": {
        "x": 460,
        "y": 60
      }
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
  ],
  "sync": {
    "tracker": "github",
    "links": [
      {
        "itemId": "f1",
        "ref": {
          "kind": "issue",
          "host": "github.com",
          "owner": "acme",
          "repo": "payments",
          "number": 42,
          "url": "https://github.com/acme/payments/issues/42"
        },
        "base": {
          "name": "Card capture",
          "description": "",
          "state": "open",
          "type": "Feature",
          "parent": {
            "kind": "issue",
            "host": "github.com",
            "owner": "acme",
            "repo": "payments",
            "number": 40,
            "url": "https://github.com/acme/payments/issues/40"
          },
          "milestone": null,
          "blockedBy": []
        }
      },
      {
        "itemId": "m1",
        "ref": {
          "kind": "milestone",
          "host": "github.com",
          "owner": "acme",
          "repo": "payments",
          "number": 3,
          "url": "https://github.com/acme/payments/milestone/3"
        },
        "base": {
          "name": "Beta",
          "description": "First cut.",
          "state": "closed",
          "due": null
        }
      }
    ]
  }
}
`;

describe("deserialization (FLO-FR-03 / FLO-FR-04 / FLO-FR-05)", () => {
  it("FLO-FR-04, FGV-FR-05: an empty or whitespace body is the empty plan", () => {
    expect(parsed("")).toEqual(emptyFlow());
    expect(parsed("  \n\t ")).toEqual(emptyFlow());
  });

  it("FLO-FR-05, FGV-FR-07: a version 1 document is an unknown version", () => {
    const body = JSON.stringify({ version: 1, nodes: [], edges: [] });
    expect(parseError(body)).toMatch(/unknown Flow version 1 \(expected 2\)/);
  });

  it("FLO-FR-05, FGV-FR-07: a document without a version is an unknown version", () => {
    expect(parseError('{"items":[],"edges":[]}')).toMatch(/unknown Flow version null/);
  });

  it("FLO-FR-05: a body that is not JSON, or not an object, is an error", () => {
    expect(parseError("{ not json")).toMatch(/not valid JSON/);
    expect(parseError("[1, 2]")).toMatch(/expected an object/);
  });

  it("FLO-FR-05, FGV-FR-07: a version given as a string is an unknown version", () => {
    expect(parseError('{"version":"2","items":[],"edges":[]}')).toMatch(/unknown Flow version "2"/);
  });

  it("FLO-FR-05: a field of the wrong type is an error", () => {
    const t = (extra: Record<string, unknown>) =>
      JSON.stringify({
        version: 2,
        items: [{ id: "t1", kind: "task", name: "T", position: { x: 0, y: 0 }, ...extra }],
        edges: [],
      });
    expect(parseError(t({ position: { x: 0 } }))).toMatch(/position has no numeric y/);
    expect(parseError(t({ localOnly: "yes" }))).toMatch(/localOnly must be a boolean/);
    expect(parseError(t({ id: "" }))).toMatch(/item 0 has no id/);
    expect(parseError(t({ footprint: [1] }))).toMatch(/footprint must be a list of strings/);
  });

  it("FLO-FR-05: a sync section of the wrong shape is an error", () => {
    const link = {
      itemId: "t1",
      ref: { kind: "issue", host: "github.com", owner: "a", repo: "r", number: 1, url: "u" },
      base: { name: "", description: "", state: "open" },
    };
    const s = (sync: unknown) => JSON.stringify({ version: 2, items: [], edges: [], sync });
    expect(parseError(s({ tracker: "gitlab", links: [] }))).toMatch(/must be github/);
    expect(parseError(s({ tracker: "github", links: {} }))).toMatch(/`sync.links` must be an array/);
    expect(parseError(s({ tracker: "github", links: [{ ...link, ref: { ...link.ref, number: 4.5 } }] }))).toMatch(
      /ref has no number/,
    );
    expect(parseError(s({ tracker: "github", links: [{ ...link, base: { ...link.base, state: "merged" } }] }))).toMatch(
      /base has no state/,
    );
    expect(parsed(s({ tracker: "github", links: [link] })).sync?.links).toHaveLength(1);
  });

  it("FLO-FR-05: items or edges that are not lists are an error", () => {
    expect(parseError('{"version":2,"items":{},"edges":[]}')).toMatch(
      /`items` must be an array/,
    );
    expect(parseError('{"version":2,"items":[],"edges":"x"}')).toMatch(
      /`edges` must be an array/,
    );
  });

  it("FLO-FR-05: an item with no kind of the six, or an edge with no type, is an error", () => {
    const badKind = { version: 2, items: [{ id: "a", kind: "node", name: "", position: { x: 0, y: 0 } }] };
    expect(parseError(JSON.stringify(badKind))).toMatch(/item "a" has no item kind/);
    const badEdge = {
      version: 2,
      items: [],
      edges: [{ id: "e1", from: "a", to: "b", type: "next" }],
    };
    expect(parseError(JSON.stringify(badEdge))).toMatch(/edge "e1" has no type/);
  });

  it("FGV-FR-01: absent items, edges, name and references read as empty", () => {
    expect(parsed('{"version":2}')).toEqual(emptyFlow());
  });

  it("FGV-FR-01: an empty list or an empty description reads as absent", () => {
    const doc = parsed(
      JSON.stringify({
        version: 2,
        name: "P",
        description: "",
        references: [],
        items: [
          { id: "t1", kind: "task", name: "T", artifactIds: [], footprint: [], position: { x: 0, y: 0 } },
        ],
        edges: [],
      }),
    );
    expect(doc.description).toBeUndefined();
    expect(doc.references).toEqual([]);
    expect(doc.items[0]).toEqual({ id: "t1", kind: "task", name: "T", position: { x: 0, y: 0 } });
  });

  it("FLO-FR-03: every field of the schema is read", () => {
    const doc = parsed(FULL_BODY);
    expect(doc.name).toBe("Payments v2");
    expect(doc.references).toEqual(["docs/payments-hld.md"]);
    expect(doc.items.map((i) => i.kind)).toEqual([
      "deliverable",
      "milestone",
      "feature",
      "task",
      "defect",
    ]);
    expect(doc.items[1]).toMatchObject({ due: "2027-03-31", localOnly: true });
    expect(doc.items[3]).toMatchObject({
      draftId: "dr_7f3a",
      footprint: ["src-tauri/src/payments/", "specifications/core/PAY-payments.md"],
    });
    expect(doc.edges.map((e) => e.type)).toEqual(["blocks", "relates"]);
    expect(doc.sync?.links).toHaveLength(2);
    expect(doc.sync?.links[1].base).toEqual({
      name: "Beta",
      description: "First cut.",
      state: "closed",
      due: null,
    });
  });
});

describe("serialization (FLO-FR-03, FLO NFR on round-trips)", () => {
  it("FLO-FR-03, FLO NFR: a document that is opened and not edited writes back byte for byte", () => {
    expect(serializeFlowDocument(parsed(FULL_BODY))).toBe(FULL_BODY);
  });

  it("FLO NFR: the key order is the schema's, whatever order the object was built in", () => {
    const scrambled = {
      sync: { links: [], tracker: "github" as const },
      edges: [{ type: "blocks" as const, to: "b", from: "a", id: "e1" }],
      items: [
        {
          size: { height: 220, width: 400 },
          position: { y: 2, x: 1 },
          name: "F",
          kind: "feature" as const,
          id: "f1",
        },
      ],
      references: ["hld.md"],
      name: "N",
      version: 2 as const,
    };
    expect(serializeFlowDocument(scrambled)).toBe(
      `${JSON.stringify(
        {
          version: 2,
          name: "N",
          references: ["hld.md"],
          items: [
            {
              id: "f1",
              kind: "feature",
              name: "F",
              position: { x: 1, y: 2 },
              size: { width: 400, height: 220 },
            },
          ],
          edges: [{ id: "e1", from: "a", to: "b", type: "blocks" }],
          sync: { tracker: "github", links: [] },
        },
        null,
        2,
      )}\n`,
    );
  });

  it("FLO-FR-25, FLO-FR-YDWT: the name, items and edges are always written; empty references and absent sync are not", () => {
    const body = serializeFlowDocument(emptyFlow());
    expect(body).toBe('{\n  "version": 2,\n  "name": "",\n  "items": [],\n  "edges": []\n}\n');
  });

  it("FLO-FR-25: an empty description and empty lists are left out", () => {
    const doc = plan([item("t1", "task", { artifactIds: [], footprint: [] })], [], {
      description: "",
    });
    const out = JSON.parse(serializeFlowDocument(doc));
    expect(out.description).toBeUndefined();
    expect(Object.keys(out.items[0])).toEqual(["id", "kind", "name", "position"]);
  });

  it("FLO NFR: the written body ends in one newline and uses two-space indent", () => {
    const body = serializeFlowDocument(emptyFlow());
    expect(body.endsWith("}\n")).toBe(true);
    expect(body.split("\n")[1]).toBe('  "version": 2,');
  });
});

describe("hierarchy helpers (FLO-FR-42, FGV-FR-YHFX, FGV-FR-KBLT)", () => {
  const doc = samplePlan();

  it("FLO-FR-42: ancestorsOf gives the chain of containers, the nearest first", () => {
    expect(ancestorsOf(doc, "t3")).toEqual(["f2", "ep1", "m1", "d1"]);
    expect(ancestorsOf(doc, "d1")).toEqual([]);
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
    expect(isAncestor(doc, "d1", "t3")).toBe(true);
    expect(isAncestor(doc, "t3", "d1")).toBe(false);
    expect(inLineage(doc, "t3", "ep1")).toBe(true);
    expect(inLineage(doc, "ep1", "t3")).toBe(true);
    expect(inLineage(doc, "t1", "t3")).toBe(false);
    expect(inLineage(doc, "f1", "ep1")).toBe(false);
  });

  it("FGV-FR-YHFX: permitsParent holds the containers each kind may sit in", () => {
    expect(permitsParent("deliverable", null)).toBe(true);
    expect(permitsParent("milestone", "deliverable")).toBe(true);
    expect(permitsParent("milestone", null)).toBe(false);
    expect(permitsParent("epic", "milestone")).toBe(true);
    expect(permitsParent("epic", "deliverable")).toBe(false);
    expect(permitsParent("feature", "epic")).toBe(true);
    expect(permitsParent("feature", "feature")).toBe(false);
    expect(permitsParent("task", "feature")).toBe(true);
    expect(permitsParent("task", "milestone")).toBe(false);
    expect(permitsParent("defect", "milestone")).toBe(true);
    expect(permitsParent("defect", "epic")).toBe(true);
    expect(permitsParent("defect", "task")).toBe(false);
  });
});

describe("geometry helpers (FLO-FR-21, FLO-FR-42, FLO-FR-44)", () => {
  const doc = samplePlan();

  it("FLO-FR-21: absolutePosition adds the position of every container", () => {
    expect(absolutePosition(doc, "t3")).toEqual({ x: 520, y: 240 });
    expect(absolutePosition(doc, "d1")).toEqual({ x: 0, y: 0 });
    expect(absolutePosition(doc, "nope")).toEqual({ x: 0, y: 0 });
  });

  it("FLO-FR-44: containerBounds gives a container's canvas rectangle and null for a leaf", () => {
    expect(containerBounds(doc, "f2")).toEqual({ x: 500, y: 180, width: 400, height: 220 });
    expect(containerBounds(doc, "t1")).toBeNull();
    expect(containerBounds(doc, "nope")).toBeNull();
  });

  it("FLO-FR-44: itemBounds gives a leaf the card extent", () => {
    expect(itemBounds(doc, "t1")).toEqual({ x: 60, y: 180, width: CARD_W, height: CARD_H });
  });

  it("FLO-FR-42: innermostContainerAt gives the deepest container that holds the point", () => {
    expect(innermostContainerAt(doc, { x: 530, y: 250 })).toBe("f2");
    expect(innermostContainerAt(doc, { x: 45, y: 125 })).toBe("f1");
    expect(innermostContainerAt(doc, { x: 5, y: 5 })).toBe("d1");
    expect(innermostContainerAt(doc, { x: -10, y: -10 })).toBeUndefined();
  });

  it("FLO-FR-42: a point on the border of a container is inside it", () => {
    expect(innermostContainerAt(doc, { x: 500, y: 180 })).toBe("f2");
    expect(innermostContainerAt(doc, { x: 1200, y: 800 })).toBe("d1");
    expect(innermostContainerAt(doc, { x: 1201, y: 800 })).toBeUndefined();
  });

  it("FLO-FR-42: an excluded container and every container under it are not the answer", () => {
    expect(innermostContainerAt(doc, { x: 530, y: 250 }, new Set(["f2"]))).toBe("ep1");
    expect(innermostContainerAt(doc, { x: 530, y: 250 }, new Set(["ep1"]))).toBe("m1");
  });

  it("FLO-FR-42: where two sibling containers overlap, the first in document order wins", () => {
    const doc = plan([
      item("d1", "deliverable", { size: { width: 100, height: 100 } }),
      item("d2", "deliverable", { position: { x: 50, y: 50 }, size: { width: 100, height: 100 } }),
    ]);
    expect(innermostContainerAt(doc, { x: 75, y: 75 })).toBe("d1");
  });

    it("FLO-FR-42: a leaf card is never the container at a point", () => {
    // t1's card covers (60,180)–(230,276); f1 holds it.
    expect(innermostContainerAt(doc, { x: 100, y: 200 })).toBe("f1");
  });
});
