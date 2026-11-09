/**
 * CLM-FR-DLUV, CLM-FR-DTYX, CLM-FR-DWLY, CLM-FR-DYAW, CLM-FR-ESMQ,
 * CLM-FR-ETEA: the nodes of a CALM canvas. A node shows its name and its
 * `node-type`, a group frames its members, and the markers state what the
 * details panel holds. All CALM text is React text (CLM-FR-IRGG).
 */
import { Handle, Position, type Node, type NodeProps } from "@xyflow/react";

import { SHOWN_INTERFACES } from "../../calm/layout";
import type { CalmNodeView, CalmShape } from "../../calm/types";

export interface CalmNodeData extends Record<string, unknown> {
  node: CalmNodeView;
  /** CAD-FR-KSEJ: no step of the chosen flow uses this node. */
  dim: boolean;
  /** CAD-FR-KSEJ: a step of the chosen flow uses this node. */
  inFlow: boolean;
  /** CAD-FR-KUBU: the details panel shows this node. */
  selected: boolean;
}

export type CalmFlowNode = Node<CalmNodeData, "calmNode" | "calmGroup">;

/** CLM-FR-DTYX: a small outline glyph for each shape, in the text colour. */
const GLYPHS: Record<CalmShape, string> = {
  actor: "M8 7.5a2.75 2.75 0 1 0 0-5.5 2.75 2.75 0 0 0 0 5.5ZM3 14c0-2.8 2.2-4.5 5-4.5s5 1.7 5 4.5",
  system: "M2.5 2.5h11v11h-11zM2.5 6h11",
  service: "M3 3.5h10a1.5 1.5 0 0 1 1.5 1.5v6a1.5 1.5 0 0 1-1.5 1.5H3A1.5 1.5 0 0 1 1.5 11V5A1.5 1.5 0 0 1 3 3.5ZM5 8h6",
  database:
    "M2.5 4c0-1.1 2.5-2 5.5-2s5.5.9 5.5 2v8c0 1.1-2.5 2-5.5 2s-5.5-.9-5.5-2zM2.5 4c0 1.1 2.5 2 5.5 2s5.5-.9 5.5-2M2.5 8c0 1.1 2.5 2 5.5 2s5.5-.9 5.5-2",
  webclient: "M2 3h12v10H2zM2 6h12M4 4.5h.01M6 4.5h.01",
  network: "M8 2v3M8 11v3M2 8h3M11 8h3M5 5h6v6H5z",
  dataAsset: "M3.5 1.5h6l3 3v10h-9zM9.5 1.5v3h3M5.5 8.5h5M5.5 11h5",
  generic: "M2.5 2.5h11v11h-11z",
};

export function ShapeGlyph({ shape }: { shape: CalmShape }) {
  return (
    <svg
      className="calm-glyph"
      viewBox="0 0 16 16"
      width={14}
      height={14}
      aria-hidden="true"
      focusable="false"
    >
      <path d={GLYPHS[shape]} />
    </svg>
  );
}

/** CLM-FR-ETEA, CLM-FR-ESMQ: the control and decision markers of an item. */
export function ItemMarkers({
  controls,
  decisions,
}: {
  controls: number;
  decisions: number;
}) {
  if (controls === 0 && decisions === 0) return null;
  return (
    <span className="calm-markers">
      {controls > 0 && (
        <span
          className="calm-marker calm-marker--controls"
          title={`${controls} ${controls === 1 ? "control" : "controls"}`}
        >
          <svg viewBox="0 0 16 16" width={11} height={11} aria-hidden="true" focusable="false">
            <path d="M8 1.5 13 3.5v4c0 3.2-2.2 5.6-5 7-2.8-1.4-5-3.8-5-7v-4z" />
          </svg>
          {controls}
        </span>
      )}
      {decisions > 0 && (
        <span
          className="calm-marker calm-marker--decisions"
          title={`${decisions} ${decisions === 1 ? "decision" : "decisions"}`}
        >
          <svg viewBox="0 0 16 16" width={11} height={11} aria-hidden="true" focusable="false">
            <path d="M8 1.5 14.5 8 8 14.5 1.5 8z" />
          </svg>
          {decisions}
        </span>
      )}
    </span>
  );
}

/** The hidden ends that the canvas library joins edges to. */
function Ends() {
  return (
    <>
      <Handle type="target" position={Position.Left} isConnectable={false} className="calm-end" />
      <Handle type="source" position={Position.Right} isConnectable={false} className="calm-end" />
    </>
  );
}

const stateClass = (data: CalmNodeData) =>
  [
    data.dim ? "calm-dim" : "",
    data.inFlow ? "calm-in-flow" : "",
    data.selected ? "calm-selected" : "",
  ]
    .filter(Boolean)
    .join(" ");

/** CLM-FR-DLUV, CLM-FR-ETEA, CLM-FR-DYAW: a node that is not a group. */
export function CalmNodeCard({ data }: NodeProps<CalmFlowNode>) {
  const { node } = data;
  const shown = node.interfaces.slice(0, SHOWN_INTERFACES);
  const more = node.interfaces.length - shown.length;
  return (
    <div
      className={`calm-node calm-node--${node.shape} ${stateClass(data)}`}
      data-calm-node={node.id}
    >
      <Ends />
      <div className="calm-node__head">
        <ShapeGlyph shape={node.shape} />
        <span className="calm-node__name">{node.name}</span>
        <ItemMarkers controls={node.controls.length} decisions={node.decisions.length} />
      </div>
      <div className="calm-node__type">{node.nodeType}</div>
      {shown.length > 0 && (
        <ul className="calm-node__interfaces">
          {shown.map((iface, index) => (
            <li key={index}>{String(iface["unique-id"] ?? "")}</li>
          ))}
          {more > 0 && <li className="calm-node__more">+{more} more</li>}
        </ul>
      )}
      {node.alsoIn.length > 0 && (
        <div className="calm-node__also">Also in {node.alsoIn.join(", ")}</div>
      )}
    </div>
  );
}

/** CLM-FR-DWLY: a group, with a dashed border for `deployed-in`. */
export function CalmGroupBox({ data }: NodeProps<CalmFlowNode>) {
  const { node } = data;
  return (
    <div
      className={`calm-group calm-group--${node.group ?? "composed-of"} ${stateClass(data)}`}
      data-calm-node={node.id}
    >
      <Ends />
      <div className="calm-group__head">
        <ShapeGlyph shape={node.shape} />
        <span className="calm-group__name">{node.name}</span>
        <span className="calm-group__type">{node.nodeType}</span>
        <span className="calm-group__kind">{node.group}</span>
        <ItemMarkers controls={node.controls.length} decisions={node.decisions.length} />
      </div>
      {node.alsoIn.length > 0 && (
        <div className="calm-node__also">Also in {node.alsoIn.join(", ")}</div>
      )}
    </div>
  );
}

export const CALM_NODE_TYPES = { calmNode: CalmNodeCard, calmGroup: CalmGroupBox };
