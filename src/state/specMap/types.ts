/**
 * The specification index the Map tab renders (`SMP-specification-map.md`).
 *
 * The shapes mirror what `"load specification map"` returns (SMP UI contract
 * boundary). A spec node is a leaf; every other node is an index node.
 */

/** SMP-FR-TGYU: the one completeness state every spec node carries. */
export type CompletenessState = "verified" | "built" | "drafted" | "gap";

/** The state order every completeness bar draws its segments in. */
export const COMPLETENESS_STATES: readonly CompletenessState[] = [
  "verified",
  "built",
  "drafted",
  "gap",
];

export interface SpecNode {
  /** The spec code, for example `STB`. */
  code: string;
  /** Relative to `specifications/`, for example `ui/STB-status-bar.md`. */
  path: string;
  label: string;
  summary: string;
  requirements: number;
  scenarios: number;
  state: CompletenessState;
}

/** SMD-FR-OYLC: a draft placed on an index node, named by its id. */
export interface PlannedDraft {
  draftId: string;
  name: string;
}

export interface IndexNode {
  id: string;
  label: string;
  summary: string;
  /** The swatch hue of a root node, as its dark-theme value. */
  hue?: string;
  children: IndexNode[] | SpecNode[];
  planned?: PlannedDraft[];
}

/** SMP-FR-QMRE: the names the index gives one level. */
export interface LevelName {
  plural: string;
  singular: string;
}

export interface Dependency {
  from: string;
  to: string;
  citations: number;
  unresolved?: boolean;
}

export interface SpecificationIndex {
  levels: LevelName[];
  roots: IndexNode[];
  dependencies: Dependency[];
}

/** What a selection, a hover, or a drag names. */
export type MapRef =
  | { kind: "index"; id: string }
  | { kind: "spec"; code: string }
  | { kind: "planned"; draftId: string };

/** Every change the map session applies to its index. */
export type MapEdit =
  | {
      kind: "create";
      parentId: string | null;
      node: { id: string; label: string; summary: string };
    }
  | { kind: "edit"; nodeId: string; label: string; summary: string }
  | { kind: "move"; ref: MapRef; parentId: string | null; index: number }
  | { kind: "delete"; nodeId: string }
  | { kind: "attachDraft"; nodeId: string; draft: PlannedDraft }
  | { kind: "renameDraft"; draftId: string; name: string }
  | { kind: "detachDraft"; draftId: string };

export function isSpecNode(node: IndexNode | SpecNode): node is SpecNode {
  return "code" in node;
}

/** A stable string for a ref, for keys and comparisons. */
export function refKey(ref: MapRef): string {
  switch (ref.kind) {
    case "index":
      return `index:${ref.id}`;
    case "spec":
      return `spec:${ref.code}`;
    case "planned":
      return `planned:${ref.draftId}`;
  }
}

export function sameRef(a: MapRef | null, b: MapRef | null): boolean {
  if (!a || !b) return a === b;
  return refKey(a) === refKey(b);
}
