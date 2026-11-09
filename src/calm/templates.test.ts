import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
}));

import { readCalm, resetCalmForTest } from "./read";
import { CALM_SCHEMA_URL, CALM_TEMPLATES } from "./templates";
import type { CalmLayout } from "./types";

beforeEach(() => resetCalmForTest());

async function layoutOf(key: string): Promise<CalmLayout> {
  const template = CALM_TEMPLATES.find((t) => t.key === key)!;
  const result = await readCalm(template.source);
  if (result.state !== "rendered") throw new Error(`${key}: ${result.summary} ${result.detail}`);
  return result.layout;
}

describe("CALM templates", () => {
  it("CAD-FR-JWXC: the menu lists the four templates in order", () => {
    expect(CALM_TEMPLATES.map((t) => t.name)).toEqual([
      "Minimal",
      "Three-tier web application",
      "Deployed services",
      "Business flow",
    ]);
  });

  it.each(CALM_TEMPLATES.map((template) => [template.name, template] as const))(
    "CAD-FR-JWXC: the %s template declares the CALM 1.3 schema and reads as a diagram",
    async (_name, template) => {
      expect(JSON.parse(template.source).$schema).toBe(CALM_SCHEMA_URL);
      expect(CALM_SCHEMA_URL).toBe("https://calm.finos.org/release/1.3/meta/calm.json");
      const result = await readCalm(template.source);
      expect(result.state, `${template.name}: ${JSON.stringify(result)}`).toBe("rendered");
    },
    15_000,
  );

  it("CAD-FR-KSAQ: the three-tier template holds connects relationships with protocols", async () => {
    const { graph } = await layoutOf("three-tier");
    const protocols = graph.edges.filter((e) => e.kind === "connects").map((e) => e.protocol);
    expect(protocols.length).toBeGreaterThanOrEqual(2);
    expect(protocols.every((p) => typeof p === "string" && p.length > 0)).toBe(true);
  });

  it("CAD-FR-KSAQ: the deployed services template holds a deployed-in group", async () => {
    const { graph } = await layoutOf("deployed");
    expect(graph.nodes.some((n) => n.group === "deployed-in")).toBe(true);
  });

  it("CAD-FR-KSAQ: the business flow template holds a flow of at least three transitions", async () => {
    const { graph } = await layoutOf("flow");
    expect(graph.flows).toHaveLength(1);
    expect(graph.flows[0].steps.length).toBeGreaterThanOrEqual(3);
  });

  it.each(CALM_TEMPLATES.map((template) => [template.name, template.key] as const))(
    "CLM-FR-GDML, CLM-FR-ERXV: in the %s template no node covers an edge label, and each edge route ends on its nodes",
    async (_name, key) => {
      const layout = await layoutOf(key);
      const absolute = new Map<string, { x: number; y: number; width: number; height: number }>();
      for (const node of layout.graph.nodes) {
        const box = layout.boxes[node.id];
        const holder = node.parent === undefined ? undefined : absolute.get(node.parent);
        absolute.set(node.id, { ...box, x: box.x + (holder?.x ?? 0), y: box.y + (holder?.y ?? 0) });
      }
      const overlaps = (a: { x: number; y: number; width: number; height: number }, b: typeof a) =>
        a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;
      for (const edge of layout.graph.edges) {
        const route = layout.routes[edge.id];
        expect(route, edge.id).toBeDefined();
        const first = route.points[0];
        const last = route.points[route.points.length - 1];
        const near = (p: { x: number; y: number }, box: { x: number; y: number; width: number; height: number }) =>
          p.x >= box.x - 1 && p.x <= box.x + box.width + 1 && p.y >= box.y - 1 && p.y <= box.y + box.height + 1;
        expect(near(first, absolute.get(edge.source)!), `${edge.id} start`).toBe(true);
        expect(near(last, absolute.get(edge.target)!), `${edge.id} end`).toBe(true);
        if (!route.label) continue;
        for (const node of layout.graph.nodes) {
          if (node.group) continue;
          expect(overlaps(route.label, absolute.get(node.id)!), `${edge.id} label over ${node.id}`).toBe(false);
        }
      }
    },
    15_000,
  );
});
