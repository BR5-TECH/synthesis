/** The Flow document shape, its constants, and the empty document. */

export interface FlowPosition {
  x: number;
  y: number;
}

export interface FlowSize {
  width: number;
  height: number;
}

/**
 * FLO-FR-06: what every element — node and loop alike — carries.
 *
 * A `position` is measured from the origin of whatever holds the element: the
 * canvas for a top-level one, the container for a child (FLO-FR-21). That is
 * what makes moving a loop one changed position rather than one per member.
 */
export interface FlowElementBase {
  /** Unique among ALL elements in the document, and stable across saves. */
  id: string;
  name: string;
  /**
   * FLO-FR-10: the referenced artifacts' stable, path-derived keys (ASC-FR-13),
   * in the order the author added them. Nodes and loops alike reference any
   * number of artifacts; the field is absent when the element references none
   * (FLO-FR-14, FLO-FR-37).
   */
  artifactIds?: string[];
  /** The loop that holds this element; absent when it sits at the top level. */
  parentId?: string;
  position: FlowPosition;
}

/** FLO-FR-06: a node's persisted fields. */
export interface FlowNode extends FlowElementBase {
  /** FLO-FR-13: the node's inline prompt, as Markdown. Absent when empty. */
  prompt?: string;
}

/**
 * FLO-FR-37: a loop — an element that holds other elements.
 *
 * It expresses the part of a workflow that is not a fixed order: its members are
 * the candidate steps, each deciding from its own prompt which of its neighbours
 * runs next, and the artifacts the loop itself references are the instruction it
 * is run under — which is where the author writes what ends it, in the prompt
 * that says so rather than in a field of its own.
 */
export interface FlowLoop extends FlowElementBase {
  /** A whole number greater than zero, or absent for a loop nothing bounds. */
  maxPasses?: number;
  size: FlowSize;
}

/** FLO-FR-15/FLO-FR-19: a directed, optionally labelled edge. */
export interface FlowEdge {
  id: string;
  from: string;
  to: string;
  label?: string;
}

export interface FlowDocument {
  version: 1;
  /**
   * FLO-FR-25: what this Flow is called — the name it will be run under, which
   * is why it is the Flow's own field rather than its filename. Required in the
   * sense that the editor asks for one and marks its absence; an empty name is
   * still written and still saved, because a Flow that cannot be saved until it
   * is named would lose work over a field nothing reads yet.
   */
  name: string;
  /** FLO-FR-25: what it does, in prose. Optional; absent when empty. */
  description?: string;
  /** FLO-FR-37: always present in memory; absent from the file when empty. */
  loops: FlowLoop[];
  nodes: FlowNode[];
  edges: FlowEdge[];
}

/** Either kind of element, tagged so callers can tell them apart. */
export type FlowElement =
  | ({ kind: "node" } & FlowNode)
  | ({ kind: "loop" } & FlowLoop);

/** The only document version this build reads or writes. */
export const FLOW_VERSION = 1;

/** FLO-FR-07: what a freshly-added node is called before the user renames it. */
export const DEFAULT_NODE_NAME = "New node";
/** FLO-FR-07: and a freshly-added loop. */
export const DEFAULT_LOOP_NAME = "New loop";
/** FLO-FR-07: the size an empty loop arrives at, in graph units. */
export const DEFAULT_LOOP_SIZE: FlowSize = { width: 420, height: 260 };
/**
 * FLO-FR-44: how much room a loop keeps around what it holds. A container drawn
 * exactly on its members' bounding box reads as if they were spilling out of it,
 * and leaves nowhere to drop the next one.
 */
export const LOOP_PADDING = 24;
/** The room the loop's header takes above its members. */
export const LOOP_HEADER_H = 56;
/**
 * The extent a node occupies on the canvas, which is what edges anchor to and
 * what a container has to be big enough to hold (FLO-FR-44).
 *
 * A node's height grows with its rendered prompt (FLO-FR-13), so this is the
 * floor rather than the truth for every node. It lives here rather than in the
 * canvas because `minLoopSize` is a document rule and has to agree with what the
 * canvas draws.
 */
export const MEMBER_NODE_W = 210;
export const MEMBER_NODE_H = 60;

export function emptyFlow(): FlowDocument {
  return { version: FLOW_VERSION, name: "", loops: [], nodes: [], edges: [] };
}
