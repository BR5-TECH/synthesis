/**
 * What the inspector shows for the selection (`SMI-specification-map-inspector.md`).
 * Pure: built from the tree, the roll-ups and the selection alone.
 */
import {
  ancestorIds,
  indexChildren,
  projectPath,
  specChildren,
  specFolder,
  specsUnder,
  type TreeIndex,
} from "../../state/specMap/tree";
import { deleteOutcome } from "../../state/specMap/organize";
import { EMPTY_ROLLUP, rollupOfSpec, type Rollup } from "../../state/specMap/rollups";
import {
  changeLocationLabel,
  isOpenSpecIndex,
  isSpecNode,
  type ChangeChip,
  type IndexNode,
  type MapRef,
  type MapIssue,
} from "../../state/specMap/types";
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
  /** SMI-FR-BWIY: null where the index holds no dependency to list. */
  dependsOn: InspectorRow[] | null;
  attention: string[];
}

export type InspectorModel =
  /** SMO-FR-RVAV: `newRootLabel` is null where the map offers no organization. */
  | { kind: "empty"; newRootLabel: string | null }
  | ({
      kind: "index";
      id: string;
      openLabel: string;
      newChildLabel: string | null;
      canDelete: boolean;
      /** SMO-FR-RVAV: whether Rename and Delete are offered. */
      organize: boolean;
    } & Detail)
  | ({ kind: "spec"; code: string } & Detail)
  | { kind: "planned"; kindName: string; draftId: string; name: string; holderLabel: string }
  | {
      kind: "change";
      kindName: string;
      chip: ChangeChip;
      locationLabel: string;
      holderLabel: string;
      attention: string[];
    };

/** SMI-FR-BXET: the line of one issue of a spec node or a change chip. */
const issueLine = (issue: MapIssue) => `${issue.level} — ${issue.message}`;

/**
 * SMI-FR-BXET: up to four issues of an index node and of the nodes beneath
 * it, in index order, each named by the label of the node that holds it.
 */
function issuesBeneath(node: IndexNode): string[] {
  const out: string[] = [];
  const walk = (n: IndexNode) => {
    for (const issue of n.issues ?? []) out.push(`${n.label} — ${issue.message}`);
    for (const child of n.children) {
      if (isSpecNode(child)) {
        for (const issue of child.issues ?? []) out.push(`${child.label} — ${issue.message}`);
      } else {
        walk(child);
      }
    }
  };
  walk(node);
  return out.slice(0, 4);
}

const capitalize = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);

export function inspectorModel(
  tree: TreeIndex,
  rollups: Map<string, Rollup>,
  selection: MapRef | null,
): InspectorModel {
  const levels = tree.index.levels;
  const openspec = isOpenSpecIndex(tree.index);
  const empty: InspectorModel = {
    kind: "empty",
    newRootLabel: openspec ? null : `New ${levels[0]?.singular ?? "node"}`,
  };
  if (!selection) return empty;
  const labelOf = (id: string) => tree.nodes.get(id)?.label ?? "";

  if (selection.kind === "change") {
    // SMI-FR-ZMZB: the change, its location, and the capability that holds it.
    const chip = tree.changes.get(selection.key);
    const holder = tree.changeParent.get(selection.key);
    if (!chip || !holder) return empty;
    return {
      kind: "change",
      kindName: "change",
      chip,
      locationLabel: changeLocationLabel(chip),
      holderLabel: labelOf(holder),
      attention: (chip.issues ?? []).map(issueLine),
    };
  }

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
      badge: { text: spec.badge ?? spec.code, folder: specFolder(spec) },
      title: spec.label,
      summary: spec.summary,
      rollup: rollupOfSpec(spec),
      totals: `${spec.requirements} requirements · ${spec.scenarios} scenarios`,
      // SMI-FR-CEVL: a spec node's one child row is its path.
      childHeading: "Path",
      children: [
        { key: "path", ref: null, label: projectPath(tree.index, spec), badge: null, count: "", rollup: null, danger: false },
      ],
      // SMI-FR-YHNT / SMI-FR-PNGA / SMI-FR-BWIY
      dependsOn: openspec ? null : outgoing.map((d) => {
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
      // SMI-FR-ZLOA / SMI-FR-BXET
      attention: openspec
        ? (spec.issues ?? []).map(issueLine)
        : outgoing
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
        label: `${s.badge ?? s.code} · ${s.label}`,
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
    dependsOn: openspec ? null : aggregateLinks(tree, depth)
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
    // SMI-FR-ZLOA / SMI-FR-BXET
    attention: openspec
      ? issuesBeneath(node)
      : specsUnder(tree, node.id)
          .filter((s) => s.state === "gap")
          .slice(0, 4)
          .map((s) => `${s.code} — ${s.label}: no coverage yet`),
    openLabel: `Open ${levels[depth]?.singular ?? ""}`,
    // SMO-FR-UAKE / SMO-FR-RVAV
    newChildLabel:
      !openspec && depth < tree.leafDepth ? `New ${levels[depth + 1]?.singular ?? ""}` : null,
    canDelete: deleteOutcome(tree, node.id).allowed,
    organize: !openspec,
  };
}
