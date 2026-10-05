/**
 * The cards, boxes and chips of the specification map
 * (`SMN-specification-map-nodes.md`). Every node is absolutely positioned on
 * the stage from its layout, in world coordinates.
 */
import type {
  CSSProperties,
  KeyboardEvent,
  MouseEvent,
  PointerEvent,
} from "react";
import {
  indexChildren,
  isWithin,
  plannedUnder,
  specFolder,
  type TreeIndex,
} from "../../state/specMap/tree";
import { EMPTY_ROLLUP, type Rollup } from "../../state/specMap/rollups";
import { sameRef, type MapRef } from "../../state/specMap/types";
import type { LayoutBox, LayoutChip, MapLayout } from "./layout";
import { CompletenessBar, Counts } from "./CompletenessBar";

export interface NodeHandlers {
  pointerDown: (ref: MapRef) => (e: PointerEvent<HTMLElement>) => void;
  select: (ref: MapRef) => void;
  hover: (ref: MapRef | null) => void;
}

/** The light-theme values of the design system's swatch hues. */
const HUE_LIGHT: Record<string, string> = {
  "#C19EFF": "#6F47C2",
  "#6BD7AA": "#1F8F6B",
  "#FF8453": "#E5530B",
  "#9AA7FF": "#3B4BEF",
  "#7A8BFF": "#4B5BFF",
  "#E0A93B": "#B07A00",
  "#8AB4E8": "#2862AB",
  "#B8B4AC": "#44403B",
};

function hueStyle(hue: string | undefined): CSSProperties | undefined {
  if (!hue) return undefined;
  return {
    "--smap-hue": hue,
    "--smap-hue-light": HUE_LIGHT[hue.toUpperCase()] ?? hue,
  } as CSSProperties;
}

function interaction(ref: MapRef, label: string, handlers: NodeHandlers) {
  return {
    role: "button" as const,
    tabIndex: 0,
    "aria-label": label,
    onPointerDown: handlers.pointerDown(ref),
    onClick: () => handlers.select(ref),
    onKeyDown: (e: KeyboardEvent<HTMLElement>) => {
      if (e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        handlers.select(ref);
      }
    },
    // SMN-FR-KTZB: the innermost node under the pointer is the hovered one.
    onMouseOver: (e: MouseEvent<HTMLElement>) => {
      e.stopPropagation();
      handlers.hover(ref);
    },
  };
}

export interface NodeLayerProps {
  tree: TreeIndex;
  layout: MapLayout;
  rollups: Map<string, Rollup>;
  selection: MapRef | null;
  active: MapRef | null;
  gapsOnly: boolean;
  related: Set<string> | null;
  dragging: MapRef | null;
  dropId: string | null;
  handlers: NodeHandlers;
}

export function NodeLayer(props: NodeLayerProps) {
  return (
    <>
      {props.layout.boxes.map((box) => (
        <MapBox key={box.id} box={box} {...props} />
      ))}
      {props.layout.chips.map((chip) => (
        <MapChip key={`${chip.kind}:${chip.key}`} chip={chip} {...props} />
      ))}
    </>
  );
}

function MapBox({
  box,
  tree,
  rollups,
  selection,
  active,
  gapsOnly,
  dragging,
  dropId,
  handlers,
}: NodeLayerProps & { box: LayoutBox }) {
  const node = tree.nodes.get(box.id)!;
  const r = rollups.get(box.id) ?? EMPTY_ROLLUP;
  const ref: MapRef = { kind: "index", id: box.id };
  const levels = tree.index.levels;
  const selected = sameRef(selection, ref);
  // SMN-FR-KTZB: the root and branch that contain the active node.
  const context =
    !selected && box.role !== "leafGroup" && active !== null && isWithin(tree, active, box.id);
  // SMO-FR-DKTM / SMN-FR-HLDQ
  const dim = sameRef(dragging, ref)
    ? "drag"
    : gapsOnly && r.gap === 0 && r.drafted === 0
      ? "gaps"
      : undefined;
  // SMD-FR-ULTF: a node that does not show its planned chips counts them.
  const planned = box.shape === "expanded" ? 0 : plannedUnder(tree, box.id);
  const plannedMeta = planned > 0 ? ` · +${planned} planned` : "";
  const attrs = {
    ...interaction(ref, `${node.label}, ${levels[box.depth]?.singular ?? ""}`, handlers),
    "data-shape": box.shape,
    "data-selected": selected || undefined,
    "data-context": context || undefined,
    "data-dim": dim,
    "data-drop": dropId === box.id || undefined,
    style: { left: box.x, top: box.y, width: box.w, height: box.h },
  };

  if (box.shape === "overview" || box.role === "root") {
    const meta =
      box.role === "leafGroup"
        ? `${r.specs} specs`
        : `${r.specs} specs · ${indexChildren(node).length} ${levels[box.depth + 1]?.plural ?? ""}`;
    return (
      <div className="smap-node smap-root" {...attrs}>
        <div className="smap-root__head">
          <span className="smap-swatch" style={hueStyle(node.hue)} />
          <span className="smap-root__label">{node.label}</span>
          <span className="smap-root__meta">
            {meta}
            {plannedMeta}
          </span>
        </div>
        {box.shape === "overview" && (
          <>
            <div className="smap-root__summary">{node.summary}</div>
            <div className="smap-root__stats">
              <CompletenessBar rollup={r} variant="card" />
              <Counts rollup={r} />
            </div>
          </>
        )}
      </div>
    );
  }

  if (box.role === "branch") {
    return (
      <div className="smap-node smap-box" data-role="branch" {...attrs}>
        <div className="smap-box__head">
          <span className="smap-box__label">{node.label}</span>
          <span className="smap-box__meta">
            {r.specs} specs
            {plannedMeta}
          </span>
        </div>
        {box.shape === "collapsed" && <div className="smap-box__summary">{node.summary}</div>}
        <div className="smap-box__bar">
          <CompletenessBar rollup={r} variant="box" />
        </div>
      </div>
    );
  }

  return (
    <div className="smap-node smap-box" data-role="leaf" {...attrs}>
      <div className="smap-box__head">
        <span className="smap-box__label">{node.label}</span>
        <CompletenessBar rollup={r} variant="mini" />
        <span className="smap-box__meta">
          {r.specs}
          {plannedMeta}
        </span>
      </div>
      <div className="smap-box__summary">{node.summary}</div>
    </div>
  );
}

function MapChip({
  chip,
  tree,
  selection,
  gapsOnly,
  related,
  dragging,
  handlers,
}: NodeLayerProps & { chip: LayoutChip }) {
  const style = { left: chip.x, top: chip.y, width: chip.w };
  if (chip.kind === "planned") {
    const draft = tree.planned.get(chip.key)!;
    const ref: MapRef = { kind: "planned", draftId: chip.key };
    return (
      <div
        className="smap-node smap-chip smap-chip--planned"
        {...interaction(ref, `draft ${draft.name}`, handlers)}
        data-selected={sameRef(selection, ref) || undefined}
        data-dim={sameRef(dragging, ref) ? "drag" : undefined}
        style={style}
      >
        <span className="smap-code smap-code--draft">draft</span>
        <span className="smap-chip__label">{draft.name}</span>
      </div>
    );
  }
  const spec = tree.specs.get(chip.key)!;
  const ref: MapRef = { kind: "spec", code: spec.code };
  const selected = sameRef(selection, ref);
  // SMO-FR-DKTM, SME-FR-WRPX, SMN-FR-HLDQ
  const dim = sameRef(dragging, ref)
    ? "drag"
    : related && !selected && !related.has(spec.code)
      ? "related"
      : gapsOnly && spec.state !== "gap"
        ? "gaps"
        : undefined;
  return (
    <div
      className="smap-node smap-chip"
      {...interaction(ref, `${spec.code} ${spec.label}`, handlers)}
      data-selected={selected || undefined}
      data-dim={dim}
      style={style}
    >
      <span className="smap-code" data-folder={specFolder(spec)}>
        {spec.code}
      </span>
      <span className="smap-chip__label">{spec.label}</span>
      <span className="smap-dot" data-state={spec.state} />
    </div>
  );
}
