/**
 * `CAD-calm-architecture-diagrams.md`: the drawing of one CALM layout at its
 * natural size. The canvas library draws the nodes and the edges, but it does
 * not pan, zoom, or drag (CAD-FR-IRIT, CAD-FR-MBML): the surface around the
 * drawing scales and moves it, as it does the drawing of a Mermaid diagram.
 */
import "@xyflow/react/dist/base.css";

import { useMemo } from "react";
import { MarkerType, ReactFlow, type EdgeMarker } from "@xyflow/react";

import type { CalmLayout } from "../../calm/types";
import { CALM_EDGE_TYPES, type CalmFlowEdge } from "./edges";
import { absoluteBoxes, type CalmSelection, type FlowHighlight } from "./model";
import { CALM_NODE_TYPES, type CalmFlowNode } from "./nodes";

const VIEWPORT = { x: 0, y: 0, zoom: 1 };
const PRO = { hideAttribution: true };

const marker = (inFlow: boolean): EdgeMarker => ({
  type: MarkerType.ArrowClosed,
  width: 16,
  height: 16,
  color: inFlow ? "var(--accent)" : "var(--fg-3)",
});

export interface CalmCanvasProps {
  layout: CalmLayout;
  /** CAD-FR-KSEJ: the highlight of the chosen flow. */
  highlight?: FlowHighlight | null;
  /** CAD-FR-KUBU: the item that the details panel shows. */
  selected?: CalmSelection | null;
  /** CAD-FR-KUBU: a click on an item, or on an empty part of the canvas. */
  onSelect?: (selection: CalmSelection | null) => void;
}

/** The nodes and the edges that the canvas library draws. */
export function flowElements(
  layout: CalmLayout,
  highlight: FlowHighlight | null,
  selected: CalmSelection | null,
  onSelect?: (selection: CalmSelection | null) => void,
): { nodes: CalmFlowNode[]; edges: CalmFlowEdge[] } {
  const absolute = absoluteBoxes(layout);
  const nodes: CalmFlowNode[] = layout.graph.nodes.map((node) => {
    const box = layout.boxes[node.id] ?? { x: 0, y: 0, width: 0, height: 0 };
    const inFlow = highlight?.nodes.has(node.id) ?? false;
    const flowNode: CalmFlowNode = {
      id: node.id,
      type: node.group ? "calmGroup" : "calmNode",
      position: { x: box.x, y: box.y },
      width: box.width,
      height: box.height,
      style: { width: box.width, height: box.height },
      data: {
        node,
        inFlow,
        dim: highlight !== null && highlight !== undefined && !inFlow,
        selected: selected?.kind === "node" && selected.id === node.id,
      },
      draggable: false,
      connectable: false,
      selectable: false,
      // A group sits under the members it holds.
      zIndex: node.group ? 0 : 1,
    };
    if (node.parent !== undefined) {
      flowNode.parentId = node.parent;
      flowNode.extent = "parent";
    }
    return flowNode;
  });
  const empty = { x: 0, y: 0, width: 0, height: 0 };
  const edges: CalmFlowEdge[] = layout.graph.edges.map((edge) => {
    const steps = highlight?.steps.get(edge.id) ?? [];
    const inFlow = steps.length > 0;
    const forward = !inFlow || steps.some((step) => !step.reversed);
    const backward = steps.some((step) => step.reversed);
    const flowEdge: CalmFlowEdge = {
      id: edge.id,
      type: "calmEdge",
      source: edge.source,
      target: edge.target,
      zIndex: 2,
      selectable: false,
      focusable: false,
      data: {
        edge,
        from: absolute[edge.source] ?? empty,
        to: absolute[edge.target] ?? empty,
        route: Object.prototype.hasOwnProperty.call(layout.routes, edge.id)
          ? layout.routes[edge.id]
          : undefined,
        steps,
        dim: highlight !== null && highlight !== undefined && !inFlow,
        selected: selected?.kind === "edge" && selected.id === edge.id,
        onPick: onSelect ? () => onSelect({ kind: "edge", id: edge.id }) : undefined,
      },
    };
    if (forward) flowEdge.markerEnd = marker(inFlow);
    if (backward) flowEdge.markerStart = marker(inFlow);
    return flowEdge;
  });
  return { nodes, edges };
}

export function CalmCanvas({ layout, highlight = null, selected = null, onSelect }: CalmCanvasProps) {
  const { nodes, edges } = useMemo(
    () => flowElements(layout, highlight, selected, onSelect),
    [layout, highlight, selected, onSelect],
  );
  const interactive = onSelect !== undefined;
  return (
    <div
      className={`calm-canvas${interactive ? " calm-canvas--interactive" : ""}`}
      style={{ width: layout.width, height: layout.height }}
      data-testid="calm-canvas"
    >
      <ReactFlow<CalmFlowNode, CalmFlowEdge>
        nodes={nodes}
        edges={edges}
        nodeTypes={CALM_NODE_TYPES}
        edgeTypes={CALM_EDGE_TYPES}
        defaultViewport={VIEWPORT}
        minZoom={1}
        maxZoom={1}
        panOnDrag={false}
        panOnScroll={false}
        zoomOnScroll={false}
        zoomOnPinch={false}
        zoomOnDoubleClick={false}
        preventScrolling={false}
        nodesDraggable={false}
        nodesConnectable={false}
        nodesFocusable={false}
        edgesFocusable={false}
        elementsSelectable={false}
        proOptions={PRO}
        onNodeClick={
          interactive
            ? (event, node) => {
                event.stopPropagation();
                onSelect({ kind: "node", id: node.id });
              }
            : undefined
        }
        onEdgeClick={
          interactive
            ? (event, edge) => {
                event.stopPropagation();
                onSelect({ kind: "edge", id: edge.id });
              }
            : undefined
        }
        onPaneClick={interactive ? () => onSelect(null) : undefined}
      />
    </div>
  );
}
