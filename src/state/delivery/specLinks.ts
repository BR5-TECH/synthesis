/**
 * The links between the items of the delivery plan and the spec nodes of the
 * specification index (`SDL-specification-delivery-links.md`).
 *
 * Pure: derived from the plan and the index alone, each time one of them
 * changes (SDL-FR-NXXQ). The links are never stored, so they cannot go stale
 * against what they come from (SDL NFR).
 */
import { childrenOf, type FlowDocument, type FlowItem } from "../flowDocument";
import { projectPath, specsUnder, type TreeIndex } from "../specMap/tree";
import type { MapRef } from "../specMap/types";

export interface SpecLinks {
  /**
   * SDL-FR-TWOV / SDL-FR-QUUA: the spec codes each item links, its own and
   * those of every item under it, in index order. An item with no link has
   * no entry.
   */
  byItem: ReadonlyMap<string, readonly string[]>;
  /** SDL-FR-VAGU / SDL-FR-SGQM: the items that link each spec code, in plan order. */
  bySpec: ReadonlyMap<string, readonly string[]>;
}

export const NO_LINKS: SpecLinks = Object.freeze({
  byItem: new Map<string, readonly string[]>(),
  bySpec: new Map<string, readonly string[]>(),
});

/**
 * FOA-FR-XRBA: an entry that ends in `/` names a folder and covers itself and
 * every path below it. Any other entry covers only itself. Paths compare
 * segment by segment and are case-sensitive.
 */
export function footprintCovers(entry: string, path: string): boolean {
  if (entry.endsWith("/")) return path === entry || path.startsWith(entry);
  return path === entry;
}

/**
 * SDL-FR-TWOV / SDL-FR-OCPB: the spec codes an item links by itself, through
 * a reference (the path of its artifact id) or a footprint entry that covers
 * the spec node's file.
 */
function ownLinks(item: FlowItem, files: readonly { code: string; path: string }[]): Set<string> {
  const refs = new Set(item.artifactIds ?? []);
  const footprint = item.footprint ?? [];
  const out = new Set<string>();
  for (const { code, path } of files) {
    if (refs.has(path) || footprint.some((entry) => footprintCovers(entry, path))) out.add(code);
  }
  return out;
}

/** SDL-FR-TWOV .. SDL-FR-QUUA: the links of every item of `doc` against `tree`. */
export function deriveSpecLinks(doc: FlowDocument | null, tree: TreeIndex | null): SpecLinks {
  if (!doc || !tree || doc.items.length === 0 || tree.specs.size === 0) return NO_LINKS;
  const files = [...tree.specs.values()].map((spec) => ({
    code: spec.code,
    path: projectPath(tree.index, spec),
  }));
  const order = new Map(files.map((f, i) => [f.code, i]));
  const own = new Map(doc.items.map((item) => [item.id, ownLinks(item, files)]));

  // SDL-FR-QUUA: a container holds the links of every item under it.
  const all = new Map<string, Set<string>>();
  const collect = (item: FlowItem, seen: Set<string>): Set<string> => {
    const done = all.get(item.id);
    if (done) return done;
    const out = new Set(own.get(item.id));
    if (!seen.has(item.id)) {
      seen.add(item.id);
      for (const child of childrenOf(doc, item.id)) {
        for (const code of collect(child, seen)) out.add(code);
      }
    }
    all.set(item.id, out);
    return out;
  };
  for (const item of doc.items) collect(item, new Set());

  const byItem = new Map<string, readonly string[]>();
  const bySpec = new Map<string, string[]>();
  for (const item of doc.items) {
    const codes = [...(all.get(item.id) ?? [])].sort(
      (a, b) => (order.get(a) ?? 0) - (order.get(b) ?? 0),
    );
    if (codes.length === 0) continue;
    byItem.set(item.id, codes);
    for (const code of codes) {
      const list = bySpec.get(code) ?? [];
      list.push(item.id);
      bySpec.set(code, list);
    }
  }
  return { byItem, bySpec };
}

/**
 * SDL-FR-VAGU: the items that link a subject — a spec node, or an index node
 * through each spec node under it — once each, in plan order.
 */
export function itemsLinking(
  links: SpecLinks,
  tree: TreeIndex,
  doc: FlowDocument,
  subject: MapRef,
): string[] {
  const codes =
    subject.kind === "spec"
      ? [subject.code]
      : subject.kind === "index"
        ? specsUnder(tree, subject.id).map((s) => s.code)
        : [];
  const linked = new Set<string>();
  for (const code of codes) for (const id of links.bySpec.get(code) ?? []) linked.add(id);
  return doc.items.filter((item) => linked.has(item.id)).map((item) => item.id);
}

/**
 * SDL-FR-CFHH: the files the task that **Plan work** adds references, once
 * each, in index order: the file of a spec node subject, or of each spec node
 * of a leaf group subject.
 */
export function planWorkFiles(tree: TreeIndex, subject: MapRef): string[] {
  const specs =
    subject.kind === "spec"
      ? [tree.specs.get(subject.code)].filter((s) => s !== undefined)
      : subject.kind === "index"
        ? specsUnder(tree, subject.id)
        : [];
  return [...new Set(specs.map((s) => projectPath(tree.index, s)))];
}

/**
 * SDL-FR-KXCY: whether a spec chip stays at full opacity while **unplanned**
 * is on: a gap or a drafted spec node that no delivery item links.
 */
export function isUnplanned(links: SpecLinks, code: string, state: string): boolean {
  return (state === "gap" || state === "drafted") && !links.bySpec.has(code);
}

/** SDL-FR-KXCY: whether an index node holds an unplanned spec chip beneath it. */
export function holdsUnplanned(links: SpecLinks, tree: TreeIndex, id: string): boolean {
  return specsUnder(tree, id).some((s) => isUnplanned(links, s.code, s.state));
}
