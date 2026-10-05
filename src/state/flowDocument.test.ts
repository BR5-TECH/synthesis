import { describe, expect, it } from "vitest";

import {
  DEFAULT_LOOP_NAME,
  DEFAULT_LOOP_SIZE,
  MEMBER_NODE_W,
  absolutePosition,
  addLoop,
  addNode,
  ancestorsOf,
  edgesSeveredByMove,
  innermostLoopAt,
  loopHolds,
  minLoopSize,
  moveElement,
  removeLoop,
  renameLoop,
  resizeLoop,
  setLoopMaxPasses,
  subtreeOf,
  addElementArtifact,
  connect,
  emptyFlow,
  moveNode,
  parseFlowDocument,
  removeEdge,
  removeNode,
  removeElementArtifact,
  renameEdge,
  renameNode,
  serializeFlowDocument,
  setFlowDescription,
  setFlowName,
  setNodePrompt,
  type FlowDocument,
} from "./flowDocument";

/** The document the FLO contract boundary spells out, as a fixture. */
const SAMPLE = `{
  "version": 1,
  "nodes": [
    {
      "id": "n1",
      "name": "Collect context",
      "artifactIds": ["prompts/gather-repo.md", ".claude/skills/review/SKILL.md"],
      "prompt": "Read the repo and list every changed file.",
      "position": { "x": 120, "y": 80 }
    },
    {
      "id": "n2",
      "name": "Draft review",
      "position": { "x": 420, "y": 80 }
    }
  ],
  "edges": [
    { "id": "e1", "from": "n1", "to": "n2", "label": "ok" }
  ]
}`;

/** Parse, asserting success — the shape most tests here start from. */
function parsed(body: string): FlowDocument {
  const result = parseFlowDocument(body);
  if (!result.ok) throw new Error(`expected a document, got: ${result.error}`);
  return result.doc;
}

/** The refusal a failed parse reports. */
function refused(body: string): string {
  const result = parseFlowDocument(body);
  if (result.ok) throw new Error("expected the parse to fail");
  return result.error;
}

/** Why `connect` declined, asserting that it did. */
function refusedConnect(result: ReturnType<typeof connect>): string {
  if (result.ok) throw new Error("expected the connection to be refused");
  return result.refused;
}

/** A graph of `n1 -> n2 -> n3`, the shape the edge rules are exercised on. */
function chain(): FlowDocument {
  let doc = emptyFlow();
  doc = addNode(doc, { x: 0, y: 0 });
  doc = addNode(doc, { x: 100, y: 0 });
  doc = addNode(doc, { x: 200, y: 0 });
  const a = connect(doc, "n1", "n2");
  if (!a.ok) throw new Error("n1->n2 refused");
  const b = connect(a.doc, "n2", "n3");
  if (!b.ok) throw new Error("n2->n3 refused");
  return b.doc;
}


/**
 * Reading, writing and editing a flow document.
 *
 * One part of the group headed by `./flowDocument.loops.test.ts`, which carries
 * the loop rules; both are written against the same sample document above.
 */
describe("deserialization (FLO-FR-03 / FLO-FR-04 / FLO-FR-05)", () => {
  it("reads the document shape the contract boundary defines", () => {
    const doc = parsed(SAMPLE);
    expect(doc.version).toBe(1);
    expect(doc.nodes).toHaveLength(2);
    expect(doc.nodes[0]).toEqual({
      id: "n1",
      name: "Collect context",
      artifactIds: [
        "prompts/gather-repo.md",
        ".claude/skills/review/SKILL.md",
      ],
      prompt: "Read the repo and list every changed file.",
      position: { x: 120, y: 80 },
    });
    // FLO-FR-14: neither optional field is required for a node to be valid.
    expect(doc.nodes[1].artifactIds).toBeUndefined();
    expect(doc.nodes[1].prompt).toBeUndefined();
    expect(doc.edges).toEqual([{ id: "e1", from: "n1", to: "n2", label: "ok" }]);
  });

  // FLO-FR-04: the state a Flow created through New Artifact, New File, or an
  // external `touch` is in — it must open ready to edit, not in an error state.
  it("reads an empty or whitespace-only body as an empty graph", () => {
    const empty = { version: 1, name: "", loops: [], nodes: [], edges: [] };
    expect(parsed("")).toEqual(empty);
    expect(parsed("   \n\t \n")).toEqual(empty);
  });

  it("refuses a body that is not JSON at all (FLO-FR-05)", () => {
    expect(refused("{ not json")).toMatch(/not valid JSON/);
    expect(refused("# a markdown heading")).toMatch(/not valid JSON/);
  });

  it("refuses valid JSON that is not a Flow document (FLO-FR-05)", () => {
    expect(refused("[]")).toMatch(/expected an object/);
    expect(refused('"a string"')).toMatch(/expected an object/);
    expect(refused("null")).toMatch(/expected an object/);
    expect(refused('{"version":1,"nodes":{},"edges":[]}')).toMatch(/`nodes` must be an array/);
    expect(refused('{"version":1,"nodes":[],"edges":null}')).toMatch(/`edges` must be an array/);
  });

  it("refuses a version it cannot read rather than reinterpreting it", () => {
    // A document written by a build that changed the shape must not be opened
    // as if it were this one — the write-back would replace it with something
    // it never said.
    expect(refused('{"version":2,"nodes":[],"edges":[]}')).toMatch(/unsupported Flow version 2/);
    expect(refused('{"nodes":[],"edges":[]}')).toMatch(/unsupported Flow version/);
  });

  it("refuses an entry that is not an object at all", () => {
    expect(refused('{"version":1,"nodes":[null],"edges":[]}')).toMatch(
      /node 0 is not an object/,
    );
    expect(refused('{"version":1,"nodes":[[]],"edges":[]}')).toMatch(
      /node 0 is not an object/,
    );
    const one = '{"id":"n1","name":"x","position":{"x":0,"y":0}}';
    expect(refused(`{"version":1,"nodes":[${one}],"edges":[7]}`)).toMatch(
      /edge 0 is not an object/,
    );
  });

  it("refuses an edge missing or mistyping the fields FLO-FR-15 requires", () => {
    const nodes =
      '{"id":"n1","name":"x","position":{"x":0,"y":0}},' +
      '{"id":"n2","name":"y","position":{"x":0,"y":0}}';
    const edge = (e: string) =>
      `{"version":1,"nodes":[${nodes}],"edges":[${e}]}`;
    expect(refused(edge('{"from":"n1","to":"n2"}'))).toMatch(/edge 0 has no id/);
    expect(refused(edge('{"id":"e1","to":"n2"}'))).toMatch(/has no endpoints/);
    expect(refused(edge('{"id":"e1","from":1,"to":"n2"}'))).toMatch(
      /has no endpoints/,
    );
    expect(
      refused(edge('{"id":"e1","from":"n1","to":"n2","label":false}')),
    ).toMatch(/label must be a string/);
  });

  // Reachable from a hand-edited file: `1e400` parses to `Infinity`, which would
  // put a node at a coordinate no layout can render or diff meaningfully.
  it("refuses a non-finite node position", () => {
    expect(
      refused('{"version":1,"nodes":[{"id":"n1","name":"x","position":{"x":1e400,"y":0}}],"edges":[]}'),
    ).toMatch(/position (has no numeric|is not an object)/);
  });

  // `null` is how a hand-edited or generated file spells "absent", and the two
  // must not diverge: an optional field read as `null` would serialize back as
  // a field that was never there.
  it("reads a null optional field as absent", () => {
    const doc = parsed(
      '{"version":1,"nodes":[{"id":"n1","name":"x","artifactIds":null,' +
        '"prompt":null,"position":{"x":0,"y":0}}],"edges":[]}',
    );
    expect(doc.nodes[0].artifactIds).toBeUndefined();
    expect(doc.nodes[0].prompt).toBeUndefined();
    expect(serializeFlowDocument(doc)).not.toContain("null");
  });

  it("refuses a node missing the fields FLO-FR-06 requires", () => {
    const doc = (nodes: string) => `{"version":1,"nodes":[${nodes}],"edges":[]}`;
    expect(refused(doc('{"name":"a","position":{"x":0,"y":0}}'))).toMatch(/node 0 has no id/);
    expect(refused(doc('{"id":"n1","position":{"x":0,"y":0}}'))).toMatch(/has no name/);
    expect(refused(doc('{"id":"n1","name":"a"}'))).toMatch(/position (has no numeric|is not an object)/);
    expect(refused(doc('{"id":"n1","name":"a","position":{"x":"0","y":0}}'))).toMatch(
      /position (has no numeric|is not an object)/,
    );
    expect(
      refused(doc('{"id":"n1","name":"a","position":{"x":0,"y":0},"prompt":7}')),
    ).toMatch(/prompt must be a string/);
  });

  // FLO-FR-06: ids are what a node's identity and every edge's endpoints are
  // resolved through, so a duplicate is not something the canvas can render.
  it("refuses duplicate node ids and duplicate edge ids", () => {
    const two = (id: string) =>
      `{"id":"${id}","name":"x","position":{"x":0,"y":0}}`;
    expect(
      refused(`{"version":1,"nodes":[${two("n1")},${two("n1")}],"edges":[]}`),
    ).toMatch(/duplicate element id "n1"/);
    expect(
      refused(
        `{"version":1,"nodes":[${two("n1")},${two("n2")}],"edges":[` +
          `{"id":"e1","from":"n1","to":"n2"},{"id":"e1","from":"n2","to":"n1"}]}`,
      ),
    ).toMatch(/duplicate edge id "e1"/);
  });

  // FLO-FR-08: no edge exists without both of its endpoints — a stored one that
  // does is a graph the canvas cannot draw.
  it("refuses an edge whose endpoint is not a node in the document", () => {
    const nodes = '{"id":"n1","name":"x","position":{"x":0,"y":0}}';
    expect(
      refused(`{"version":1,"nodes":[${nodes}],"edges":[{"id":"e1","from":"n1","to":"gone"}]}`),
    ).toMatch(/ends at unknown element "gone"/);
    expect(
      refused(`{"version":1,"nodes":[${nodes}],"edges":[{"id":"e1","from":"gone","to":"n1"}]}`),
    ).toMatch(/starts at unknown element "gone"/);
  });

  // The two graph rules, enforced on the way in as well as at the canvas — a
  // stored document violating one is not something this editor could have
  // produced, and opening it would mean writing back a graph the file never
  // described.
  it("refuses a stored self-edge and a stored duplicate ordered pair", () => {
    const nodes =
      '{"id":"n1","name":"x","position":{"x":0,"y":0}},' +
      '{"id":"n2","name":"y","position":{"x":0,"y":0}}';
    expect(
      refused(`{"version":1,"nodes":[${nodes}],"edges":[{"id":"e1","from":"n1","to":"n1"}]}`),
    ).toMatch(/connects element "n1" to itself/);
    expect(
      refused(
        `{"version":1,"nodes":[${nodes}],"edges":[` +
          `{"id":"e1","from":"n1","to":"n2"},{"id":"e2","from":"n1","to":"n2"}]}`,
      ),
    ).toMatch(/more than one edge from "n1" to "n2"/);
  });

  // FLO-FR-18: a cycle is not an error — it is how a loop or a retry is
  // expressed — so a stored one must load like any other graph.
  it("reads a stored cycle without complaint", () => {
    const doc = parsed(
      '{"version":1,"nodes":[' +
        '{"id":"n1","name":"a","position":{"x":0,"y":0}},' +
        '{"id":"n2","name":"b","position":{"x":0,"y":0}}],' +
        '"edges":[{"id":"e1","from":"n1","to":"n2"},' +
        '{"id":"e2","from":"n2","to":"n1","label":"retry"}]}',
    );
    expect(doc.edges.map((e) => [e.from, e.to])).toEqual([
      ["n1", "n2"],
      ["n2", "n1"],
    ]);
  });
});

describe("serialization (FLO NFR: stable key order and formatting)", () => {
  // The Flow is a committed, diffable file: opening one and saving it without
  // editing must not show up in Git as a change.
  it("round-trips an unedited document to the same bytes", () => {
    const once = serializeFlowDocument(parsed(SAMPLE));
    expect(serializeFlowDocument(parsed(once))).toBe(once);
    expect(parsed(once)).toEqual(parsed(SAMPLE));
  });

  it("writes fields in a fixed order whatever order they arrived in", () => {
    // The same node with its keys shuffled: `JSON.stringify` preserves
    // insertion order, so a serializer that echoed the parsed object would
    // produce a different file for an identical graph.
    const shuffled = parsed(
      '{"nodes":[{"position":{"y":8,"x":4},"prompt":"p",' +
        '"name":"N","artifactIds":["a.md"],"id":"n1"}],"description":"d",' +
        '"edges":[],"name":"F","version":1}',
    );
    expect(serializeFlowDocument(shuffled)).toBe(
      `{
  "version": 1,
  "name": "F",
  "description": "d",
  "nodes": [
    {
      "id": "n1",
      "name": "N",
      "artifactIds": [
        "a.md"
      ],
      "prompt": "p",
      "position": {
        "x": 4,
        "y": 8
      }
    }
  ],
  "edges": []
}
`,
    );
  });

  it("omits absent optional fields rather than writing them as null", () => {
    const text = serializeFlowDocument(chain());
    expect(text).not.toContain("artifactId");
    expect(text).not.toContain("artifactIds");
    expect(text).not.toContain("prompt");
    expect(text).not.toContain("label");
    expect(text).not.toContain("null");
    expect(text.endsWith("}\n")).toBe(true);
  });
});

describe("node mutation (FLO-FR-06 – FLO-FR-14, FLO-FR-21)", () => {
  it("adds an unconnected node with a default name and neither field", () => {
    const doc = addNode(emptyFlow(), { x: 12, y: 34 });
    expect(doc.nodes).toEqual([
      { id: "n1", name: "New node", position: { x: 12, y: 34 } },
    ]);
    expect(doc.edges).toEqual([]);
  });

  // Without this, deleting one node and adding another re-mints an id already
  // in use: React keys collide and an edge resolves to the wrong node.
  it("mints an id nothing in the document holds, after a delete", () => {
    let doc = addNode(addNode(addNode(emptyFlow(), { x: 0, y: 0 }), { x: 0, y: 0 }), {
      x: 0,
      y: 0,
    });
    doc = removeNode(doc, "n2");
    doc = addNode(doc, { x: 0, y: 0 });
    const ids = doc.nodes.map((n) => n.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("removes a node together with every edge incident to it (FLO-FR-08)", () => {
    const doc = removeNode(chain(), "n2");
    expect(doc.nodes.map((n) => n.id)).toEqual(["n1", "n3"]);
    expect(doc.edges).toEqual([]);
  });

  it("renames without touching identity (FLO-FR-09)", () => {
    // Two nodes may share a name — identity is the id, never the name.
    let doc = renameNode(chain(), "n1", "Collect context");
    doc = renameNode(doc, "n2", "Collect context");
    expect(doc.nodes.map((n) => n.name)).toEqual([
      "Collect context",
      "Collect context",
      "New node",
    ]);
    expect(doc.nodes.map((n) => n.id)).toEqual(["n1", "n2", "n3"]);
    expect(doc.edges).toEqual(chain().edges);
  });

  // FLO-FR-25: the Flow's own fields. A Flow written before it could carry a
  // name opens as one with an empty name rather than as an unreadable file —
  // the alternative is refusing to open every Flow that already exists.
  it("names and describes the Flow, and reads a document that carries neither", () => {
    const older = parsed('{"version":1,"nodes":[],"edges":[]}');
    expect(older.name).toBe("");
    expect(older.description).toBeUndefined();

    let doc = setFlowName(older, "Onboarding review");
    doc = setFlowDescription(doc, "Reviews a new joiner's first PR.");
    expect(doc.name).toBe("Onboarding review");
    expect(doc.description).toBe("Reviews a new joiner's first PR.");

    // The name is always written, empty or not; the description is dropped when
    // emptied, so a cleared one serializes as one that was never set.
    doc = setFlowDescription(doc, "");
    expect("description" in doc).toBe(false);
    const text = serializeFlowDocument(setFlowName(doc, ""));
    expect(text).toContain('"name": ""');
    expect(text).not.toContain("description");
    expect(parsed(text).name).toBe("");
  });

  it("returns the same document when a field is set to what it holds", () => {
    const doc = setFlowDescription(setFlowName(emptyFlow(), "F"), "d");
    expect(setFlowName(doc, "F")).toBe(doc);
    expect(setFlowDescription(doc, "d")).toBe(doc);
    // An empty description set on a document that has none changes nothing.
    const bare = emptyFlow();
    expect(setFlowDescription(bare, "")).toBe(bare);
  });

  it("refuses a name or description that is not a string (FLO-FR-05)", () => {
    expect(refused('{"version":1,"name":7,"nodes":[],"edges":[]}')).toMatch(
      /`name` must be a string/,
    );
    expect(
      refused('{"version":1,"description":[],"nodes":[],"edges":[]}'),
    ).toMatch(/`description` must be a string/);
  });

  it("accumulates artifact references and removes them one at a time (FLO-FR-10)", () => {
    let doc = addElementArtifact(chain(), "n1", "prompts/gather.md");
    doc = addElementArtifact(doc, "n1", "prompts/other.md");
    // Added, not replaced, and in the order the author added them.
    expect(doc.nodes[0].artifactIds).toEqual([
      "prompts/gather.md",
      "prompts/other.md",
    ]);

    doc = removeElementArtifact(doc, "n1", "prompts/gather.md");
    expect(doc.nodes[0].artifactIds).toEqual(["prompts/other.md"]);

    doc = removeElementArtifact(doc, "n1", "prompts/other.md");
    // Emptied means absent, not `[]`: an empty list would serialize as a field
    // that was never there on a node that references nothing (FLO-FR-14).
    expect("artifactIds" in doc.nodes[0]).toBe(false);
    // None of it changed anything else about the node or its edges.
    expect(doc.nodes[0].name).toBe(chain().nodes[0].name);
    expect(doc.edges).toEqual(chain().edges);
  });

  // FLO-FR-26: an action that leaves the graph as it was is not an edit. The
  // store raises the unsaved state on any new document, so a mutator that
  // rebuilt the graph to change nothing would dirty a Flow nobody edited.
  it("returns the very same document for an add or remove that changes nothing", () => {
    const doc = addElementArtifact(chain(), "n1", "prompts/gather.md");
    expect(addElementArtifact(doc, "n1", "prompts/gather.md")).toBe(doc);
    expect(addElementArtifact(doc, "n1", "")).toBe(doc);
    expect(removeElementArtifact(doc, "n1", "prompts/never-added.md")).toBe(doc);
  });

  // A node written before a node could reference more than one artifact. Its
  // Flow has to open — landing in the FLO-FR-05 error state over it would cost
  // the author the whole file — and its next write emits the list form.
  it("reads the single `artifactId` an older document carries (FLO-FR-10)", () => {
    const doc = parsed(
      '{"version":1,"nodes":[{"id":"n1","name":"x","artifactId":"a.md",' +
        '"position":{"x":0,"y":0}}],"edges":[]}',
    );
    expect(doc.nodes[0].artifactIds).toEqual(["a.md"]);
    expect(serializeFlowDocument(doc)).toContain('"artifactIds"');
    expect(serializeFlowDocument(doc)).not.toContain('"artifactId"');
  });

  it("folds a repeated reference and refuses a non-string one", () => {
    const doc = parsed(
      '{"version":1,"nodes":[{"id":"n1","name":"x",' +
        '"artifactIds":["a.md","b.md","a.md"],"position":{"x":0,"y":0}}],"edges":[]}',
    );
    expect(doc.nodes[0].artifactIds).toEqual(["a.md", "b.md"]);
    expect(
      refused(
        '{"version":1,"nodes":[{"id":"n1","name":"x","artifactIds":[7],' +
          '"position":{"x":0,"y":0}}],"edges":[]}',
      ),
    ).toMatch(/not a string/);
    // An empty ENTRY is refused alongside a non-string one: an artifact id is a
    // path-derived node key (ASC-FR-13) and `""` names nothing. The backend
    // refuses it too (FGV-FR-08), and this is where the two readings of the
    // schema would otherwise diverge — a body it called valid would land here
    // and fail to deserialize.
    expect(
      refused(
        '{"version":1,"nodes":[{"id":"n1","name":"x","artifactIds":[""],' +
          '"position":{"x":0,"y":0}}],"edges":[]}',
      ),
    ).toMatch(/not a string/);
    expect(
      refused(
        '{"version":1,"nodes":[{"id":"n1","name":"x","artifactIds":"a.md",' +
          '"position":{"x":0,"y":0}}],"edges":[]}',
      ),
    ).toMatch(/must be an array/);
    // An empty list is the same as no references at all.
    expect(
      parsed(
        '{"version":1,"nodes":[{"id":"n1","name":"x","artifactIds":[],' +
          '"position":{"x":0,"y":0}}],"edges":[]}',
      ).nodes[0].artifactIds,
    ).toBeUndefined();
  });

  it("drops an emptied inline prompt rather than storing an empty string", () => {
    let doc = setNodePrompt(chain(), "n1", "One paragraph, no lists.");
    expect(doc.nodes[0].prompt).toBe("One paragraph, no lists.");
    doc = setNodePrompt(doc, "n1", "");
    expect("prompt" in doc.nodes[0]).toBe(false);
    expect(serializeFlowDocument(doc)).not.toContain("prompt");
  });

  it("moves a node without disturbing anything else (FLO-FR-21)", () => {
    const doc = moveNode(chain(), "n2", { x: 640, y: 220 });
    expect(doc.nodes[1].position).toEqual({ x: 640, y: 220 });
    expect(doc.nodes[0].position).toEqual({ x: 0, y: 0 });
    expect(doc.edges).toEqual(chain().edges);
  });

  it("leaves the document it was given untouched", () => {
    // Every mutator returns a new document: React compares identities to decide
    // what to re-render, and an in-place edit would render nothing.
    const before = chain();
    const snapshot = JSON.parse(JSON.stringify(before));
    removeNode(renameNode(addNode(before, { x: 1, y: 1 }), "n1", "x"), "n2");
    expect(before).toEqual(snapshot);
  });
});

describe("edge rules (FLO-FR-15 – FLO-FR-20)", () => {
  it("refuses a connection from a node to itself (FLO-FR-16)", () => {
    const doc = chain();
    const result = connect(doc, "n1", "n1");
    expect(result).toEqual({ ok: false, refused: "self" });
  });

  it("refuses a second edge on an ordered pair, keeping the first (FLO-FR-17)", () => {
    const doc = renameEdge(chain(), "e1", "ok");
    expect(connect(doc, "n1", "n2")).toEqual({ ok: false, refused: "duplicate" });
    expect(doc.edges.find((e) => e.id === "e1")?.label).toBe("ok");
  });

  it("treats B->A as a separate ordered pair from A->B (FLO-FR-17 / FLO-FR-18)", () => {
    const result = connect(chain(), "n2", "n1");
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.doc.edges.map((e) => [e.from, e.to])).toEqual([
      ["n1", "n2"],
      ["n2", "n3"],
      ["n2", "n1"],
    ]);
  });

  // FLO-FR-18: no connection is ever refused on the grounds that it would form
  // a cycle. This is the retry/loop case, and it must simply work.
  it("creates an edge that closes a cycle", () => {
    const result = connect(chain(), "n3", "n1");
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.doc.edges).toHaveLength(3);
    expect(result.doc.edges[2]).toEqual({ id: "e3", from: "n3", to: "n1" });
  });

  it("allows unbounded fan-out and fan-in from one node (FLO-FR-15)", () => {
    let doc = chain();
    doc = addNode(doc, { x: 0, y: 0 });
    for (const [from, to] of [
      ["n1", "n3"],
      ["n1", "n4"],
      ["n2", "n4"],
      ["n3", "n4"],
    ]) {
      const r = connect(doc, from, to);
      expect(r.ok).toBe(true);
      if (r.ok) doc = r.doc;
    }
    expect(doc.edges.filter((e) => e.from === "n1")).toHaveLength(3);
    expect(doc.edges.filter((e) => e.to === "n4")).toHaveLength(3);
  });

  it("refuses a connection to a node that is no longer in the graph", () => {
    const doc = removeNode(chain(), "n3");
    expect(connect(doc, "n1", "n3")).toEqual({ ok: false, refused: "unknown-node" });
  });

  // The counterpart of the node-id test above, on the other id space: without
  // it, removing an edge and drawing another re-mints an id already in use and
  // two edges collide on one React key.
  it("mints an edge id nothing in the document holds, after a delete", () => {
    let doc = chain();
    doc = removeEdge(doc, "e1");
    const first = connect(doc, "n1", "n3");
    expect(first.ok).toBe(true);
    if (!first.ok) return;
    const second = connect(first.doc, "n3", "n1");
    expect(second.ok).toBe(true);
    if (!second.ok) return;
    const ids = second.doc.edges.map((e) => e.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  // A selection can name something an external reload removed; the mutators
  // have to be inert for it rather than throwing or inventing an entry.
  it("is a no-op for an id the document does not hold", () => {
    const before = chain();
    expect(removeNode(before, "gone")).toEqual(before);
    expect(renameNode(before, "gone", "x")).toEqual(before);
    expect(setNodePrompt(before, "gone", "p")).toEqual(before);
    // `toBe`, not `toEqual`: identity is what `applyEdit` reads to tell a
    // no-op from an edit, and an equal-but-new document dirties the Flow.
    expect(addElementArtifact(before, "gone", "a.md")).toBe(before);
    expect(removeElementArtifact(before, "gone", "a.md")).toBe(before);
    expect(moveNode(before, "gone", { x: 1, y: 1 })).toBe(before);
    expect(removeEdge(before, "gone")).toEqual(before);
    expect(renameEdge(before, "gone", "x")).toEqual(before);
  });

  it("removes only the named edge, leaving both endpoints (FLO-FR-20)", () => {
    const doc = removeEdge(chain(), "e1");
    expect(doc.nodes.map((n) => n.id)).toEqual(["n1", "n2", "n3"]);
    expect(doc.edges.map((e) => e.id)).toEqual(["e2"]);
  });

  it("labels an edge and clears the label back to a bare arrow (FLO-FR-19)", () => {
    let doc = renameEdge(chain(), "e2", "retry");
    expect(doc.edges[1].label).toBe("retry");
    // Labels are not required to be unique.
    doc = renameEdge(doc, "e1", "retry");
    expect(doc.edges.map((e) => e.label)).toEqual(["retry", "retry"]);
    doc = renameEdge(doc, "e1", "");
    expect("label" in doc.edges[0]).toBe(false);
  });
});

