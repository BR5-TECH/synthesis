/**
 * Dependency edges aggregated to the level the map shows
 * (`SME-specification-map-edges.md`).
 *
 * Pure: the paths are SVG path strings in world coordinates, grouped by the
 * class the edge layer styles them with.
 */
import { ancestorAtDepth, isWithin, type TreeIndex } from "../../state/specMap/tree";
import type { MapRef } from "../../state/specMap/types";
import {
  holderRect,
  lastLevelOf,
  type MapLayout,
  type MapScope,
  type Rect,
} from "./layout";

export interface Link {
  from: string;
  to: string;
  citations: number;
}

export type EdgeWeight = "light" | "medium" | "heavy";

export interface MapEdges {
  light: string[];
  medium: string[];
  heavy: string[];
  active: string[];
  unresolved: string[];
  /** SME-FR-WRPX: the active spec and every spec it has a dependency with. */
  related: Set<string> | null;
}

/**
 * SME-FR-FKIW: sum the resolved dependencies between the nodes at one depth,
 * most citations first. An unresolved dependency shows only as its own dashed
 * edge (SME-FR-VAEC), so it adds to no link.
 */
export function aggregateLinks(tree: TreeIndex, depth: number): Link[] {
  const links = new Map<string, Link>();
  for (const dep of tree.index.dependencies) {
    if (dep.unresolved) continue;
    const a = ancestorAtDepth(tree, { kind: "spec", code: dep.from }, depth);
    const b = ancestorAtDepth(tree, { kind: "spec", code: dep.to }, depth);
    if (!a || !b || a === b) continue;
    const key = `${a}>${b}`;
    const link = links.get(key) ?? { from: a, to: b, citations: 0 };
    link.citations += dep.citations;
    links.set(key, link);
  }
  // Array.prototype.sort is stable, so equal citations keep the index order.
  return [...links.values()].sort((x, y) => y.citations - x.citations);
}

/** SME-FR-SGUC */
export function weightOf(citations: number, atLevelZero: boolean): EdgeWeight {
  if (citations >= (atLevelZero ? 60 : 30)) return "heavy";
  if (citations >= (atLevelZero ? 25 : 14)) return "medium";
  return "light";
}

/** SME-FR-LBVO */
export const linkLimit = (atLevelZero: boolean): number => (atLevelZero ? 10 : 14);

const centre = (r: Rect) => ({ x: r.x + r.w / 2, y: r.y + r.h / 2 });

/** SME-FR-DZHE: where the line to `target` crosses a margin 6px outside `box`. */
export function anchor(box: Rect, target: { x: number; y: number }): { x: number; y: number } {
  const c = centre(box);
  const dx = target.x - c.x;
  const dy = target.y - c.y;
  if (!dx && !dy) return c;
  const t = Math.min(
    (box.w / 2 + 6) / Math.abs(dx || 1e-6),
    (box.h / 2 + 6) / Math.abs(dy || 1e-6),
  );
  return { x: c.x + dx * t, y: c.y + dy * t };
}

/** SME-FR-DZHE: a quadratic curve, its control point offset by 0.11 of the line. */
export function curve(a: { x: number; y: number }, b: { x: number; y: number }): string {
  const mx = (a.x + b.x) / 2;
  const my = (a.y + b.y) / 2;
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const k = 0.11;
  const f = (n: number) => n.toFixed(1);
  return `M${f(a.x)} ${f(a.y)}Q${f(mx - dy * k)} ${f(my + dx * k)} ${f(b.x)} ${f(b.y)}`;
}

const between = (a: Rect, b: Rect) => curve(anchor(a, centre(b)), anchor(b, centre(a)));

export interface EdgeInput {
  tree: TreeIndex;
  layout: MapLayout;
  scope: MapScope;
  level: number;
  active: MapRef | null;
  showDeps: boolean;
}

export function computeEdges({ tree, layout, scope, level, active, showDeps }: EdgeInput): MapEdges {
  const out: MapEdges = { light: [], medium: [], heavy: [], active: [], unresolved: [], related: null };
  const lastLevel = lastLevelOf(scope);
  const deps = tree.index.dependencies;

  if (level < lastLevel) {
    if (showDeps) {
      const atLevelZero = level === 0;
      const depth = atLevelZero ? scope.baseDepth : scope.baseDepth + 1;
      const links = aggregateLinks(tree, depth)
        .filter((l) => layout.box.has(l.from) && layout.box.has(l.to))
        .slice(0, linkLimit(atLevelZero));
      for (const link of links) {
        const path = between(layout.box.get(link.from)!, layout.box.get(link.to)!);
        // SME-FR-TJMY: an edge whose end node contains the active node.
        const hot =
          active !== null && (isWithin(tree, active, link.from) || isWithin(tree, active, link.to));
        if (hot) out.active.push(path);
        else out[weightOf(link.citations, atLevelZero)].push(path);
      }
    }
  } else if (active?.kind === "spec") {
    // SME-FR-WRPX: only the active spec's own dependencies, chip to chip.
    const related = new Set([active.code]);
    for (const dep of deps) {
      if (dep.from !== active.code && dep.to !== active.code) continue;
      related.add(dep.from === active.code ? dep.to : dep.from);
      if (!showDeps || dep.unresolved) continue;
      const a = layout.chip.get(dep.from);
      const b = layout.chip.get(dep.to);
      if (a && b) out.active.push(curve(centre(a), centre(b)));
    }
    out.related = related;
  }

  // SME-FR-VAEC: unresolved citations draw at every level, outside the limits.
  for (const dep of deps) {
    if (!dep.unresolved) continue;
    const a = holderRect(tree, layout, { kind: "spec", code: dep.from });
    const b = holderRect(tree, layout, { kind: "spec", code: dep.to });
    if (!a || !b || a.key === b.key) continue;
    const chips = a.key.startsWith("chip:") && b.key.startsWith("chip:");
    out.unresolved.push(chips ? curve(centre(a), centre(b)) : between(a, b));
  }
  return out;
}
