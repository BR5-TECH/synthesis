/**
 * CLM-FR-GDML, CLM-FR-GFZK: the layout step of a read. It places the graph
 * with the layered algorithm of ELK, from left to right. Groups hold their
 * members, so the layout runs over the whole hierarchy at once.
 */
import type { Box, CalmEdgeView, CalmGraph, CalmLayout, CalmNodeView, CalmRoute } from "./types";

/** The part of an ELK graph that the layout step reads and writes. */
export interface ElkNode {
  id: string;
  width?: number;
  height?: number;
  x?: number;
  y?: number;
  layoutOptions?: Record<string, string>;
  children?: ElkNode[];
  edges?: ElkEdge[];
}

interface ElkPoint {
  x: number;
  y: number;
}

/** The part of an ELK edge that the layout step reads and writes. */
export interface ElkEdge {
  id: string;
  sources: string[];
  targets: string[];
  labels?: Array<{ id: string; text: string; width: number; height: number; x?: number; y?: number }>;
  sections?: Array<{ startPoint: ElkPoint; endPoint: ElkPoint; bendPoints?: ElkPoint[] }>;
}

/** The layout engine, as `elkjs` gives it. */
export interface ElkEngine {
  layout(graph: ElkNode): Promise<ElkNode>;
}

/** The size of a node that is not a group. */
export const NODE_WIDTH = 200;
const NODE_BASE_HEIGHT = 58;
const LINE_HEIGHT = 18;
/** CLM-FR-ETEA: at most three interfaces show on a node, and then "+N more". */
export const SHOWN_INTERFACES = 3;
/** The room above the members of a group, for its name and its type. */
export const GROUP_HEADER = 46;
const GROUP_PADDING = 20;

/** The height of a node that is not a group, from what it shows. */
export function nodeHeight(node: CalmNodeView): number {
  const count = node.interfaces.length;
  const lines = Math.min(count, SHOWN_INTERFACES) + (count > SHOWN_INTERFACES ? 1 : 0);
  return NODE_BASE_HEIGHT + lines * LINE_HEIGHT + (node.alsoIn.length > 0 ? LINE_HEIGHT : 0);
}

const ROOT_OPTIONS: Record<string, string> = {
  "elk.algorithm": "layered",
  "elk.direction": "RIGHT",
  "elk.hierarchyHandling": "INCLUDE_CHILDREN",
  "elk.spacing.nodeNode": "40",
  "elk.layered.spacing.nodeNodeBetweenLayers": "50",
  "elk.layered.spacing.edgeNodeBetweenLayers": "20",
  "elk.edgeRouting": "ORTHOGONAL",
  "elk.edgeLabels.placement": "CENTER",
  "elk.spacing.edgeLabel": "4",
  // Edge routes and label boxes come back in the coordinates of the diagram.
  "elk.json.edgeCoords": "ROOT",
  "elk.spacing.edgeNode": "24",
  "elk.layered.nodePlacement.strategy": "BRANDES_KOEPF",
  "elk.padding": "[top=24,left=24,bottom=24,right=24]",
};

const GROUP_OPTIONS: Record<string, string> = {
  "elk.padding": `[top=${GROUP_HEADER},left=${GROUP_PADDING},bottom=${GROUP_PADDING},right=${GROUP_PADDING}]`,
  "elk.nodeSize.constraints": "MINIMUM_SIZE",
  "elk.nodeSize.minimum": `(${NODE_WIDTH}, ${NODE_BASE_HEIGHT + GROUP_HEADER})`,
};

/** The height of an edge label (CLM-FR-ERXV). */
export const LABEL_HEIGHT = 20;
const LABEL_MAX_WIDTH = 240;
const STEP_WIDTH = 18;

/**
 * CLM-FR-ERXV, CAD-FR-KSEJ: the room that the label of an edge takes, so the
 * layout keeps nodes off it: its description, its protocol chip, and a circle
 * for each flow step that can show on it. Zero when the edge has no label.
 */
export function labelWidth(edge: CalmEdgeView, steps: number): number {
  const parts: number[] = [];
  if (edge.label !== undefined) parts.push(edge.label.length * 5.8);
  if (edge.protocol !== undefined) parts.push(edge.protocol.length * 6.2 + 10);
  for (let i = 0; i < steps; i++) parts.push(STEP_WIDTH);
  if (parts.length === 0) return 0;
  const content = parts.reduce((sum, part) => sum + part, 0) + (parts.length - 1) * 4;
  return Math.min(LABEL_MAX_WIDTH, Math.ceil(content + 14));
}

/** The number of flow steps that can show on each edge, the most of any flow. */
function stepsByEdge(graph: CalmGraph): Map<string, number> {
  const most = new Map<string, number>();
  for (const flow of graph.flows) {
    const count = new Map<string, number>();
    for (const step of flow.steps) {
      for (const id of step.edgeIds) count.set(id, (count.get(id) ?? 0) + 1);
    }
    for (const [id, n] of count) most.set(id, Math.max(most.get(id) ?? 0, n));
  }
  return most;
}

/** The ELK graph of a diagram graph. */
export function elkGraphOf(graph: CalmGraph): ElkNode {
  const byId = new Map<string, ElkNode>();
  const roots: ElkNode[] = [];
  for (const node of graph.nodes) {
    const elk: ElkNode = { id: node.id };
    if (node.group) {
      elk.children = [];
      elk.layoutOptions = GROUP_OPTIONS;
    } else {
      elk.width = NODE_WIDTH;
      elk.height = nodeHeight(node);
    }
    byId.set(node.id, elk);
  }
  // `graph.nodes` lists a group before its members.
  for (const node of graph.nodes) {
    const elk = byId.get(node.id)!;
    const holder = node.parent === undefined ? undefined : byId.get(node.parent);
    if (holder) (holder.children ??= []).push(elk);
    else roots.push(elk);
  }
  for (const elk of byId.values()) {
    // A group that lists no member still needs a size.
    if (elk.children && elk.children.length === 0) {
      delete elk.children;
      elk.width = NODE_WIDTH;
      elk.height = NODE_BASE_HEIGHT + GROUP_HEADER;
    }
  }
  const steps = stepsByEdge(graph);
  const parentOf = new Map(graph.nodes.map((n) => [n.id, n.parent]));
  const ancestors = (id: string): Set<string> => {
    const out = new Set<string>();
    for (let at = parentOf.get(id); at !== undefined; at = parentOf.get(at)) out.add(at);
    return out;
  };
  // An edge between a node and a group that holds it, and an edge from a node
  // to itself, do not change the placement. The canvas still draws them.
  const edges = graph.edges
    .filter(
      (edge) =>
        edge.source !== edge.target &&
        !ancestors(edge.source).has(edge.target) &&
        !ancestors(edge.target).has(edge.source),
    )
    .map((edge) => {
      const width = labelWidth(edge, steps.get(edge.id) ?? 0);
      const elk: ElkEdge = { id: edge.id, sources: [edge.source], targets: [edge.target] };
      if (width > 0) {
        elk.labels = [{ id: `${edge.id}:label`, text: edge.label ?? "", width, height: LABEL_HEIGHT }];
      }
      return elk;
    });
  return { id: "calm-root", layoutOptions: ROOT_OPTIONS, children: roots, edges };
}

/** The boxes of each node of a laid-out ELK graph, relative to their holder. */
function collectBoxes(node: ElkNode, boxes: Record<string, Box>): void {
  for (const child of node.children ?? []) {
    boxes[child.id] = {
      x: Math.round(child.x ?? 0),
      y: Math.round(child.y ?? 0),
      width: Math.round(child.width ?? NODE_WIDTH),
      height: Math.round(child.height ?? NODE_BASE_HEIGHT),
    };
    collectBoxes(child, boxes);
  }
}

/** CLM-FR-GDML: the diagram graph with a place for each node. */
export async function layoutGraph(engine: ElkEngine, graph: CalmGraph): Promise<CalmLayout> {
  const placed = await engine.layout(elkGraphOf(graph));
  // A `unique-id` such as `__proto__` or `constructor` is an own key here.
  const boxes: Record<string, Box> = Object.create(null);
  collectBoxes(placed, boxes);
  const routes: Record<string, CalmRoute> = Object.create(null);
  for (const edge of placed.edges ?? []) {
    const section = edge.sections?.[0];
    if (!section) continue;
    const route: CalmRoute = {
      points: [section.startPoint, ...(section.bendPoints ?? []), section.endPoint].map((p) => ({
        x: Math.round(p.x),
        y: Math.round(p.y),
      })),
    };
    const label = edge.labels?.[0];
    if (label && label.x !== undefined && label.y !== undefined) {
      route.label = {
        x: Math.round(label.x),
        y: Math.round(label.y),
        width: label.width,
        height: label.height,
      };
    }
    routes[edge.id] = route;
  }
  return {
    graph,
    boxes,
    routes,
    width: Math.max(Math.round(placed.width ?? 0), 1),
    height: Math.max(Math.round(placed.height ?? 0), 1),
  };
}
