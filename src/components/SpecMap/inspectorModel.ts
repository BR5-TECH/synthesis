/**
 * What the inspector shows for the selection (`SMI-specification-map-inspector.md`).
 * Pure: built from the tree, the roll-ups and the selection alone.
 */
import {
  SPEC_ROOT,
  ancestorIds,
  indexChildren,
  specChildren,
  specFolder,
  specsUnder,
  type TreeIndex,
} from "../../state/specMap/tree";
import { deleteOutcome } from "../../state/specMap/organize";
import { EMPTY_ROLLUP, rollupOfSpec, type Rollup } from "../../state/specMap/rollups";
import type { MapRef } from "../../state/specMap/types";
import { aggregateLinks } from "./edges";

export interface InspectorRow {
  key: string;
  /** What a click on the row selects, or null for a row that selects nothing. */
  ref: MapRef | null;
  label: string;
  badge: { text: string; folder: string } | null;
  count: string;
  rollup: Rollup | null;
  danger: boolean;
}

interface Detail {
  ref: MapRef;
  kindName: string;
  breadcrumb: string;
  badge: { text: string; folder: string };
  title: string;
  summary: string;
  rollup: Rollup;
  totals: string;
  childHeading: string;
  children: InspectorRow[];
  dependsOn: InspectorRow[];
  attention: string[];
}

export type InspectorModel =
  | { kind: "empty"; newRootLabel: string }
  | ({
      kind: "index";
      id: string;
      openLabel: string;
      newChildLabel: string | null;
      canDelete: boolean;
    } & Detail)
  | ({ kind: "spec"; code: string } & Detail)
  | { kind: "planned"; kindName: string; draftId: string; name: string; holderLabel: string };

const capitalize = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);

export function inspectorModel(
  tree: TreeIndex,
  rollups: Map<string, Rollup>,
  selection: MapRef | null,
): InspectorModel {
  const levels = tree.index.levels;
  const empty: InspectorModel = {
    kind: "empty",
    newRootLabel: `New ${levels[0]?.singular ?? "node"}`,
  };
  if (!selection) return empty;
  const labelOf = (id: string) => tree.nodes.get(id)?.label ?? "";

  if (selection.kind === "planned") {
    const draft = tree.planned.get(selection.draftId);
    const holder = tree.plannedParent.get(selection.draftId);
    if (!draft || !holder) return empty;
    return {
      kind: "planned",
      kindName: "draft",
      draftId: draft.draftId,
      name: draft.name,
      holderLabel: labelOf(holder),
    };
  }

  if (selection.kind === "spec") {
    const spec = tree.specs.get(selection.code);
    if (!spec) return empty;
    const parent = tree.specParent.get(spec.code)!;
    const outgoing = tree.index.dependencies
      .filter((d) => d.from === spec.code)
      .sort((a, b) => b.citations - a.citations);
    return {
      kind: "spec",
      code: spec.code,
      ref: selection,
      kindName: levels[tree.leafDepth + 1]?.singular ?? "spec",
      breadcrumb: [...ancestorIds(tree, parent), parent].map(labelOf).join(" → "),
      badge: { text: spec.code, folder: specFolder(spec) },
      title: spec.label,
      summary: spec.summary,
      rollup: rollupOfSpec(spec),
      totals: `${spec.requirements} requirements · ${spec.scenarios} scenarios`,
      // SMI-FR-CEVL: a spec node's one child row is its path.
      childHeading: "Path",
      children: [
        { key: "path", ref: null, label: SPEC_ROOT + spec.path, badge: null, count: "", rollup: null, danger: false },
      ],
      // SMI-FR-YHNT / SMI-FR-PNGA
      dependsOn: outgoing.map((d) => {
        const target = tree.specs.get(d.to);
        return {
          key: `dep:${d.to}`,
          ref: d.unresolved ? null : { kind: "spec", code: d.to },
          label: target?.label ?? d.to,
          badge: { text: d.to, folder: target ? specFolder(target) : "dir" },
          count: String(d.citations),
          rollup: null,
          danger: !!d.unresolved,
        };
      }),
      // SMI-FR-ZLOA
      attention: outgoing
        .filter((d) => d.unresolved)
        .map((d) => `cites ${d.to} — identifier resolves to nothing`),
    };
  }

  const node = tree.nodes.get(selection.id);
  if (!node) return empty;
  const depth = tree.depth.get(node.id)!;
  const r = rollups.get(node.id) ?? EMPTY_ROLLUP;
  const leaf = depth === tree.leafDepth;
  const children: InspectorRow[] = leaf
    ? specChildren(node).map((s) => ({
        key: `spec:${s.code}`,
        ref: { kind: "spec", code: s.code },
        label: `${s.code} · ${s.label}`,
        badge: null,
        count: "1",
        rollup: rollupOfSpec(s),
        danger: false,
      }))
    : indexChildren(node).map((c) => {
        const cr = rollups.get(c.id) ?? EMPTY_ROLLUP;
        return {
          key: `index:${c.id}`,
          ref: { kind: "index", id: c.id },
          label: c.label,
          badge: null,
          count: String(cr.specs),
          rollup: cr,
          danger: false,
        };
      });
  return {
    kind: "index",
    id: node.id,
    ref: selection,
    kindName: levels[depth]?.singular ?? "",
    breadcrumb: depth === 0 ? "specifications" : ancestorIds(tree, node.id).map(labelOf).join(" → "),
    badge: { text: `${r.specs} specs`, folder: "ui" },
    title: node.label,
    summary: node.summary,
    rollup: r,
    totals: `${r.requirements} requirements · ${r.scenarios} scenarios`,
    childHeading: capitalize(levels[depth + 1]?.plural ?? ""),
    children,
    dependsOn: aggregateLinks(tree, depth)
      .filter((l) => l.from === node.id)
      .map((l) => ({
        key: `link:${l.to}`,
        ref: { kind: "index", id: l.to },
        label: labelOf(l.to),
        badge: { text: String(rollups.get(l.to)?.specs ?? 0), folder: "dir" },
        count: String(l.citations),
        rollup: null,
        danger: false,
      })),
    attention: specsUnder(tree, node.id)
      .filter((s) => s.state === "gap")
      .slice(0, 4)
      .map((s) => `${s.code} — ${s.label}: no coverage yet`),
    openLabel: `Open ${levels[depth]?.singular ?? ""}`,
    newChildLabel: depth < tree.leafDepth ? `New ${levels[depth + 1]?.singular ?? ""}` : null,
    canDelete: deleteOutcome(tree, node.id).allowed,
  };
}
