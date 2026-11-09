import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
}));

import { logDebug } from "../logging";
import {
  CALM_CACHE_LIMIT,
  cachedCalm,
  readCalm,
  releaseCalm,
  resetCalmForTest,
  showCalm,
} from "./read";
import { CALM_ERROR, type CalmError, type CalmLayout, type CalmResult } from "./types";

const SCHEMA = "https://calm.finos.org/release/1.3/meta/calm.json";

const node = (id: string, type = "service", extra: Record<string, unknown> = {}) => ({
  "unique-id": id,
  "node-type": type,
  name: id.toUpperCase(),
  description: `The ${id}.`,
  ...extra,
});

const connects = (id: string, source: string, destination: string, extra = {}) => ({
  "unique-id": id,
  "relationship-type": { connects: { source: { node: source }, destination: { node: destination } } },
  ...extra,
});

const source = (body: Record<string, unknown>) => JSON.stringify({ $schema: SCHEMA, ...body });

function rendered(result: CalmResult): CalmLayout {
  if (result.state !== "rendered") {
    throw new Error(`expected a diagram, got ${result.summary} ${result.detail ?? ""}`);
  }
  return result.layout;
}

function failed(result: CalmResult): CalmError {
  if (result.state !== "error") throw new Error("expected an error");
  return result;
}

beforeEach(() => {
  resetCalmForTest();
  vi.mocked(logDebug).mockClear();
});

describe("CALM read: parse", () => {
  it("CLM-FR-ATLN: a source that is not JSON fails at the parse step with its line", async () => {
    const result = failed(await readCalm('{\n  "nodes": [\n    oops\n  ]\n}'));
    expect(result.step).toBe("parse");
    expect(result.summary).toBe(CALM_ERROR.parse);
    expect(result.summary).toBe("The CALM source is not valid JSON.");
    expect(result.line).toBe(3);
    expect(result.detail).toMatch(/Line 3, column \d+/);
  });

  it("CLM-FR-ATLN: a top-level value that is not an object fails at the parse step", async () => {
    for (const text of ["[]", "42", "null", '"text"']) {
      const result = failed(await readCalm(text));
      expect(result.step).toBe("parse");
      expect(result.detail).toMatch(/not an object/);
    }
  });
});

describe("CALM read: schema", () => {
  it("CLM-FR-BWGJ, CLM-FR-KDTA: a node with no description fails at the schema step", async () => {
    const result = failed(
      await readCalm(source({ nodes: [{ "unique-id": "a", "node-type": "service", name: "A" }] })),
    );
    expect(result.step).toBe("schema");
    expect(result.summary).toBe("The CALM source does not match the CALM 1.3 schema.");
    expect(result.detail).toContain("/nodes/0: must have required property 'description'");
  });

  it("CLM-FR-BWGJ: a document that declares another version is validated against CALM 1.3", async () => {
    const declared = JSON.stringify({
      $schema: "https://calm.finos.org/release/1.1/meta/calm.json",
      nodes: [{ "unique-id": "a", "node-type": "service", name: "A" }],
    });
    expect(failed(await readCalm(declared)).step).toBe("schema");
    const valid = JSON.stringify({
      $schema: "https://calm.finos.org/release/1.1/meta/calm.json",
      nodes: [node("a")],
    });
    expect(rendered(await readCalm(valid)).graph.nodes).toHaveLength(1);
  });

  it("CLM-FR-OIGM: a schema error lists at most five problems and counts the rest", async () => {
    const nodes = Array.from({ length: 7 }, (_, i) => ({
      "unique-id": `n${i}`,
      "node-type": "service",
      name: `N${i}`,
    }));
    const result = failed(await readCalm(source({ nodes })));
    const lines = result.detail!.split("\n");
    expect(lines).toHaveLength(6);
    expect(lines[0]).toBe("/nodes/0: must have required property 'description'");
    expect(lines[5]).toBe("… and 2 more problems.");
  });

  it("CLM-FR-OIGM: five problems need no count line, and six count one more problem", async () => {
    const nodes = (count: number) =>
      Array.from({ length: count }, (_, i) => ({ "unique-id": `n${i}`, "node-type": "service", name: `N${i}` }));
    const five = failed(await readCalm(source({ nodes: nodes(5) })));
    expect(five.detail!.split("\n")).toHaveLength(5);
    const six = failed(await readCalm(source({ nodes: nodes(6) })));
    const lines = six.detail!.split("\n");
    expect(lines[lines.length - 1]).toBe("… and 1 more problem.");
  });

  it("CLM-FR-ODQZ: the schema step runs before the references step", async () => {
    const result = failed(
      await readCalm(
        source({
          nodes: [{ "unique-id": "a", "node-type": "service", name: "A" }],
          relationships: [connects("r", "a", "missing")],
        }),
      ),
    );
    expect(result.step).toBe("schema");
  });
});

describe("CALM read: references", () => {
  it("CLM-FR-DFFG: a relationship to a node that does not exist fails with its pointer", async () => {
    const result = failed(
      await readCalm(source({ nodes: [node("a")], relationships: [connects("r", "a", "ghost")] })),
    );
    expect(result.step).toBe("references");
    expect(result.summary).toBe("The architecture refers to an item that does not exist.");
    expect(result.detail).toBe(
      '"ghost" does not exist (named at /relationships/0/relationship-type/connects/destination/node).',
    );
  });

  it("CLM-FR-DFFG: an interface, a decision, and a transition must name items that exist", async () => {
    const result = failed(
      await readCalm(
        source({
          nodes: [node("a", "service", { interfaces: [{ "unique-id": "a-http" }] }), node("b")],
          relationships: [
            {
              "unique-id": "r",
              "relationship-type": {
                connects: {
                  source: { node: "a", interfaces: ["a-grpc"] },
                  destination: { node: "b" },
                },
              },
            },
            {
              "unique-id": "choice",
              "relationship-type": {
                options: [{ description: "Pick", nodes: ["c"], relationships: ["nope"] }],
              },
            },
          ],
          flows: [
            {
              "unique-id": "f",
              name: "F",
              description: "A flow.",
              transitions: [
                { "relationship-unique-id": "lost", "sequence-number": 1, description: "Go" },
              ],
            },
          ],
        }),
      ),
    );
    expect(result.step).toBe("references");
    expect(result.detail).toContain('"a-grpc" does not exist');
    expect(result.detail).toContain('"c" does not exist');
    expect(result.detail).toContain('"nope" does not exist');
    expect(result.detail).toContain('"lost" does not exist (named at /flows/0/transitions/0/relationship-unique-id)');
  });

  it("CLM-FR-DHXO: two relationships or two flows with the same unique-id fail the references step", async () => {
    const twoRels = failed(
      await readCalm(source({ nodes: [node("a"), node("b")], relationships: [connects("r", "a", "b"), connects("r", "b", "a")] })),
    );
    expect(twoRels.detail).toBe('"r" is used more than once.');
    const flow = { "unique-id": "f", name: "F", description: "F.", transitions: [{ "relationship-unique-id": "r", "sequence-number": 1, description: "Go" }] };
    const twoFlows = failed(
      await readCalm(source({ nodes: [node("a"), node("b")], relationships: [connects("r", "a", "b")], flows: [flow, flow] })),
    );
    expect(twoFlows.summary).toBe("The architecture uses one unique-id more than once.");
  });

  it("CLM-FR-DFFG: a missing actor, container, member, or destination interface fails the references step", async () => {
    const result = failed(
      await readCalm(
        source({
          nodes: [node("a", "service", { interfaces: [{ "unique-id": "a-in" }] }), node("b")],
          relationships: [
            { "unique-id": "i", "relationship-type": { interacts: { actor: "ghost-actor", nodes: ["a"] } } },
            { "unique-id": "c", "relationship-type": { "composed-of": { container: "ghost-box", nodes: ["ghost-part"] } } },
            {
              "unique-id": "k",
              "relationship-type": { connects: { source: { node: "b" }, destination: { node: "a", interfaces: ["a-out"] } } },
            },
          ],
        }),
      ),
    );
    for (const id of ["ghost-actor", "ghost-box", "ghost-part", "a-out"]) {
      expect(result.detail).toContain(`"${id}" does not exist`);
    }
  });

  it("CLM-FR-DHXO: two nodes with the same unique-id fail the references step", async () => {
    const result = failed(await readCalm(source({ nodes: [node("a"), node("a")] })));
    expect(result.step).toBe("references");
    expect(result.summary).toBe("The architecture uses one unique-id more than once.");
    expect(result.detail).toBe('"a" is used more than once.');
  });
});

describe("CALM read: graph", () => {
  it("CLM-FR-DLUV, CLM-FR-DTYX: each node gets a diagram node with the shape of its type", async () => {
    const types = ["actor", "system", "ecosystem", "service", "database", "webclient", "network", "ldap", "data-asset", "queue"];
    const layout = rendered(
      await readCalm(source({ nodes: types.map((type, i) => node(`n${i}`, type)) })),
    );
    expect(layout.graph.nodes.map((n) => n.shape)).toEqual([
      "actor",
      "system",
      "system",
      "service",
      "database",
      "webclient",
      "network",
      "network",
      "dataAsset",
      "generic",
    ]);
    const generic = layout.graph.nodes[9];
    expect(generic).toMatchObject({ id: "n9", name: "N9", nodeType: "queue", description: "The n9." });
  });

  it("CLM-FR-DWLY: containment nests groups, and a group comes before its members", async () => {
    const layout = rendered(
      await readCalm(
        source({
          nodes: [node("svc"), node("pod", "system"), node("cluster", "network")],
          relationships: [
            { "unique-id": "c1", "relationship-type": { "composed-of": { container: "pod", nodes: ["svc"] } } },
            { "unique-id": "c2", "relationship-type": { "deployed-in": { container: "cluster", nodes: ["pod"] } } },
          ],
        }),
      ),
    );
    const byId = Object.fromEntries(layout.graph.nodes.map((n) => [n.id, n]));
    expect(byId.svc.parent).toBe("pod");
    expect(byId.pod.parent).toBe("cluster");
    expect(byId.pod.group).toBe("composed-of");
    expect(byId.cluster.group).toBe("deployed-in");
    expect(layout.graph.nodes.map((n) => n.id)).toEqual(["cluster", "pod", "svc"]);
    expect(layout.graph.edges).toHaveLength(0);
  });

  it("CLM-FR-DYAW: a node in two groups shows in the first and names the other", async () => {
    const layout = rendered(
      await readCalm(
        source({
          nodes: [node("svc"), node("east", "network"), node("west", "network")],
          relationships: [
            { "unique-id": "d1", "relationship-type": { "deployed-in": { container: "east", nodes: ["svc"] } } },
            { "unique-id": "d2", "relationship-type": { "deployed-in": { container: "west", nodes: ["svc"] } } },
          ],
        }),
      ),
    );
    const svc = layout.graph.nodes.find((n) => n.id === "svc")!;
    expect(svc.parent).toBe("east");
    expect(svc.alsoIn).toEqual(["WEST"]);
  });

  it("CLM-FR-EKSX: a containment loop fails the graph step and names the loop", async () => {
    const result = failed(
      await readCalm(
        source({
          nodes: [node("a", "system"), node("b", "system")],
          relationships: [
            { "unique-id": "r1", "relationship-type": { "composed-of": { container: "a", nodes: ["b"] } } },
            { "unique-id": "r2", "relationship-type": { "composed-of": { container: "b", nodes: ["a"] } } },
          ],
        }),
      ),
    );
    expect(result.step).toBe("graph");
    expect(result.summary).toBe("The architecture holds a containment loop.");
    expect(result.detail).toBe('"A" holds "B" holds "A"');
  });

  it("CLM-FR-EKSX: a group that holds itself is a loop", async () => {
    const result = failed(
      await readCalm(
        source({
          nodes: [node("a", "system")],
          relationships: [
            { "unique-id": "r1", "relationship-type": { "deployed-in": { container: "a", nodes: ["a"] } } },
          ],
        }),
      ),
    );
    expect(result.step).toBe("graph");
  });

  it("CLM-FR-ERXV: a connects relationship makes one edge with its label, protocol, and interfaces", async () => {
    const layout = rendered(
      await readCalm(
        source({
          nodes: [
            node("web", "webclient", { interfaces: [{ "unique-id": "web-out" }] }),
            node("api", "service", { interfaces: [{ "unique-id": "api-in" }] }),
          ],
          relationships: [
            {
              "unique-id": "call",
              description: "Calls",
              protocol: "HTTPS",
              "relationship-type": {
                connects: {
                  source: { node: "web", interfaces: ["web-out"] },
                  destination: { node: "api", interfaces: ["api-in"] },
                },
              },
            },
          ],
        }),
      ),
    );
    expect(layout.graph.edges).toEqual([
      {
        id: "call",
        relationshipId: "call",
        kind: "connects",
        source: "web",
        target: "api",
        label: "Calls",
        protocol: "HTTPS",
        sourceInterfaces: ["web-out"],
        targetInterfaces: ["api-in"],
      },
    ]);
  });

  it("CLM-FR-ESCS: an interacts relationship makes one edge from the actor to each node", async () => {
    const layout = rendered(
      await readCalm(
        source({
          nodes: [node("user", "actor"), node("a"), node("b")],
          relationships: [
            {
              "unique-id": "uses",
              description: "Uses",
              "relationship-type": { interacts: { actor: "user", nodes: ["a", "b"] } },
            },
          ],
        }),
      ),
    );
    expect(layout.graph.edges.map((e) => [e.id, e.kind, e.source, e.target, e.label])).toEqual([
      ["uses#0", "interacts", "user", "a", "Uses"],
      ["uses#1", "interacts", "user", "b", "Uses"],
    ]);
  });

  it("CLM-FR-ESMQ: an options relationship makes no edge and marks what its decisions name", async () => {
    const layout = rendered(
      await readCalm(
        source({
          nodes: [node("a"), node("b")],
          relationships: [
            connects("ab", "a", "b"),
            {
              "unique-id": "choice",
              "relationship-type": {
                options: [{ description: "Use B", nodes: ["b"], relationships: ["ab"] }],
              },
            },
          ],
        }),
      ),
    );
    expect(layout.graph.edges.map((e) => e.id)).toEqual(["ab"]);
    const b = layout.graph.nodes.find((n) => n.id === "b")!;
    expect(b.decisions).toEqual([{ relationshipId: "choice", description: "Use B" }]);
    const ab = layout.graph.relationships.find((r) => r.id === "ab")!;
    expect(ab.decisions).toEqual([{ relationshipId: "choice", description: "Use B" }]);
  });

  it("CLM-FR-ETEA: the controls of a node are read with their requirements", async () => {
    const layout = rendered(
      await readCalm(
        source({
          nodes: [
            node("a", "service", {
              controls: {
                "security-1": {
                  description: "TLS everywhere",
                  requirements: [
                    { "requirement-url": "https://example.com/req", config: { min: "1.2" } },
                  ],
                },
              },
            }),
          ],
        }),
      ),
    );
    expect(layout.graph.nodes[0].controls).toEqual([
      {
        id: "security-1",
        description: "TLS everywhere",
        requirements: [{ requirementUrl: "https://example.com/req", config: { min: "1.2" } }],
      },
    ]);
  });

  it("CLM-FR-DTYX, CLM-FR-GDML: ids and types that are names of the object prototype are ordinary values", async () => {
    const layout = rendered(
      await readCalm(
        source({
          nodes: [node("__proto__", "constructor"), node("constructor", "toString")],
          relationships: [connects("r", "__proto__", "constructor")],
        }),
      ),
    );
    expect(layout.graph.nodes.map((n) => n.shape)).toEqual(["generic", "generic"]);
    for (const id of ["__proto__", "constructor"]) {
      expect(Object.prototype.hasOwnProperty.call(layout.boxes, id)).toBe(true);
      expect(Number.isFinite(layout.boxes[id].x)).toBe(true);
    }
  });

  it("CLM-FR-HSUH: a document with no nodes gives a graph with no nodes", async () => {
    const layout = rendered(await readCalm(source({ nodes: [] })));
    expect(layout.graph.nodes).toHaveLength(0);
    expect(rendered(await readCalm("{}")).graph.nodes).toHaveLength(0);
  });
});

describe("CALM read: flows", () => {
  const flowSource = source({
    nodes: [node("user", "actor"), node("a"), node("b"), node("net", "network")],
    relationships: [
      { "unique-id": "uses", "relationship-type": { interacts: { actor: "user", nodes: ["a"] } } },
      connects("ab", "a", "b"),
      { "unique-id": "dep", "relationship-type": { "deployed-in": { container: "net", nodes: ["a", "b"] } } },
    ],
    flows: [
      {
        "unique-id": "f",
        name: "Flow",
        description: "A flow.",
        transitions: [
          { "relationship-unique-id": "ab", "sequence-number": 3, description: "Back", direction: "destination-to-source" },
          { "relationship-unique-id": "uses", "sequence-number": 1, description: "Start" },
          { "relationship-unique-id": "ab", "sequence-number": 2, description: "Forward" },
          { "relationship-unique-id": "dep", "sequence-number": 2, description: "Deploy" },
        ],
      },
    ],
  });

  it("CLM-FR-FDGM: steps go by sequence-number, and ties keep document order", async () => {
    const [flow] = rendered(await readCalm(flowSource)).graph.flows;
    expect(flow).toMatchObject({ id: "f", name: "Flow", description: "A flow." });
    expect(flow.steps.map((s) => [s.sequence, s.description])).toEqual([
      [1, "Start"],
      [2, "Forward"],
      [2, "Deploy"],
      [3, "Back"],
    ]);
  });

  it("CLM-FR-FKHM: a destination-to-source step goes from the destination to the source", async () => {
    const steps = rendered(await readCalm(flowSource)).graph.flows[0].steps;
    expect(steps[0]).toMatchObject({ from: ["user"], to: ["a"], reversed: false, edgeIds: ["uses"] });
    expect(steps[1]).toMatchObject({ from: ["a"], to: ["b"], reversed: false, edgeIds: ["ab"] });
    expect(steps[3]).toMatchObject({ from: ["b"], to: ["a"], reversed: true, edgeIds: ["ab"] });
  });

  it("CLM-FR-FSCM: a step whose relationship makes no edge highlights its nodes", async () => {
    const step = rendered(await readCalm(flowSource)).graph.flows[0].steps[2];
    expect(step.edgeIds).toEqual([]);
    expect(step.nodeIds).toEqual(["net", "a", "b"]);
  });
});

describe("CALM read: layout", () => {
  const grouped = source({
    nodes: [node("user", "actor"), node("a"), node("b"), node("net", "network")],
    relationships: [
      { "unique-id": "uses", "relationship-type": { interacts: { actor: "user", nodes: ["a"] } } },
      connects("ab", "a", "b"),
      { "unique-id": "dep", "relationship-type": { "deployed-in": { container: "net", nodes: ["a", "b"] } } },
    ],
  });

  it("CLM-FR-GDML: each member lies inside its group, and the flow runs left to right", async () => {
    const { boxes, width, height } = rendered(await readCalm(grouped));
    const net = boxes.net;
    for (const id of ["a", "b"]) {
      const box = boxes[id];
      expect(box.x).toBeGreaterThanOrEqual(0);
      expect(box.y).toBeGreaterThanOrEqual(0);
      expect(box.x + box.width).toBeLessThanOrEqual(net.width);
      expect(box.y + box.height).toBeLessThanOrEqual(net.height);
    }
    expect(boxes.user.x).toBeLessThan(net.x);
    expect(boxes.a.x).toBeLessThan(boxes.b.x);
    expect(width).toBeGreaterThanOrEqual(net.x + net.width);
    expect(height).toBeGreaterThan(0);
  });

  it("CLM-FR-GDML: the same graph always gets the same positions", async () => {
    const first = rendered(await readCalm(grouped)).boxes;
    resetCalmForTest();
    const second = rendered(await readCalm(grouped)).boxes;
    expect(second).toEqual(first);
  });
});

describe("CALM read: kept results and logs", () => {
  it("CLM-FR-IAHL: a kept result returns with no new read", async () => {
    const text = source({ nodes: [node("a")] });
    const first = await readCalm(text);
    expect(cachedCalm(text)).toBe(first);
    vi.mocked(logDebug).mockClear();
    expect(await readCalm(text)).toBe(first);
    expect(logDebug).not.toHaveBeenCalled();
  });

  it("CLM-FR-IAHL: at most 64 results are kept, and a shown result is never removed", async () => {
    const owner = {};
    const pinned = source({ nodes: [node("pinned")] });
    await readCalm(pinned);
    showCalm(owner, [pinned]);
    for (let i = 0; i < CALM_CACHE_LIMIT + 4; i++) await readCalm(`[${i}]`);
    expect(CALM_CACHE_LIMIT).toBe(64);
    // 1 pinned + 68 reads: the 5 oldest reads that do not show go.
    expect(cachedCalm(pinned)).toBeDefined();
    for (let i = 0; i < 5; i++) expect(cachedCalm(`[${i}]`)).toBeUndefined();
    for (let i = 5; i < CALM_CACHE_LIMIT + 4; i++) expect(cachedCalm(`[${i}]`)).toBeDefined();
    releaseCalm(owner);
  });

  it("CLM-FR-JIWR: a read logs its step, its node count, and its duration, and no document text", async () => {
    await readCalm(source({ nodes: [node("secret-node")] }));
    expect(logDebug).toHaveBeenCalledTimes(1);
    const [domains, message, fields] = vi.mocked(logDebug).mock.calls[0];
    expect(domains).toEqual(["frontend"]);
    expect(message).toBe("a CALM read ended");
    expect(fields).toEqual({ step: "done", nodes: 1, durationMs: expect.any(Number) });
    expect(JSON.stringify(fields)).not.toContain("secret");
  });

  it("CLM-FR-JIWR: a failed read logs the step that ended it", async () => {
    await readCalm("not json");
    expect(vi.mocked(logDebug).mock.calls[0][2]).toMatchObject({ step: "parse", nodes: 0 });
  });
});
