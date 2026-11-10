/**
 * The print drawing of a CALM diagram (`CAD-calm-architecture-diagrams.md`
 * CAD-FR-QHEJ, CAD-FR-FONB, CAD-FR-VBDT).
 */
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";

vi.mock("../../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
}));

import { readCalm } from "../../calm/read";
import { CALM_TEMPLATES } from "../../calm/templates";
import type { CalmEdgeView, CalmLayout, CalmNodeView } from "../../calm/types";
import { routePath } from "./geometry";
import { estimateWidth, fitText } from "./printMeasure";
import { PRINT_TOKENS } from "./printPalette";
import { calmPrintSvg, escapeXml } from "./printSvg";

beforeAll(async () => {
  // The first read loads the CALM libraries.
  await readCalm(CALM_TEMPLATES[0].source);
}, 30_000);

afterEach(() => {
  document.documentElement.removeAttribute("data-theme");
});

async function templateLayout(key: string): Promise<CalmLayout> {
  const result = await readCalm(CALM_TEMPLATES.find((t) => t.key === key)!.source);
  if (result.state !== "rendered") throw new Error(result.summary);
  return result.layout;
}

function nodeView(over: Partial<CalmNodeView> & { id: string }): CalmNodeView {
  return {
    name: over.id,
    nodeType: "service",
    shape: "service",
    description: "",
    interfaces: [],
    controls: [],
    metadata: undefined,
    alsoIn: [],
    decisions: [],
    ...over,
  };
}

function edgeView(over: Partial<CalmEdgeView> & { id: string; source: string; target: string }): CalmEdgeView {
  return {
    relationshipId: over.id,
    kind: "connects",
    sourceInterfaces: [],
    targetInterfaces: [],
    ...over,
  };
}

/** A layout of `nodes` side by side, 200 by 80 each, and `edges` routed between them. */
function layoutOf(nodes: CalmNodeView[], edges: CalmEdgeView[] = []): CalmLayout {
  const boxes: CalmLayout["boxes"] = Object.create(null);
  nodes.forEach((node, index) => {
    boxes[node.id] = { x: 20 + index * 260, y: 20, width: 200, height: 80 };
  });
  const routes: CalmLayout["routes"] = Object.create(null);
  for (const edge of edges) {
    const from = boxes[edge.source];
    const to = boxes[edge.target];
    routes[edge.id] = {
      points: [
        { x: from.x + from.width, y: 60 },
        { x: from.x + from.width + 30, y: 60 },
        { x: from.x + from.width + 30, y: 50 },
        { x: to.x, y: 50 },
      ],
      label: { x: from.x + from.width + 5, y: 70, width: 50, height: 20 },
    };
  }
  return {
    graph: { nodes, edges, relationships: [], flows: [], metadata: undefined, controls: [], adrs: [] },
    boxes,
    routes,
    width: 20 + nodes.length * 260,
    height: 140,
  };
}

const svgOf = (layout: CalmLayout) => calmPrintSvg(layout, estimateWidth)!;

/** The parsed drawing; a parse error fails the test. */
function parsed(svg: string): Document {
  const doc = new DOMParser().parseFromString(svg, "image/svg+xml");
  expect(doc.getElementsByTagName("parsererror")).toHaveLength(0);
  return doc;
}

const texts = (doc: Document) => [...doc.getElementsByTagName("text")].map((t) => t.textContent ?? "");

describe("the print drawing", () => {
  it(
    "CAD-FR-QHEJ: the three-tier template draws its nodes, its types, its labels, its chips, and its interface labels at the size of the diagram",
    async () => {
      const layout = await templateLayout("three-tier");
      const svg = svgOf(layout);
      const doc = parsed(svg);
      const root = doc.documentElement;
      expect(root.getAttribute("width")).toBe(String(Math.ceil(layout.width)));
      expect(root.getAttribute("height")).toBe(String(Math.ceil(layout.height)));
      const shown = texts(doc);
      for (const name of ["Customer", "Web client", "Shop API", "Shop database"]) expect(shown).toContain(name);
      for (const type of ["actor", "webclient", "service", "database"]) expect(shown).toContain(type);
      for (const label of ["Browses", "Calls the API", "Reads and writes", "HTTPS", "JDBC"]) {
        expect(shown).toContain(label);
      }
      // CLM-FR-ERXV: the destination interface by its end, and in the node list.
      expect(shown.filter((t) => t === "shop-api-http").length).toBeGreaterThanOrEqual(2);
    },
    15_000,
  );

  it(
    "CAD-FR-QHEJ: each routed edge follows the route that the canvas draws, with the arrow, and an interaction is dashed",
    async () => {
      const layout = await templateLayout("three-tier");
      const svg = svgOf(layout);
      for (const edge of layout.graph.edges) {
        const route = layout.routes[edge.id];
        expect(svg).toContain(`d="${routePath(route.points)}"`);
      }
      const doc = parsed(svg);
      const lines = [...doc.getElementsByTagName("path")].filter((p) => p.getAttribute("marker-end"));
      expect(lines).toHaveLength(layout.graph.edges.length);
      expect(lines.every((p) => p.getAttribute("marker-end") === "url(#calm-arrow)")).toBe(true);
      expect(lines.filter((p) => p.getAttribute("stroke-dasharray") === "5 4")).toHaveLength(
        layout.graph.edges.filter((e) => e.kind === "interacts").length,
      );
      expect(doc.getElementById("calm-arrow")?.getAttribute("orient")).toBe("auto-start-reverse");
    },
    15_000,
  );

  it(
    "CAD-FR-QHEJ: a deployed-in group is a dashed box under its members, with its name, type, and kind",
    async () => {
      const layout = await templateLayout("deployed");
      const doc = parsed(svgOf(layout));
      const group = layout.graph.nodes.find((n) => n.group === "deployed-in")!;
      const shown = texts(doc);
      expect(shown).toContain(group.name);
      expect(shown).toContain("deployed-in");
      const dashed = [...doc.getElementsByTagName("path")].filter(
        (p) => p.getAttribute("stroke-dasharray") === "6 4",
      );
      expect(dashed).toHaveLength(layout.graph.nodes.filter((n) => n.group === "deployed-in").length);
      // The group comes before its members, so the members draw over it.
      const order = shown.indexOf(group.name);
      const member = layout.graph.nodes.find((n) => n.parent === group.id)!;
      expect(order).toBeLessThan(shown.indexOf(member.name));
    },
    15_000,
  );

  it("CAD-FR-QHEJ: a composed-of group is a solid box on a recessed ground", () => {
    const layout = layoutOf([nodeView({ id: "whole", group: "composed-of", shape: "system", nodeType: "system" })]);
    const svg = svgOf(layout);
    expect(svg).toContain(`fill="${PRINT_TOKENS["--bg-sunken"]}" fill-opacity="0.55" stroke="${PRINT_TOKENS["--border-2"]}"`);
    expect(svg).toContain(">composed-of<");
  });

  it("CAD-FR-QHEJ: each shape draws its outline with the spine colour of its family and its glyph", () => {
    const shapes = [
      ["actor", "--code-keyword"],
      ["system", "--code-type"],
      ["service", "--code-function"],
      ["database", "--code-number"],
      ["webclient", "--code-string"],
      ["network", "--code-meta"],
      ["dataAsset", "--code-number"],
      ["generic", "--fg-4"],
    ] as const;
    for (const [shape, spine] of shapes) {
      const svg = svgOf(layoutOf([nodeView({ id: `n-${shape}`, shape, nodeType: shape })]));
      expect(svg, shape).toContain(`width="3" height="80" fill="${PRINT_TOKENS[spine]}"`);
      expect(svg, shape).toContain(`stroke="${PRINT_TOKENS[spine]}" stroke-width="1.4"`);
    }
    // The generic outline is dashed, and the network outline is double.
    expect(svgOf(layoutOf([nodeView({ id: "g", shape: "generic" })]))).toContain('stroke-dasharray="4 3"');
    const network = parsed(svgOf(layoutOf([nodeView({ id: "w", shape: "network" })])));
    const outlines = [...network.getElementsByTagName("path")].filter((p) => p.getAttribute("fill") === "none");
    expect(outlines.filter((p) => p.getAttribute("stroke") === PRINT_TOKENS["--border-3"])).toHaveLength(2);
  });

  it("CAD-FR-QHEJ: a node shows three interfaces and \"+N more\", the markers, and the groups it is also in", () => {
    const node = nodeView({
      id: "api",
      interfaces: ["a", "b", "c", "d", "e"].map((id) => ({ "unique-id": `if-${id}` })),
      controls: [{ id: "c1", description: "", requirements: [] }, { id: "c2", description: "", requirements: [] }],
      decisions: [{ relationshipId: "o", description: "" }],
      alsoIn: ["Zone B"],
    });
    const shown = texts(parsed(svgOf(layoutOf([node]))));
    expect(shown).toEqual(expect.arrayContaining(["if-a", "if-b", "if-c", "+2 more", "2", "1", "Also in Zone B"]));
    expect(shown).not.toContain("if-d");
  });

  it("CAD-FR-QHEJ: an edge with no route runs between the borders of its nodes, with its label", () => {
    const layout = layoutOf(
      [nodeView({ id: "a" }), nodeView({ id: "b" })],
      [edgeView({ id: "e", source: "a", target: "b", label: "Talks", protocol: "TCP" })],
    );
    delete layout.routes.e;
    const doc = parsed(svgOf(layout));
    const line = [...doc.getElementsByTagName("path")].find((p) => p.getAttribute("marker-end"))!;
    expect(line.getAttribute("d")).toMatch(/^M ?220[ ,]60/);
    expect(texts(doc)).toEqual(expect.arrayContaining(["Talks", "TCP"]));
  });

  it("CAD-FR-VBDT: CALM text is escaped text and never markup, and the drawing refers to nothing outside itself", () => {
    const hostile = `<script>alert("x")</script> & 'y' <image href="file:///etc/passwd"/>`;
    const layout = layoutOf(
      [
        nodeView({ id: "a", name: hostile, nodeType: `<b>type</b>`, interfaces: [{ "unique-id": hostile }] }),
        nodeView({ id: "b", name: "\u0001Bee" }),
      ],
      [edgeView({ id: "e", source: "a", target: "b", protocol: "</text>" })],
    );
    const svg = svgOf(layout);
    const doc = parsed(svg);
    expect(doc.getElementsByTagName("script")).toHaveLength(0);
    expect(doc.getElementsByTagName("image")).toHaveLength(0);
    expect(doc.getElementsByTagName("b")).toHaveLength(0);
    expect(texts(doc).some((t) => t.startsWith("<script>"))).toBe(true);
    expect(texts(doc)).toContain("</text>");
    expect(texts(doc)).toContain("Bee");
    // Only the namespace names a URL, and every `url(...)` is a local reference.
    expect(svg.replace('xmlns="http://www.w3.org/2000/svg"', "")).not.toMatch(/https?:|file:|href=/);
    expect([...svg.matchAll(/url\(([^)]*)\)/g)].every((m) => m[1].startsWith("#"))).toBe(true);
  });

  it("CAD-FR-VBDT: escapeXml escapes the markup characters and drops the control characters", () => {
    expect(escapeXml(`a<b>&"c"'d'\u0000\u0007\u001F\tz`)).toBe("a&lt;b&gt;&amp;&quot;c&quot;&apos;d&apos;\tz");
  });

  it("CAD-FR-VBDT: a name wider than its place ends with an ellipsis", () => {
    const long = "A very long service name that does not fit on one card";
    const shown = texts(parsed(svgOf(layoutOf([nodeView({ id: "a", name: long })]))));
    const name = shown.find((t) => t.startsWith("A very"))!;
    expect(name.endsWith("…")).toBe(true);
    expect(name.length).toBeLessThan(long.length);
    expect(fitText("short", 100, { size: 10 }, estimateWidth)).toBe("short");
    expect(fitText("abcdef", 0, { size: 10 }, estimateWidth)).toBe("…");
  });

  it("CAD-FR-VBDT: a graph with no nodes gives no drawing", () => {
    expect(calmPrintSvg(layoutOf([]), estimateWidth)).toBeNull();
  });

  it(
    "CAD-FR-FONB: with the dark theme active, the drawing holds only the fixed light tokens and the bundled faces",
    async () => {
      document.documentElement.setAttribute("data-theme", "dark");
      const svg = svgOf(await templateLayout("three-tier"));
      expect(svg).toContain(`fill="${PRINT_TOKENS["--bg-canvas"]}"`);
      for (const dark of ["#1B1F27", "#ECEAE5", "#4A4F5E", "var("]) expect(svg).not.toContain(dark);
      const families = new Set([...svg.matchAll(/font-family="([^"]*)"/g)].map((m) => m[1].split(",")[0]));
      expect([...families].sort()).toEqual(["Liberation Mono", "Liberation Sans"]);
    },
    15_000,
  );
});
