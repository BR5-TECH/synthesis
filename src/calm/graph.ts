/**
 * CLM-FR-DLUV – CLM-FR-FSCM: the graph step of a read. It turns a document
 * that passed the schema step and the references step into a diagram graph.
 */
import { field, listOf, relationshipTypeOf, textOf, textsOf } from "./references";
import {
  CALM_ERROR,
  type CalmControl,
  type CalmDecision,
  type CalmEdgeView,
  type CalmError,
  type CalmFlowStep,
  type CalmFlowView,
  type CalmGraph,
  type CalmGroupKind,
  type CalmNodeView,
  type CalmRelationshipType,
  type CalmRelationshipView,
  type CalmShape,
  type JsonObject,
} from "./types";

/** CLM-FR-DTYX: the shape of each `node-type` of the CALM enum. */
const SHAPES: Record<string, CalmShape> = {
  actor: "actor",
  system: "system",
  ecosystem: "system",
  service: "service",
  database: "database",
  webclient: "webclient",
  network: "network",
  ldap: "network",
  "data-asset": "dataAsset",
};

/** CLM-FR-DTYX: the shape of a node, from its `node-type`. */
export function shapeOf(nodeType: string): CalmShape {
  // A type such as `constructor` names no shape, not a key of the prototype.
  return Object.prototype.hasOwnProperty.call(SHAPES, nodeType) ? SHAPES[nodeType] : "generic";
}

/** The controls of an item (`control.json` `controls`), in document order. */
export function controlsOf(value: unknown): CalmControl[] {
  const controls = field(value, "controls");
  if (controls === null || typeof controls !== "object" || Array.isArray(controls)) return [];
  return Object.entries(controls as JsonObject).map(([id, control]) => ({
    id,
    description: textOf(control, "description") ?? "",
    requirements: listOf(control, "requirements").map((requirement) => {
      const out: CalmControl["requirements"][number] = {
        requirementUrl: textOf(requirement, "requirement-url") ?? "",
      };
      const configUrl = textOf(requirement, "config-url");
      if (configUrl !== undefined) out.configUrl = configUrl;
      const config = field(requirement, "config");
      if (config !== undefined) out.config = config;
      return out;
    }),
  }));
}

const isObject = (value: unknown): value is JsonObject =>
  value !== null && typeof value === "object" && !Array.isArray(value);

interface Containment {
  container: string;
  member: string;
  kind: CalmGroupKind;
}

/**
 * CLM-FR-EKSX: the nodes of one containment loop, in loop order, or null when
 * containment makes no loop.
 */
export function containmentLoop(links: readonly Containment[]): string[] | null {
  const members = new Map<string, string[]>();
  for (const { container, member } of links) {
    const list = members.get(container) ?? [];
    list.push(member);
    members.set(container, list);
  }
  const state = new Map<string, "open" | "done">();
  const path: string[] = [];
  const visit = (id: string): string[] | null => {
    state.set(id, "open");
    path.push(id);
    for (const next of members.get(id) ?? []) {
      const seen = state.get(next);
      if (seen === "open") return path.slice(path.indexOf(next));
      if (seen === undefined) {
        const loop = visit(next);
        if (loop) return loop;
      }
    }
    path.pop();
    state.set(id, "done");
    return null;
  };
  for (const { container } of links) {
    if (state.has(container)) continue;
    const loop = visit(container);
    if (loop) return loop;
  }
  return null;
}

/** CLM-FR-DLUV – CLM-FR-FSCM: the graph of a valid document, or its loop error. */
export function buildGraph(document: JsonObject): CalmGraph | CalmError {
  const rawNodes = listOf(document, "nodes").filter(isObject);
  const rawRelationships = listOf(document, "relationships").filter(isObject);

  const names = new Map<string, string>();
  for (const node of rawNodes) {
    const id = textOf(node, "unique-id") ?? "";
    names.set(id, textOf(node, "name") ?? id);
  }

  // CLM-FR-DWLY: containment, in document order.
  const links: Containment[] = [];
  for (const rel of rawRelationships) {
    const kind = relationshipTypeOf(rel);
    if (kind !== "deployed-in" && kind !== "composed-of") continue;
    const body = field(field(rel, "relationship-type"), kind);
    const container = textOf(body, "container");
    if (container === undefined) continue;
    for (const member of textsOf(body, "nodes")) links.push({ container, member, kind });
  }
  const loop = containmentLoop(links);
  if (loop) {
    return {
      state: "error",
      step: "graph",
      summary: CALM_ERROR.graph,
      detail: [...loop, loop[0]].map((id) => `"${names.get(id) ?? id}"`).join(" holds "),
    };
  }
  const parent = new Map<string, string>();
  const alsoIn = new Map<string, string[]>();
  const groupKind = new Map<string, CalmGroupKind>();
  for (const { container, member, kind } of links) {
    if (!groupKind.has(container)) groupKind.set(container, kind);
    const first = parent.get(member);
    if (first === undefined) {
      parent.set(member, container);
    } else if (first !== container) {
      // CLM-FR-DYAW: the node shows in its first group; a badge names the others.
      const others = alsoIn.get(member) ?? [];
      const name = names.get(container) ?? container;
      if (!others.includes(name)) others.push(name);
      alsoIn.set(member, others);
    }
  }

  // CLM-FR-ESMQ: the decisions of each `options` relationship, by the item they name.
  const decisions = new Map<string, CalmDecision[]>();
  const addDecision = (key: string, decision: CalmDecision) => {
    const list = decisions.get(key) ?? [];
    list.push(decision);
    decisions.set(key, list);
  };
  for (const rel of rawRelationships) {
    if (relationshipTypeOf(rel) !== "options") continue;
    const relationshipId = textOf(rel, "unique-id") ?? "";
    const body = field(field(rel, "relationship-type"), "options");
    if (!Array.isArray(body)) continue;
    for (const item of body) {
      const decision = { relationshipId, description: textOf(item, "description") ?? "" };
      for (const id of textsOf(item, "nodes")) addDecision(`node:${id}`, decision);
      for (const id of textsOf(item, "relationships")) addDecision(`rel:${id}`, decision);
    }
  }

  const nodes: CalmNodeView[] = rawNodes.map((node) => {
    const id = textOf(node, "unique-id") ?? "";
    const nodeType = textOf(node, "node-type") ?? "";
    const view: CalmNodeView = {
      id,
      name: textOf(node, "name") ?? id,
      nodeType,
      shape: shapeOf(nodeType),
      description: textOf(node, "description") ?? "",
      interfaces: listOf(node, "interfaces").filter(isObject),
      controls: controlsOf(node),
      metadata: field(node, "metadata"),
      alsoIn: alsoIn.get(id) ?? [],
      decisions: decisions.get(`node:${id}`) ?? [],
    };
    const holder = parent.get(id);
    if (holder !== undefined) view.parent = holder;
    const kind = groupKind.get(id);
    if (kind !== undefined) view.group = kind;
    return view;
  });

  // A group comes before the nodes it holds; otherwise document order stays.
  const depth = (id: string): number => {
    let d = 0;
    for (let at = parent.get(id); at !== undefined; at = parent.get(at)) d++;
    return d;
  };
  const depths = new Map(nodes.map((node) => [node.id, depth(node.id)]));
  nodes.sort((a, b) => (depths.get(a.id) ?? 0) - (depths.get(b.id) ?? 0));

  const edges: CalmEdgeView[] = [];
  const relationships: CalmRelationshipView[] = rawRelationships.map((rel) => {
    const id = textOf(rel, "unique-id") ?? "";
    const type = (relationshipTypeOf(rel) ?? "connects") as CalmRelationshipType;
    const body = field(field(rel, "relationship-type"), type);
    const description = textOf(rel, "description") ?? "";
    const protocol = textOf(rel, "protocol");
    const view: CalmRelationshipView = {
      id,
      type,
      description,
      controls: controlsOf(rel),
      metadata: field(rel, "metadata"),
      edgeIds: [],
      nodeIds: [],
      sourceInterfaces: [],
      targetInterfaces: [],
      decisions: decisions.get(`rel:${id}`) ?? [],
    };
    if (protocol !== undefined) view.protocol = protocol;
    const label = description || undefined;
    if (type === "connects") {
      const source = textOf(field(body, "source"), "node") ?? "";
      const target = textOf(field(body, "destination"), "node") ?? "";
      view.sourceInterfaces = textsOf(field(body, "source"), "interfaces");
      view.targetInterfaces = textsOf(field(body, "destination"), "interfaces");
      view.nodeIds = [source, target];
      view.edgeIds = [id];
      edges.push({
        id,
        relationshipId: id,
        kind: "connects",
        source,
        target,
        label,
        protocol,
        sourceInterfaces: view.sourceInterfaces,
        targetInterfaces: view.targetInterfaces,
      });
    } else if (type === "interacts") {
      const actor = textOf(body, "actor") ?? "";
      const targets = textsOf(body, "nodes");
      view.nodeIds = [actor, ...targets];
      targets.forEach((target, index) => {
        const edgeId = targets.length === 1 ? id : `${id}#${index}`;
        view.edgeIds.push(edgeId);
        edges.push({
          id: edgeId,
          relationshipId: id,
          kind: "interacts",
          source: actor,
          target,
          label,
          sourceInterfaces: [],
          targetInterfaces: [],
        });
      });
    } else if (type === "deployed-in" || type === "composed-of") {
      const container = textOf(body, "container");
      view.nodeIds = [...(container === undefined ? [] : [container]), ...textsOf(body, "nodes")];
    } else if (type === "options" && Array.isArray(body)) {
      view.nodeIds = [...new Set(body.flatMap((item) => textsOf(item, "nodes")))];
    }
    return view;
  });

  const byId = new Map(relationships.map((rel) => [rel.id, rel]));
  const flows: CalmFlowView[] = listOf(document, "flows")
    .filter(isObject)
    .map((flow) => {
      const transitions = listOf(flow, "transitions").filter(isObject);
      const steps = transitions
        .map((transition, order) => ({ transition, order }))
        .sort((a, b) => {
          const sa = Number(field(a.transition, "sequence-number") ?? 0);
          const sb = Number(field(b.transition, "sequence-number") ?? 0);
          return sa - sb || a.order - b.order;
        })
        .map(({ transition }) => stepOf(transition, byId));
      return {
        id: textOf(flow, "unique-id") ?? "",
        name: textOf(flow, "name") ?? "",
        description: textOf(flow, "description") ?? "",
        steps,
      };
    });

  return {
    nodes,
    edges,
    relationships,
    flows,
    metadata: field(document, "metadata"),
    controls: controlsOf(document),
    adrs: textsOf(document, "adrs"),
  };
}

/** CLM-FR-FKHM, CLM-FR-FSCM: one step of a flow. */
function stepOf(transition: JsonObject, byId: Map<string, CalmRelationshipView>): CalmFlowStep {
  const relationshipId = textOf(transition, "relationship-unique-id") ?? "";
  const rel = byId.get(relationshipId);
  const reversed = textOf(transition, "direction") === "destination-to-source";
  const step: CalmFlowStep = {
    sequence: Number(field(transition, "sequence-number") ?? 0),
    description: textOf(transition, "description") ?? "",
    relationshipId,
    from: [],
    to: [],
    reversed,
    edgeIds: [],
    nodeIds: [],
  };
  if (!rel) return step;
  if (rel.type === "connects" || rel.type === "interacts") {
    const [first, ...rest] = rel.nodeIds;
    const source = first === undefined ? [] : [first];
    step.from = reversed ? rest : source;
    step.to = reversed ? source : rest;
    step.edgeIds = [...rel.edgeIds];
  } else {
    step.nodeIds = [...rel.nodeIds];
  }
  return step;
}
