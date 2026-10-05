/**
 * The world layout of the specification map at one detail level
 * (`SMN-specification-map-nodes.md`, `SMZ-specification-map-zoom.md` SMZ-FR-RNIT).
 *
 * Pure: the same scope, level and canvas width give the same layout. Every
 * coordinate is a world coordinate, centred on the content's bounding box, so
 * the stage places the middle of the content at the middle of the canvas.
 *
 * The layout walks whatever depth the index has. Roles, not level names,
 * decide a node's geometry: a root node, a branch node, or a leaf group.
 */
import {
  indexChildren,
  nodeOfRef,
  specChildren,
  type NodeRole,
  type TreeIndex,
} from "../../state/specMap/tree";
import type { IndexNode, LevelName, MapRef } from "../../state/specMap/types";

export const CHIP_W = 96;
export const CHIP_H = 22;
export const CHIP_GAP = 6;
export const CHIPS_PER_ROW = 3;
export const CHIP_ROW = CHIP_H + CHIP_GAP;
export const CHIP_TOP = 62;
export const INSET = 10;
export const OVERVIEW_W = 300;
export const OVERVIEW_H = 176;
export const OVERVIEW_GAP = 30;
export const EXPANDED_GAP = 34;
export const BRANCH_COLLAPSED_H = 116;
export const LEAF_COLLAPSED_H = 64;

/** What the map shows: the whole index, or the subtree of a focused node. */
export interface MapScope {
  roots: IndexNode[];
  /** The level names from the scope's own depth to the last level. */
  levels: LevelName[];
  baseDepth: number;
  leafDepth: number;
}

export type BoxShape = "overview" | "expanded" | "collapsed";

export interface LayoutBox {
  id: string;
  depth: number;
  relDepth: number;
  role: NodeRole;
  shape: BoxShape;
  parentId: string | null;
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface LayoutChip {
  kind: "spec" | "planned";
  /** The spec code, or the draft id. */
  key: string;
  parentId: string;
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface MapLayout {
  boxes: LayoutBox[];
  chips: LayoutChip[];
  width: number;
  height: number;
  box: Map<string, LayoutBox>;
  chip: Map<string, LayoutChip>;
}

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** SMZ-FR-FOXU: the whole index, or only the subtree of the focused node. */
export function scopeOf(tree: TreeIndex, focusId: string | null): MapScope {
  const focused = focusId ? tree.nodes.get(focusId) : undefined;
  if (focused) {
    const depth = tree.depth.get(focused.id)!;
    return {
      roots: [focused],
      levels: tree.index.levels.slice(depth),
      baseDepth: depth,
      leafDepth: tree.leafDepth,
    };
  }
  return {
    roots: tree.index.roots,
    levels: tree.index.levels,
    baseDepth: 0,
    leafDepth: tree.leafDepth,
  };
}

export const lastLevelOf = (scope: MapScope): number => scope.levels.length - 1;

/** SMN-FR-XCOK: 3 columns from 1020px of canvas, 2 from 660px, 1 below. */
export function columnsFor(canvasWidth: number): number {
  if (canvasWidth >= 1020) return 3;
  if (canvasWidth >= 660) return 2;
  return 1;
}

/** SMN-FR-TNDA: `320 + 20 × (G − d)`, G being the leaf-group depth. */
export function nodeWidth(scope: MapScope, depth: number): number {
  return 320 + 20 * (scope.leafDepth - depth);
}

export function layoutMap(
  scope: MapScope,
  level: number,
  canvasWidth: number,
): MapLayout {
  const boxes: LayoutBox[] = [];
  const chips: LayoutChip[] = [];

  const chipAt = (
    kind: LayoutChip["kind"],
    key: string,
    parentId: string,
    left: number,
    top: number,
    i: number,
  ) =>
    chips.push({
      kind,
      key,
      parentId,
      x: left + (i % CHIPS_PER_ROW) * (CHIP_W + CHIP_GAP),
      y: top + Math.floor(i / CHIPS_PER_ROW) * CHIP_ROW,
      w: CHIP_W,
      h: CHIP_H,
    });

  const place = (
    node: IndexNode,
    relDepth: number,
    x: number,
    y: number,
    parentId: string | null,
  ): number => {
    const depth = scope.baseDepth + relDepth;
    const role: NodeRole =
      depth === scope.leafDepth ? "leafGroup" : relDepth === 0 ? "root" : "branch";
    const overview = level === 0 && relDepth === 0;
    const box: LayoutBox = {
      id: node.id,
      depth,
      relDepth,
      role,
      shape: "expanded",
      parentId,
      x,
      y,
      w: overview ? OVERVIEW_W : nodeWidth(scope, depth),
      h: 0,
    };
    boxes.push(box);

    // SMZ-FR-RNIT: a node at the level's depth renders collapsed.
    if (relDepth === level) {
      box.shape = overview ? "overview" : "collapsed";
      box.h = overview
        ? OVERVIEW_H
        : role === "leafGroup"
          ? LEAF_COLLAPSED_H
          : BRANCH_COLLAPSED_H;
      return box.h;
    }

    const planned = node.planned ?? [];
    if (role === "leafGroup") {
      // SMN-FR-ISBE / SMD-FR-CGNW: spec chips, then planned chips, in one grid.
      const specs = specChildren(node);
      specs.forEach((s, i) => chipAt("spec", s.code, node.id, x + INSET, y + CHIP_TOP, i));
      planned.forEach((p, i) =>
        chipAt("planned", p.draftId, node.id, x + INSET, y + CHIP_TOP, specs.length + i),
      );
      const rows = Math.ceil((specs.length + planned.length) / CHIPS_PER_ROW);
      box.h = LEAF_COLLAPSED_H + rows * CHIP_ROW + 4;
      return box.h;
    }

    // SMN-FR-TNDA: children stack inset, from 46px under a root node and 52px
    // under a branch node, 10px apart under a root node and 8px elsewhere.
    const start = relDepth === 0 ? 46 : 52;
    const gap = relDepth === 0 ? 10 : 8;
    const tail = relDepth === 0 ? 6 : 8;
    let cursor = start;
    for (const child of indexChildren(node)) {
      cursor += place(child, relDepth + 1, x + INSET, y + cursor, node.id) + gap;
    }
    // SMD-FR-ULTF: planned chips after the children, and the node grows to hold them.
    planned.forEach((p, i) => chipAt("planned", p.draftId, node.id, x + INSET, y + cursor, i));
    cursor += Math.ceil(planned.length / CHIPS_PER_ROW) * CHIP_ROW;
    box.h = cursor + tail;
    return box.h;
  };

  // SMN-FR-XCOK / SMN-FR-JEQO: the n-th root takes column n mod columns.
  const columns = columnsFor(canvasWidth);
  const columnWidth = level === 0 ? OVERVIEW_W : nodeWidth(scope, scope.baseDepth);
  const gap = level === 0 ? OVERVIEW_GAP : EXPANDED_GAP;
  const columnY = new Array<number>(columns).fill(0);
  scope.roots.forEach((root, i) => {
    const column = i % columns;
    const h = place(root, 0, column * (columnWidth + gap), columnY[column], null);
    columnY[column] += h + gap;
  });

  let width = 0;
  let height = 0;
  for (const b of boxes) {
    if (b.relDepth !== 0) continue;
    width = Math.max(width, b.x + b.w);
    height = Math.max(height, b.y + b.h);
  }
  const ox = -width / 2;
  const oy = -height / 2;
  for (const item of [...boxes, ...chips]) {
    item.x += ox;
    item.y += oy;
  }
  return {
    boxes,
    chips,
    width,
    height,
    box: new Map(boxes.map((b) => [b.id, b])),
    chip: new Map(chips.map((c) => [c.key, c])),
  };
}

/**
 * The rendered node that holds `ref`: its own chip or box when one renders,
 * otherwise the deepest rendered box above it.
 */
export function holderRect(
  tree: TreeIndex,
  layout: MapLayout,
  ref: MapRef,
): (Rect & { key: string }) | null {
  if (ref.kind !== "index") {
    const key = ref.kind === "spec" ? ref.code : ref.draftId;
    const chip = layout.chip.get(key);
    if (chip) return { ...chip, key: `chip:${key}` };
  }
  let id = nodeOfRef(tree, ref);
  while (id !== null) {
    const box = layout.box.get(id);
    if (box) return { ...box, key: `box:${id}` };
    id = tree.parent.get(id) ?? null;
  }
  return null;
}

/**
 * SMI-FR-RPCO: the level at which `ref` renders collapsed — its own depth for
 * an index node, the last level for a chip — or null outside the scope.
 */
export function levelFor(tree: TreeIndex, scope: MapScope, ref: MapRef): number | null {
  const holder = nodeOfRef(tree, ref);
  if (holder === null) return null;
  const inScope = scope.roots.some((root) => {
    let id: string | null = holder;
    while (id !== null) {
      if (id === root.id) return true;
      id = tree.parent.get(id) ?? null;
    }
    return false;
  });
  if (!inScope) return null;
  if (ref.kind !== "index") return lastLevelOf(scope);
  return (tree.depth.get(ref.id) ?? 0) - scope.baseDepth;
}
