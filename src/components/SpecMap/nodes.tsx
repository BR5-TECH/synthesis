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
  changesUnder,
  indexChildren,
  isWithin,
  specFolder,
  type TreeIndex,
} from "../../state/specMap/tree";
import { EMPTY_ROLLUP, type Rollup } from "../../state/specMap/rollups";
import { changeLocationLabel, sameRef, type MapRef } from "../../state/specMap/types";
import type { LayoutBox, LayoutChip, MapLayout } from "./layout";
import { CompletenessBar, Counts } from "./CompletenessBar";
import { holdsUnplanned, isUnplanned, type SpecLinks } from "../../state/delivery/specLinks";
import type { MenuBindings } from "../MapDiscussion/NodeMenu";

export interface NodeHandlers {
  select: (ref: MapRef) => void;
  hover: (ref: MapRef | null) => void;
  /**
   * SMN-FR-JHQR: the handlers that open the **Discuss** menu of a node, or
   * undefined for a node that has none, such as a change chip.
   */
  menu?: (ref: MapRef) => MenuBindings | undefined;
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

/** SMZ-FR-KAHU: a press on a node never pans the canvas. */
function keepFromPan(e: PointerEvent<HTMLElement>) {
  if (e.button === 0) e.stopPropagation();
}

function interaction(ref: MapRef, label: string, handlers: NodeHandlers) {
  const menu = handlers.menu?.(ref);
  return {
    role: "button" as const,
    tabIndex: 0,
    "aria-label": label,
    onPointerDown: keepFromPan,
    onClick: () => handlers.select(ref),
    onKeyDown: (e: KeyboardEvent<HTMLElement>) => {
      if (e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        handlers.select(ref);
      }
      menu?.onKeyDown(e);
    },
    onContextMenu: menu?.onContextMenu,
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
  /** SDL-FR-SGQM: the delivery links, or null where none show. */
  links?: SpecLinks | null;
  /** SDL-FR-KXCY: the **unplanned** filter is on. */
  unplanned?: boolean;
  related: Set<string> | null;
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
  links,
  unplanned,
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
  // SMN-FR-HLDQ / SDL-FR-KXCY
  const dim =
    gapsOnly && r.gap === 0 && r.drafted === 0
      ? "gaps"
      : unplanned && links && !holdsUnplanned(links, tree, box.id)
        ? "unplanned"
        : undefined;
  // SMN-FR-YEVY: a node that does not show its change chips counts the changes
  // beneath it, each in its location.
  const changes = box.shape === "expanded" ? 0 : changesUnder(tree, box.id);
  const changeMeta =
    changes > 0 ? ` · +${changes} ${changes === 1 ? "change" : "changes"}` : "";
  const attrs = {
    ...interaction(ref, `${node.label}, ${levels[box.depth]?.singular ?? ""}`, handlers),
    "data-shape": box.shape,
    "data-selected": selected || undefined,
    "data-context": context || undefined,
    "data-dim": dim,
    // SMN-FR-VNTJ: a capability a change names and the live specs do not hold.
    "data-planned": node.plannedCapability || undefined,
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
          <span className="smap-root__meta" title={`${meta}${changeMeta}`}>
            {meta}
            {changeMeta}
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
          <span className="smap-box__meta" title={`${r.specs} specs${changeMeta}`}>
            {r.specs} specs
            {changeMeta}
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
        {node.plannedCapability && <span className="smap-box__planned">planned</span>}
        <CompletenessBar rollup={r} variant="mini" />
        <span className="smap-box__meta" title={`${r.specs}${changeMeta}`}>
          {r.specs}
          {changeMeta}
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
  links,
  unplanned,
  related,
  handlers,
}: NodeLayerProps & { chip: LayoutChip }) {
  const style = { left: chip.x, top: chip.y, width: chip.w };
  if (chip.kind === "change") {
    const change = tree.changes.get(chip.key)!;
    const ref: MapRef = { kind: "change", key: chip.key };
    const issue = (change.issues?.length ?? 0) > 0;
    const holder = tree.nodes.get(chip.parentId)?.label ?? "";
    return (
      <div
        className="smap-node smap-chip smap-chip--change"
        {...interaction(
          ref,
          `change ${change.change}, ${changeLocationLabel(change)}, on ${holder}`,
          handlers,
        )}
        data-selected={sameRef(selection, ref) || undefined}
        style={style}
      >
        <span className="smap-code smap-code--change">change</span>
        <span className="smap-chip__label">{change.change}</span>
        {issue && <span className="smap-dot" data-state="gap" aria-label="has issues" />}
      </div>
    );
  }
  const spec = tree.specs.get(chip.key)!;
  const ref: MapRef = { kind: "spec", code: spec.code };
  const selected = sameRef(selection, ref);
  // SME-FR-WRPX, SMN-FR-HLDQ, SDL-FR-KXCY
  const dim =
    related && !selected && !related.has(spec.code)
      ? "related"
      : gapsOnly && spec.state !== "gap"
        ? "gaps"
        : unplanned && links && !isUnplanned(links, spec.code, spec.state)
          ? "unplanned"
          : undefined;
  // SDL-FR-SGQM: a spec node that a delivery item links carries the planned mark.
  const linked = links?.bySpec.has(spec.code) ?? false;
  return (
    <div
      className="smap-node smap-chip"
      {...interaction(ref, `${spec.code} ${spec.label}`, handlers)}
      data-selected={selected || undefined}
      data-dim={dim}
      data-plan-linked={linked || undefined}
      style={style}
    >
      {/* SMN-FR-PMZM: the badge of the node where it carries one. */}
      <span className="smap-code" data-folder={specFolder(spec)}>
        {spec.badge ?? spec.code}
      </span>
      <span className="smap-chip__label">{spec.label}</span>
      <span className="smap-dot" data-state={spec.state} />
    </div>
  );
}
