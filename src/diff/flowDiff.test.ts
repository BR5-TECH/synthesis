import { describe, expect, it } from "vitest";

import { changedFacts, changedOnly, flowDiff } from "./flowDiff";
import type { FlowDiffEntry } from "./flowDiff";

/** A Flow body, from the parts a test cares about. */
function flow(doc: {
  name?: string;
  description?: string;
  nodes?: Array<Record<string, unknown>>;
  edges?: Array<Record<string, unknown>>;
}): string {
  return JSON.stringify({
    version: 1,
    name: doc.name ?? "F",
    ...(doc.description ? { description: doc.description } : {}),
    nodes: doc.nodes ?? [],
    edges: doc.edges ?? [],
  });
}

const node = (
  id: string,
  name: string,
  extra: Record<string, unknown> = {},
) => ({ id, name, position: { x: 0, y: 0 }, ...extra });

function entries(oldBody: string | null, newBody: string | null) {
  const result = flowDiff(oldBody, newBody);
  if (!result.ok) throw new Error(`expected a diff, got: ${result.error}`);
  return result.entries;
}

const byKey = (list: FlowDiffEntry[], key: string) =>
  list.find((e) => e.key === key)!;

describe("flowDiff (DFV-FR-33)", () => {
  it("reports an added and a removed node as the entries they are", () => {
    const before = flow({ nodes: [node("n1", "Collect context")] });
    const after = flow({ nodes: [node("n2", "Draft review")] });

    const list = entries(before, after);

    expect(byKey(list, "node:n1")).toMatchObject({
      kind: "node",
      status: "removed",
      titleBefore: "Collect context",
      titleAfter: null,
    });
    expect(byKey(list, "node:n2")).toMatchObject({
      status: "added",
      titleBefore: null,
      titleAfter: "Draft review",
    });
  });

  // DFV-FR-33: the pairing is by id, which FLO-FR-06 makes stable across saves.
  // Pairing by name would report every rename as a delete and an add, and lose
  // every other change the renamed node carries.
  it("reads a rename as one changed node rather than an add and a delete", () => {
    const list = entries(
      flow({ nodes: [node("n1", "Collect context")] }),
      flow({ nodes: [node("n1", "Gather context")] }),
    );

    expect(list.filter((e) => e.kind === "node")).toHaveLength(1);
    const entry = byKey(list, "node:n1");
    expect(entry.status).toBe("changed");
    expect(entry.facts).toContainEqual({
      label: "Name",
      status: "changed",
      before: "Collect context",
      after: "Gather context",
    });
  });

  it("reports each artifact reference gained and lost on its own", () => {
    const list = entries(
      flow({
        nodes: [node("n1", "A", { artifactIds: ["prompts/a.md", "kept.md"] })],
      }),
      flow({
        nodes: [node("n1", "A", { artifactIds: ["kept.md", "prompts/b.md"] })],
      }),
    );

    const facts = byKey(list, "node:n1").facts;
    expect(facts).toContainEqual({
      label: "Artifact",
      status: "unchanged",
      before: "kept.md",
      after: "kept.md",
    });
    expect(facts).toContainEqual({
      label: "Artifact",
      status: "added",
      after: "prompts/b.md",
    });
    expect(facts).toContainEqual({
      label: "Artifact",
      status: "removed",
      before: "prompts/a.md",
    });
  });

  // A node that is itself new carries no marked facts: its own marking already
  // says the whole of it is new, and marking each part repeats that.
  it("leaves the facts of an added node unmarked", () => {
    const list = entries(
      flow({ nodes: [] }),
      flow({
        nodes: [node("n1", "A", { artifactIds: ["a.md"], prompt: "do it" })],
      }),
    );

    const entry = byKey(list, "node:n1");
    expect(entry.status).toBe("added");
    expect(entry.facts.map((f) => f.status)).toEqual(["unchanged", "unchanged"]);
    expect(entry.facts.map((f) => f.label)).toEqual(["Artifact", "Prompt"]);
  });

  it("reports an inline prompt added, changed, and removed", () => {
    const withPrompt = flow({ nodes: [node("n1", "A", { prompt: "one" })] });
    const without = flow({ nodes: [node("n1", "A")] });

    expect(byKey(entries(without, withPrompt), "node:n1").facts).toEqual([
      { label: "Prompt", status: "added", after: "one" },
    ]);
    expect(byKey(entries(withPrompt, without), "node:n1").facts).toEqual([
      { label: "Prompt", status: "removed", before: "one" },
    ]);
    expect(
      byKey(
        entries(withPrompt, flow({ nodes: [node("n1", "A", { prompt: "two" })] })),
        "node:n1",
      ).facts,
    ).toEqual([
      { label: "Prompt", status: "changed", before: "one", after: "two" },
    ]);
  });

  // A node the author only dragged still changed the file, so a diff that
  // reported nothing would be saying the file did not change.
  it("reports a move, and reports it last", () => {
    const list = entries(
      flow({ nodes: [node("n1", "A", { prompt: "p" })] }),
      flow({
        nodes: [{ id: "n1", name: "B", position: { x: 40, y: 10 }, prompt: "p" }],
      }),
    );

    const facts = byKey(list, "node:n1").facts;
    expect(facts[facts.length - 1]).toEqual({
      label: "Position",
      status: "changed",
      before: "0, 0",
      after: "40, 10",
    });
  });

  describe("connections", () => {
    const two = [node("n1", "A"), node("n2", "B")];

    it("pairs an edge by its endpoints and names it by them", () => {
      const list = entries(
        flow({ nodes: two, edges: [{ id: "e1", from: "n1", to: "n2", label: "ok" }] }),
        // A different edge id for the same connection: the same edge to a reader.
        flow({ nodes: two, edges: [{ id: "e9", from: "n1", to: "n2", label: "retry" }] }),
      );

      const entry = byKey(list, "edge:n1 n2");
      expect(entry.kind).toBe("edge");
      expect(entry.status).toBe("changed");
      expect(entry.titleAfter).toBe("A → B");
      expect(entry.facts).toEqual([
        { label: "Label", status: "changed", before: "ok", after: "retry" },
      ]);
    });

    it("reports an added and a removed connection", () => {
      const list = entries(
        flow({ nodes: two, edges: [{ id: "e1", from: "n1", to: "n2" }] }),
        flow({ nodes: two, edges: [{ id: "e1", from: "n2", to: "n1" }] }),
      );

      expect(byKey(list, "edge:n1 n2").status).toBe("removed");
      expect(byKey(list, "edge:n2 n1").status).toBe("added");
      // Each is named in its own revision's terms.
      expect(byKey(list, "edge:n2 n1").titleAfter).toBe("B → A");
    });

    it("names an edge by node id when the node carries no name", () => {
      const list = entries(
        null,
        flow({
          nodes: [node("n1", ""), node("n2", "B")],
          edges: [{ id: "e1", from: "n1", to: "n2" }],
        }),
      );
      expect(byKey(list, "edge:n1 n2").titleAfter).toBe("n1 → B");
    });
  });

  it("reports the Flow's own name and description", () => {
    const list = entries(
      flow({ name: "Old name", description: "was" }),
      flow({ name: "New name" }),
    );

    expect(byKey(list, "flow")).toMatchObject({
      kind: "flow",
      status: "changed",
      facts: [
        { label: "Name", status: "changed", before: "Old name", after: "New name" },
        { label: "Description", status: "removed", before: "was" },
      ],
    });
  });

  // A missing revision is the comparison adding or deleting the file, and an
  // empty Flow is exactly what that means for every entry.
  it("reads a missing revision as an empty Flow", () => {
    const body = flow({ nodes: [node("n1", "A")] });

    expect(
      entries(null, body)
        .filter((e) => e.kind === "node")
        .map((e) => e.status),
    ).toEqual(["added"]);
    expect(
      entries(body, null)
        .filter((e) => e.kind === "node")
        .map((e) => e.status),
    ).toEqual(["removed"]);
  });

  it("reports a revision that is not a Flow rather than comparing it", () => {
    expect(flowDiff("{ not json", flow({}))).toEqual({
      ok: false,
      error: expect.stringMatching(/^old revision: .*not valid JSON/),
    });
    expect(flowDiff(flow({}), '{"version":9}')).toEqual({
      ok: false,
      error: expect.stringMatching(/^new revision: .*unsupported Flow version/),
    });
  });

  // A hand edit or a merge can list the same nodes in a different order. Pairing
  // that walked the two revisions in step would run past a reordered node and
  // pair it with nothing — reporting it as added and losing what it carries.
  it("pairs by identity however the two revisions order their nodes", () => {
    const a = node("n1", "A", { prompt: "one" });
    const b = node("n2", "B");
    const c = node("n3", "C");
    const list = entries(
      flow({ nodes: [a, b, c] }),
      // Reordered, and the reordered node changed too.
      flow({ nodes: [c, { ...a, prompt: "two" }, b] }),
    );

    expect(
      list.filter((e) => e.kind === "node").map((e) => [e.key, e.status]),
    ).toEqual([
      ["node:n3", "unchanged"],
      ["node:n1", "changed"],
      ["node:n2", "unchanged"],
    ]);
    expect(byKey(list, "node:n1").facts).toEqual([
      { label: "Prompt", status: "changed", before: "one", after: "two" },
    ]);
  });

  it("pairs a reordered connection by its endpoints", () => {
    const nodes = [node("n1", "A"), node("n2", "B"), node("n3", "C")];
    const list = entries(
      flow({
        nodes,
        edges: [
          { id: "e1", from: "n1", to: "n2" },
          { id: "e2", from: "n2", to: "n3", label: "ok" },
        ],
      }),
      flow({
        nodes,
        edges: [
          { id: "e2", from: "n2", to: "n3", label: "retry" },
          { id: "e1", from: "n1", to: "n2" },
        ],
      }),
    );

    expect(byKey(list, "edge:n1 n2").status).toBe("unchanged");
    expect(byKey(list, "edge:n2 n3").status).toBe("changed");
    expect(list.filter((e) => e.kind === "edge")).toHaveLength(2);
  });

  // DFV-FR-35: each side states what its own revision holds, so a removed
  // entry's facts have to be on the side that will render them.
  it("states a removed node's own facts on the side it exists on", () => {
    const list = entries(
      flow({
        nodes: [node("n1", "Gone", { artifactIds: ["a.md"], prompt: "was" })],
        edges: [],
      }),
      flow({}),
    );

    expect(byKey(list, "node:n1").facts).toEqual([
      { label: "Artifact", status: "unchanged", before: "a.md" },
      { label: "Prompt", status: "unchanged", before: "was" },
    ]);
  });

  it("states a removed connection's label on the side it exists on", () => {
    const nodes = [node("n1", "A"), node("n2", "B")];
    const list = entries(
      flow({ nodes, edges: [{ id: "e1", from: "n1", to: "n2", label: "ok" }] }),
      flow({ nodes, edges: [] }),
    );

    expect(byKey(list, "edge:n1 n2")).toMatchObject({
      status: "removed",
      titleBefore: "A → B",
      titleAfter: null,
      facts: [{ label: "Label", status: "unchanged", before: "ok" }],
    });
  });

  // The Flow's own entry belongs to the revisions that have a Flow at all: on a
  // file the comparison added, an entry present on both sides would render as a
  // ghost opposite the side that has no file.
  it("adds and removes the Flow's own entry with the file", () => {
    expect(byKey(entries(null, flow({ name: "New" })), "flow")).toMatchObject({
      status: "added",
      titleBefore: null,
      titleAfter: "Flow",
      facts: [{ label: "Name", status: "unchanged", after: "New" }],
    });
    expect(byKey(entries(flow({ name: "Old" }), null), "flow")).toMatchObject({
      status: "removed",
      titleBefore: "Flow",
      titleAfter: null,
      facts: [{ label: "Name", status: "unchanged", before: "Old" }],
    });
  });

  // DFV-FR-36: the file changed, the graph did not. Over a Flow with a graph in
  // it, so the pairing is what proves it rather than `JSON.parse` alone.
  it("reports nothing changed when only the file's formatting moved", () => {
    const nodes = [node("n1", "A", { prompt: "p" }), node("n2", "B")];
    const edges = [{ id: "e1", from: "n1", to: "n2", label: "ok" }];
    const compact = JSON.stringify({ version: 1, name: "F", nodes, edges });
    const pretty = JSON.stringify(
      { edges, nodes, name: "F", version: 1 },
      null,
      4,
    );

    expect(changedOnly(entries(compact, pretty))).toEqual([]);
  });
});

describe("what each mode shows (DFV-FR-34)", () => {
  const before = flow({
    nodes: [node("n1", "A", { prompt: "keep" }), node("n2", "B")],
  });
  const after = flow({
    nodes: [
      node("n1", "A", { prompt: "keep" }),
      node("n2", "B", { artifactIds: ["new.md"] }),
    ],
  });

  it("shows only what changed, and only the facts that differ", () => {
    const list = entries(before, after);

    // Final's material: every entry, unchanged ones included.
    expect(list.map((e) => e.key)).toEqual(["flow", "node:n1", "node:n2"]);
    // Unified's: the changed entry alone…
    const changed = changedOnly(list);
    expect(changed.map((e) => e.key)).toEqual(["node:n2"]);
    // …showing the gained reference and not the facts that stayed put.
    expect(changedFacts(changed[0])).toEqual([
      { label: "Artifact", status: "added", after: "new.md" },
    ]);
  });

  it("keeps every fact of an added or removed entry, which has no unchanged ones", () => {
    const list = entries(flow({}), flow({ nodes: [node("n1", "A", { prompt: "p" })] }));
    const added = byKey(list, "node:n1");

    expect(changedFacts(added)).toEqual(added.facts);
  });
});

describe("loops in a Flow comparison (DFV-FR-33)", () => {
  const loop = (over: Record<string, unknown> = {}) => ({
    id: "l1",
    name: "Review cycle",
    position: { x: 0, y: 0 },
    size: { width: 400, height: 200 },
    ...over,
  });
  const node = (id: string, over: Record<string, unknown> = {}) => ({
    id,
    name: id.toUpperCase(),
    position: { x: 0, y: 0 },
    ...over,
  });
  const body = (doc: Record<string, unknown>) =>
    JSON.stringify({ version: 1, name: "F", nodes: [], edges: [], ...doc });

  const entry = (result: ReturnType<typeof flowDiff>, key: string) => {
    if (!result.ok) throw new Error(result.error);
    const found = result.entries.find((e) => e.key === key);
    if (!found) {
      throw new Error(
        `no entry ${key} in ${result.entries.map((e) => e.key).join(", ")}`,
      );
    }
    return found;
  };

  it("renders an added loop as an added entry", () => {
    const result = flowDiff(body({}), body({ loops: [loop()] }));
    const l = entry(result, "loop:l1");
    expect(l.kind).toBe("loop");
    expect(l.status).toBe("added");
    expect(l.titleAfter).toBe("Review cycle");
  });

  // DFV-FR-33: a loop carries artifact references on a node's terms, so a
  // reference it gained is stated individually rather than as a list that
  // changed.
  it("states a loop's own fields that differ", () => {
    const result = flowDiff(
      body({
        loops: [loop({ artifactIds: ["prompts/old-stop.md"], maxPasses: 3 })],
      }),
      body({
        loops: [loop({ artifactIds: ["prompts/stop-when-clean.md"], maxPasses: 5 })],
      }),
    );
    const l = entry(result, "loop:l1");
    expect(l.status).toBe("changed");
    const facts = l.facts;
    const refs = facts.filter((f) => f.label === "Artifact");
    expect(refs).toEqual([
      { label: "Artifact", status: "added", after: "prompts/stop-when-clean.md" },
      { label: "Artifact", status: "removed", before: "prompts/old-stop.md" },
    ]);
    const byLabel = Object.fromEntries(facts.map((f) => [f.label, f]));
    expect(byLabel["Max passes"]).toMatchObject({ before: "3", after: "5" });
    // The references read ahead of the pass bound, both being what the loop is
    // for, and both ahead of what it holds.
    const labels = facts.map((f) => f.label);
    expect(labels.indexOf("Artifact")).toBeLessThan(labels.indexOf("Max passes"));
  });

  // DFV-FR-33: a container change is reported AHEAD of any position fact,
  // because it changes what the step is part of and what it may connect to.
  it("names the container a node left and the one it joined, ahead of position", () => {
    const result = flowDiff(
      body({ loops: [loop()], nodes: [node("n1")] }),
      body({
        loops: [loop()],
        nodes: [node("n1", { parentId: "l1", position: { x: 9, y: 9 } })],
      }),
    );
    const n = entry(result, "node:n1");
    expect(n.status).toBe("changed");
    const labels = n.facts.map((f) => f.label);
    expect(labels).toContain("Container");
    expect(labels.indexOf("Container")).toBeLessThan(labels.indexOf("Position"));
    const container = n.facts.find((f) => f.label === "Container")!;
    expect(container).toMatchObject({
      status: "changed",
      before: "(top level)",
      after: "Review cycle",
    });
  });

  // DFV-FR-33: every element is one entry at one level however deeply the loops
  // nest — a loop states what it holds by naming its members.
  it("keeps nested loops flat and names what each holds", () => {
    const nested = body({
      loops: [loop(), loop({ id: "l2", name: "Inner", parentId: "l1" })],
      nodes: [node("n1", { parentId: "l2" })],
    });
    const result = flowDiff(body({}), nested);
    if (!result.ok) throw new Error(result.error);
    expect(result.entries.filter((e) => e.kind === "loop").map((e) => e.key)).toEqual([
      "loop:l1",
      "loop:l2",
    ]);
    const outer = entry(result, "loop:l1");
    expect(outer.facts.find((f) => f.label === "Holds")?.after).toBe("Inner");
    const inner = entry(result, "loop:l2");
    expect(inner.facts.find((f) => f.label === "Holds")?.after).toBe("N1");
  });

  it("reports a resized loop last, after what it is for", () => {
    const result = flowDiff(
      body({ loops: [loop()] }),
      body({ loops: [loop({ size: { width: 600, height: 300 } })] }),
    );
    const l = entry(result, "loop:l1");
    expect(l.facts[l.facts.length - 1]).toMatchObject({
      label: "Size",
      status: "changed",
      before: "400 × 200",
      after: "600 × 300",
    });
  });

  // FLO-FR-39: an edge endpoint may be a loop, so the edge's title names it.
  // DFV-FR-33: a removed loop states what it held and where it sat on the
  // BEFORE side — the branch a comparison that only ever adds never reaches.
  it("renders a removed loop with its facts on the before side", () => {
    const result = flowDiff(
      body({
        loops: [loop({ artifactIds: ["prompts/stop-when-clean.md"] })],
        nodes: [node("n1", { parentId: "l1" })],
      }),
      body({}),
    );
    const l = entry(result, "loop:l1");
    expect(l.status).toBe("removed");
    expect(l.titleBefore).toBe("Review cycle");
    expect(l.titleAfter).toBeNull();
    const facts = Object.fromEntries(l.facts.map((f) => [f.label, f]));
    // On a loop that is itself gone, its references are simply part of it.
    expect(facts["Artifact"]).toMatchObject({
      status: "unchanged",
      before: "prompts/stop-when-clean.md",
    });
    expect(facts["Holds"]).toMatchObject({ before: "N1" });
    expect(facts["Holds"].after).toBeUndefined();
  });

  // DFV-FR-33: a container reads ahead of what it holds.
  it("orders every loop entry ahead of every node entry", () => {
    const result = flowDiff(
      null,
      body({ loops: [loop()], nodes: [node("n1", { parentId: "l1" })] }),
    );
    if (!result.ok) throw new Error(result.error);
    const kinds = result.entries.map((e) => e.kind);
    expect(kinds.lastIndexOf("loop")).toBeLessThan(kinds.indexOf("node"));
  });

  // A loop moved between containers is reported the same way a node is.
  it("names the container a nested loop left and the one it joined", () => {
    const outer = loop({ id: "l0", name: "Outer" });
    const result = flowDiff(
      body({ loops: [outer, loop()] }),
      body({ loops: [outer, loop({ parentId: "l0" })] }),
    );
    const l = entry(result, "loop:l1");
    expect(l.facts.find((f) => f.label === "Container")).toMatchObject({
      status: "changed",
      before: "(top level)",
      after: "Outer",
    });
  });

  it("titles an edge that lands on a loop by the loop's name", () => {
    const result = flowDiff(
      null,
      body({
        loops: [loop()],
        nodes: [node("n1")],
        edges: [{ id: "e1", from: "n1", to: "l1" }],
      }),
    );
    expect(entry(result, "edge:n1 l1").titleAfter).toBe("N1 → Review cycle");
  });
});
