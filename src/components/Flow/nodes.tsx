import { useEffect, useRef } from "react";

import type { ArtifactOption } from "../../hooks/useProjectArtifacts";
import type {
  FlowLoop,
  FlowNode,
  FlowPosition,
  FlowSize,
} from "../../state/flowDocument";
import { CommentMarkdown } from "../CommentMarkdown";
import { Icon } from "../icons";
import { NODE_W, isInteractive, type Reference } from "./geometry";
import { ArtifactPicker, ElementReferences } from "./references";

export interface FlowNodeViewProps {
  node: FlowNode;
  /** Where the node sits on the canvas, containers resolved (FLO-FR-21). */
  at: FlowPosition;
  references: Reference[];
  artifacts: ArtifactOption[];
  selected: boolean;
  /** FLO-FR-33: this node is what an in-flight connection would land on. */
  magnet: boolean;
  onDragStart: (e: React.PointerEvent) => void;
  onConnectStart: (e: React.PointerEvent) => void;
  onConnectEnd: () => void;
  onRename: (name: string) => void;
  onAddArtifact: (artifactId: string) => void;
  onRemoveArtifact: (artifactId: string) => void;
  onSetPrompt: (prompt: string) => void;
  onOpenArtifact: (artifact: ArtifactOption) => void;
  onRemove: () => void;
  /** Reports the height this node paints at, for FLO-FR-48's border anchors. */
  onMeasure: (id: string, height: number) => void;
}

/**
 * One node's three stacked regions, any of which may be absent (FLO-FR-14): its
 * name, its artifact references, and its inline prompt. A node carrying neither
 * a reference nor a prompt renders name-only, and its editing affordances — the
 * artifact picker, the per-reference remove buttons, and the prompt's editable
 * source — appear when it is selected.
 */
export function FlowNodeView({
  node,
  at,
  references,
  artifacts,
  selected,
  magnet,
  onDragStart,
  onConnectStart,
  onConnectEnd,
  onRename,
  onAddArtifact,
  onRemoveArtifact,
  onSetPrompt,
  onOpenArtifact,
  onRemove,
  onMeasure,
}: FlowNodeViewProps) {
  const referenced = new Set(references.map((r) => r.id));
  const rootRef = useRef<HTMLDivElement>(null);
  /**
   * FLO-FR-48: what the node actually paints at, which is what its edges have
   * to find their border on. A node's height is its content's — references and
   * a rendered prompt (FLO-FR-13) — so nothing but the DOM knows it, and an
   * edge anchored to a guess lands its arrowhead inside the node, under the
   * node itself. Watched as well as measured: a prompt re-rendering or a font
   * arriving changes the height without this component rendering again.
   */
  useEffect(() => {
    const el = rootRef.current;
    if (!el) return;
    // Only while the node is NOT selected. Selecting one opens its picker and
    // its prompt field and makes it markedly taller, and selection is view
    // state that the document knows nothing about (FLO-FR-22) — so letting it
    // move an anchor would re-route the graph, and swing an edge from one
    // border of a node to another, every time the author clicked on it. The
    // height a node keeps is the height it presents when it is being read.
    if (selected) return;
    const report = () => onMeasure(node.id, el.offsetHeight);
    report();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(report);
    observer.observe(el);
    return () => observer.disconnect();
  }, [node.id, onMeasure, selected]);
  return (
    <div
      ref={rootRef}
      className="flow-node"
      data-testid={`flow-node-${node.id}`}
      data-node-id={node.id}
      data-element-id={node.id}
      data-selected={selected}
      data-magnet={magnet}
      style={{ left: at.x, top: at.y, width: NODE_W }}
      onPointerDown={(e) => {
        if (!isInteractive(e.target)) onDragStart(e);
      }}
      // FLO-FR-16: a connection released back over its own source lands here,
      // and is refused by the graph rules rather than by anything this view
      // decides for itself.
      onPointerUp={onConnectEnd}
    >
      <div className="flow-node__head">
        <span className="flow-node__grip" aria-hidden="true">
          ⠿
        </span>
        {/* FLO-FR-09: renamed in place. Free text, not required to be unique — a
            node's identity is its id, never its name. */}
        <input
          className="flow-node__name"
          aria-label="Node name"
          value={node.name}
          onChange={(e) => onRename(e.target.value)}
        />
        {selected && (
          <button
            className="btn btn--ghost btn--icon btn--sm"
            aria-label={`Remove node ${node.name}`}
            onClick={onRemove}
          >
            <Icon.X size={11} />
          </button>
        )}
      </div>

      <ElementReferences
        references={references}
        selected={selected}
        onRemoveArtifact={onRemoveArtifact}
        onOpenArtifact={onOpenArtifact}
      />

      {selected && (
        <ArtifactPicker
          artifacts={artifacts}
          referenced={referenced}
          onAdd={onAddArtifact}
        />
      )}

      {/* FLO-FR-13: the prompt is Markdown, and which of its two faces the node
          shows is decided by whether the node is selected — the same distinction
          that shows and hides the picker above. Selected, it is the Markdown
          source in a plain field: no formatting toolbar, no embedded Editor, and
          therefore no Find or Find & Replace (SNV-FR-43). Unselected, it is that
          same prompt rendered as rich text on the terms a comment's body renders
          (CMT-FR-09), and the node is as tall as it needs to be. */}
      {selected ? (
        <textarea
          className="flow-node__prompt"
          aria-label="Inline prompt"
          placeholder="Inline prompt (Markdown)"
          rows={3}
          value={node.prompt ?? ""}
          onChange={(e) => onSetPrompt(e.target.value)}
        />
      ) : (
        node.prompt !== undefined &&
        node.prompt !== "" && (
          <CommentMarkdown
            body={node.prompt}
            className="flow-node__rendered"
            testId={`flow-node-prompt-${node.id}`}
          />
        )
      )}

      {/* FLO-FR-15: the connection source. Any number of edges may leave an
          element and any number may arrive at it. */}
      <button
        className="flow-node__port"
        aria-label={`Connect from ${node.name}`}
        onPointerDown={onConnectStart}
      />
    </div>
  );
}

export interface FlowLoopViewProps {
  loop: FlowLoop;
  at: FlowPosition;
  size: FlowSize;
  references: Reference[];
  artifacts: ArtifactOption[];
  selected: boolean;
  magnet: boolean;
  onDragStart: (e: React.PointerEvent) => void;
  onResizeStart: (e: React.PointerEvent) => void;
  onConnectStart: (e: React.PointerEvent) => void;
  onConnectEnd: () => void;
  onRename: (name: string) => void;
  onAddArtifact: (artifactId: string) => void;
  onRemoveArtifact: (artifactId: string) => void;
  onOpenArtifact: (artifact: ArtifactOption) => void;
  onSetMaxPasses: (value: number | null) => void;
  onRemove: () => void;
}

/**
 * FLO-FR-38: a loop, drawn as the container it is — a header carrying its name
 * and its pass bound, its artifact references beneath, over a region holding its
 * members.
 *
 * The name and the pass bound are short single-line plain text edited in place,
 * and the references render on exactly a node's terms (FLO-FR-11). A loop
 * carries no inline prompt of its own: what it is run under is the artifact it
 * references (FLO-FR-37), so the rich-text rendering of FLO-FR-13 belongs to a
 * node's prompt and to nothing else. A loop holding nothing renders as an empty
 * container at its own size rather than collapsing away.
 */
export function FlowLoopView({
  loop,
  at,
  size,
  references,
  artifacts,
  selected,
  magnet,
  onDragStart,
  onResizeStart,
  onConnectStart,
  onConnectEnd,
  onRename,
  onAddArtifact,
  onRemoveArtifact,
  onOpenArtifact,
  onSetMaxPasses,
  onRemove,
}: FlowLoopViewProps) {
  const referenced = new Set(references.map((r) => r.id));
  return (
    <div
      className="flow-loop"
      data-testid={`flow-loop-${loop.id}`}
      data-loop-id={loop.id}
      data-element-id={loop.id}
      data-selected={selected}
      data-magnet={magnet}
      style={{ left: at.x, top: at.y, width: size.width, height: size.height }}
      onPointerDown={(e) => {
        // Only the header drags the loop: a press in the body is a press on the
        // canvas behind it, which is what lets the author pan and marquee over
        // a region a large loop covers.
        const el = e.target as HTMLElement | null;
        if (!isInteractive(el) && el?.closest?.(".flow-loop__head")) {
          onDragStart(e);
        }
      }}
      onPointerUp={onConnectEnd}
    >
      <div className="flow-loop__head">
        <span className="flow-loop__grip" aria-hidden="true">
          ⟳
        </span>
        <input
          className="flow-loop__name"
          aria-label="Loop name"
          placeholder="Loop name"
          value={loop.name}
          onChange={(e) => onRename(e.target.value)}
        />
        <input
          className="flow-loop__passes"
          aria-label="Loop max passes"
          placeholder="Max"
          inputMode="numeric"
          value={loop.maxPasses ?? ""}
          onChange={(e) => {
            const raw = e.target.value.trim();
            if (raw === "") {
              onSetMaxPasses(null);
              return;
            }
            const n = Number(raw);
            onSetMaxPasses(Number.isFinite(n) ? Math.trunc(n) : null);
          }}
        />
        {selected && (
          <button
            className="btn btn--ghost btn--icon btn--sm"
            aria-label={`Remove loop ${loop.name}`}
            onClick={onRemove}
          >
            <Icon.X size={11} />
          </button>
        )}
      </div>
      {/* FLO-FR-37: the artifacts the loop is run under — the prompt among them
          being where the author says what ends it — on exactly the terms a
          node's references render (FLO-FR-11). The rows sit in the loop's header
          region rather than over its members, so the container's body stays the
          region elements are dropped into. */}
      <div className="flow-loop__refs">
        <ElementReferences
          references={references}
          selected={selected}
          onRemoveArtifact={onRemoveArtifact}
          onOpenArtifact={onOpenArtifact}
        />
        {selected && (
          <ArtifactPicker
            artifacts={artifacts}
            referenced={referenced}
            onAdd={onAddArtifact}
          />
        )}
      </div>

      {/* FLO-FR-39: the loop is connectable as an element of whatever contains
          it, and its border is the anchor an edge lands on. */}
      <button
        className="flow-node__port flow-loop__port"
        aria-label={`Connect from ${loop.name}`}
        onPointerDown={onConnectStart}
      />

      {/* FLO-FR-44: resized from its own corner. Never smaller than what it
          holds, which the document rule enforces rather than this handle. */}
      <button
        className="flow-loop__resize"
        aria-label={`Resize loop ${loop.name}`}
        onPointerDown={onResizeStart}
      />
    </div>
  );
}
