/**
 * `CLM-calm-model.md`: the shapes of a read of a CALM document.
 *
 * A read makes a diagram graph or an error. The graph holds what each surface
 * of `CAD-calm-architecture-diagrams.md` shows: the nodes with their shapes and
 * groups, the edges, the flows, and the details of each item.
 */

/** A JSON object of a CALM document, as the document holds it. */
export type JsonObject = Record<string, unknown>;

/** CLM-FR-ODQZ: the steps of a read, in order. */
export type CalmStep = "parse" | "schema" | "references" | "graph" | "layout" | "load";

/** CLM-FR-DTYX: the shape of a diagram node, from its `node-type`. */
export type CalmShape =
  | "actor"
  | "system"
  | "service"
  | "database"
  | "webclient"
  | "network"
  | "dataAsset"
  | "generic";

/** CLM-FR-DWLY: the kind of containment that makes a node a group. */
export type CalmGroupKind = "deployed-in" | "composed-of";

/** The relationship types of CALM 1.3. */
export type CalmRelationshipType =
  | "interacts"
  | "connects"
  | "deployed-in"
  | "composed-of"
  | "options";

/** One requirement of a control (`control.json` `control-detail`). */
export interface CalmControlRequirement {
  requirementUrl: string;
  configUrl?: string;
  config?: unknown;
}

/** One control of an item, by its id (`control.json` `controls`). */
export interface CalmControl {
  id: string;
  description: string;
  requirements: CalmControlRequirement[];
}

/** CLM-FR-ESMQ: one decision of an `options` relationship. */
export interface CalmDecision {
  /** The `unique-id` of the `options` relationship. */
  relationshipId: string;
  description: string;
}

/** CLM-FR-DLUV: one diagram node. */
export interface CalmNodeView {
  id: string;
  name: string;
  nodeType: string;
  shape: CalmShape;
  description: string;
  interfaces: JsonObject[];
  controls: CalmControl[];
  metadata: unknown;
  /** CLM-FR-DWLY: the group that holds this node. */
  parent?: string;
  /** CLM-FR-DWLY: set when this node is a group. */
  group?: CalmGroupKind;
  /** CLM-FR-DYAW: the names of the other groups that list this node. */
  alsoIn: string[];
  /** CLM-FR-ESMQ: the decisions that name this node. */
  decisions: CalmDecision[];
}

/** CLM-FR-ERXV, CLM-FR-ESCS: one diagram edge. */
export interface CalmEdgeView {
  id: string;
  relationshipId: string;
  kind: "connects" | "interacts";
  source: string;
  target: string;
  label?: string;
  protocol?: string;
  /** CLM-FR-ERXV: the interfaces that the source end names. */
  sourceInterfaces: string[];
  /** CLM-FR-ERXV: the interfaces that the destination end names. */
  targetInterfaces: string[];
}

/** The details of one relationship, for the edges it makes and its steps. */
export interface CalmRelationshipView {
  id: string;
  type: CalmRelationshipType;
  description: string;
  protocol?: string;
  controls: CalmControl[];
  metadata: unknown;
  /** The edges that this relationship makes. */
  edgeIds: string[];
  /** CLM-FR-FSCM: the nodes that this relationship names. */
  nodeIds: string[];
  /** The source and destination interfaces, for a `connects` relationship. */
  sourceInterfaces: string[];
  targetInterfaces: string[];
  decisions: CalmDecision[];
}

/** CLM-FR-FDGM, CLM-FR-FKHM: one step of a flow. */
export interface CalmFlowStep {
  sequence: number;
  description: string;
  relationshipId: string;
  /** The node the step starts at. */
  from: string[];
  /** The nodes the step goes to. */
  to: string[];
  /** True when the step goes against the edges of its relationship. */
  reversed: boolean;
  /** The edges the step uses; empty for a relationship that makes no edge. */
  edgeIds: string[];
  /** CLM-FR-FSCM: the nodes the step highlights in place of an edge. */
  nodeIds: string[];
}

/** CLM-FR-FDGM: one flow. */
export interface CalmFlowView {
  id: string;
  name: string;
  description: string;
  steps: CalmFlowStep[];
}

/** The graph that the graph step makes. */
export interface CalmGraph {
  /** In document order. A group comes before the nodes it holds. */
  nodes: CalmNodeView[];
  edges: CalmEdgeView[];
  relationships: CalmRelationshipView[];
  flows: CalmFlowView[];
  /** CAD-FR-MUJL: the top-level content of the document. */
  metadata: unknown;
  controls: CalmControl[];
  adrs: string[];
}

/** A box in the coordinates of the diagram. */
export interface Box {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** CLM-FR-GDML: the route of one edge and the box of its label, in diagram coordinates. */
export interface CalmRoute {
  points: Array<{ x: number; y: number }>;
  label?: Box;
}

/** CLM-FR-GDML: the graph with a place for each node. */
export interface CalmLayout {
  graph: CalmGraph;
  /**
   * The box of each node, by id. `x` and `y` are relative to the group that
   * holds the node, or to the diagram when no group holds it.
   */
  boxes: Record<string, Box>;
  /**
   * The route of each edge that the layout placed, by edge id. An edge between
   * a node and a group that holds it has no route; the canvas draws it alone.
   */
  routes: Record<string, CalmRoute>;
  /** The size of the whole diagram. */
  width: number;
  height: number;
}

/** CLM-FR-ODQZ: what one read made. */
export type CalmResult =
  | {
      state: "rendered";
      layout: CalmLayout;
      /** A short tag for the result, so a surface knows when it changed. */
      tag: string;
    }
  | {
      state: "error";
      step: CalmStep;
      /** The error statement of the step that failed. */
      summary: string;
      /** The reason, when the step gave one. */
      detail?: string;
      /** CAD-FR-OZST: the 1-based source line of the error, when known. */
      line?: number;
      /** CLM-FR-HPJT: a failed library load, which the next read tries again. */
      transient?: boolean;
    };

export type CalmError = Extract<CalmResult, { state: "error" }>;

/** CLM-FR-ODQZ: the error statement of each step. */
export const CALM_ERROR = {
  parse: "The CALM source is not valid JSON.",
  schema: "The CALM source does not match the CALM 1.3 schema.",
  references: "The architecture refers to an item that does not exist.",
  duplicates: "The architecture uses one unique-id more than once.",
  graph: "The architecture holds a containment loop.",
  layout: "The application cannot compute the layout.",
  load: "The diagram library cannot be loaded.",
} as const;

/** CLM-FR-HSUH: the statement that a graph with no nodes shows. */
export const NO_NODES_STATEMENT = "The architecture has no nodes.";
