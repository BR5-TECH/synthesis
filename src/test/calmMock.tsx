/**
 * A stand-in for the canvas library of the CALM diagrams in jsdom, which has
 * no layout for it to measure with. The read of a document (`src/calm`) is
 * real; only the drawing is replaced.
 *
 * Each test file mocks the module with it:
 *
 *     vi.mock("@xyflow/react", async (importOriginal) =>
 *       (await import("../test/calmMock")).xyflowModule(await importOriginal()));
 *
 * The stand-in draws each node with its own node component and each edge with
 * its own edge component, in plain DOM, and it calls the click handlers of the
 * canvas as the library does. A marker of an edge shows as a data attribute.
 */
import type { ComponentType, MouseEvent, ReactNode } from "react";

interface FlowNode {
  id: string;
  type: string;
  data: unknown;
  parentId?: string;
}
interface FlowEdge {
  id: string;
  type: string;
  source: string;
  target: string;
  data: unknown;
  markerEnd?: unknown;
  markerStart?: unknown;
}

interface FlowProps {
  nodes: FlowNode[];
  edges: FlowEdge[];
  nodeTypes: Record<string, ComponentType<Record<string, unknown>>>;
  edgeTypes: Record<string, ComponentType<Record<string, unknown>>>;
  onNodeClick?: (event: MouseEvent, node: FlowNode) => void;
  onEdgeClick?: (event: MouseEvent, edge: FlowEdge) => void;
  onPaneClick?: (event: MouseEvent) => void;
  [option: string]: unknown;
}

/** The options of the canvas that keep it from moving or changing, as attributes. */
const STILL_OPTIONS = [
  "panOnDrag",
  "panOnScroll",
  "zoomOnScroll",
  "zoomOnPinch",
  "zoomOnDoubleClick",
  "preventScrolling",
  "nodesDraggable",
  "nodesConnectable",
  "elementsSelectable",
] as const;

function ReactFlow(props: FlowProps) {
  const { nodes, edges, nodeTypes, edgeTypes, onNodeClick, onEdgeClick, onPaneClick } = props;
  const options = Object.fromEntries(
    STILL_OPTIONS.map((option) => [`data-${option.toLowerCase()}`, String(props[option])]),
  );
  return (
    <div
      className="react-flow"
      data-testid="calm-flow"
      {...options}
      onClick={(event) => onPaneClick?.(event)}
    >
      {nodes.map((node) => {
        const Node = nodeTypes[node.type];
        return (
          <div
            key={node.id}
            className="react-flow__node"
            data-id={node.id}
            data-parent={node.parentId}
            data-draggable={String((node as { draggable?: boolean }).draggable)}
            data-connectable={String((node as { connectable?: boolean }).connectable)}
            onClick={(event) => onNodeClick?.(event, node)}
          >
            <Node id={node.id} data={node.data} />
          </div>
        );
      })}
      {edges.map((edge) => {
        const Edge = edgeTypes[edge.type];
        return (
          <div
            key={edge.id}
            className="react-flow__edge"
            data-id={edge.id}
            data-marker-end={edge.markerEnd ? "true" : undefined}
            data-marker-start={edge.markerStart ? "true" : undefined}
            onClick={(event) => onEdgeClick?.(event, edge)}
          >
            <Edge
              id={edge.id}
              data={edge.data}
              source={edge.source}
              target={edge.target}
              markerEnd={edge.markerEnd ? "url(#end)" : undefined}
              markerStart={edge.markerStart ? "url(#start)" : undefined}
            />
          </div>
        );
      })}
    </div>
  );
}

function BaseEdge({ id, path, className }: { id: string; path: string; className?: string }) {
  return (
    <svg>
      <path className={`react-flow__edge-path ${className ?? ""}`} data-edge-path={id} d={path} />
    </svg>
  );
}

function EdgeLabelRenderer({ children }: { children: ReactNode }) {
  return <>{children}</>;
}

function Handle() {
  return null;
}

export function xyflowModule(actual: Record<string, unknown>): Record<string, unknown> {
  return { ...actual, ReactFlow, BaseEdge, EdgeLabelRenderer, Handle };
}
