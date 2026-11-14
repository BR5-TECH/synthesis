/**
 * The specification index the Map tab renders (`SMP-specification-map.md`).
 *
 * The shapes mirror what `"load specification map"` returns (SMP UI contract
 * boundary). A spec node is a leaf; every other node is an index node.
 */

import type { OpenSpecFailure } from "../../types/openspec";

/** SMP-FR-TGYU: the one completeness state every spec node carries. */
export type CompletenessState = "verified" | "built" | "drafted" | "gap";

/** The state order every completeness bar draws its segments in. */
export const COMPLETENESS_STATES: readonly CompletenessState[] = [
  "verified",
  "built",
  "drafted",
  "gap",
];

/** SMP UI contract boundary: one issue the OpenSpec validation reported. */
export interface MapIssue {
  level: "ERROR" | "WARNING" | "INFO";
  message: string;
}

/** OSI-FR-ALQI: where the change of a change chip lives. */
export type ChangeChipLocation =
  | { kind: "active_worktree" }
  | { kind: "stream"; streamId: string; streamName: string };

/** OSI-FR-ALQI: one active change on one capability, in one location. */
export interface ChangeChip {
  change: string;
  location: ChangeChipLocation;
  /** OSI-FR-GRCV: the issues of the change, in the active worktree alone. */
  issues?: MapIssue[];
}

export interface SpecNode {
  /** The spec code, for example `STB`, or `<capability-id>#<n>` in OpenSpec mode. */
  code: string;
  /** SMN-FR-PMZM: the text of the code badge, where it is not the code. */
  badge?: string;
  /**
   * Relative to `specifications/` in the demo index, for example
   * `ui/STB-status-bar.md`, and to the project root in an OpenSpec index.
   */
  path: string;
  label: string;
  summary: string;
  requirements: number;
  scenarios: number;
  state: CompletenessState;
  /** OSI-FR-SBVF: the issues of an OpenSpec requirement. */
  issues?: MapIssue[];
}

export interface IndexNode {
  id: string;
  label: string;
  summary: string;
  /** The swatch hue of a root node, as its dark-theme value. */
  hue?: string;
  children: IndexNode[] | SpecNode[];
  /**
   * OSI-FR-VOOS: a capability that a change names and the live specs do not
   * hold. The wire field is `planned: true`.
   */
  plannedCapability?: boolean;
  /** OSI-FR-SBVF: the issues of an OpenSpec capability. */
  issues?: MapIssue[];
  /** OSI-FR-ALQI: the change chips of an OpenSpec capability. */
  changes?: ChangeChip[];
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

/** SMP-FR-HWIC: where the index came from. Absent means the demo index. */
export type MapSource = "demo" | "openspec";

export interface SpecificationIndex {
  levels: LevelName[];
  roots: IndexNode[];
  dependencies: Dependency[];
  source?: MapSource;
  /** OSI-FR-OIOB: the result of the validation of an OpenSpec index. */
  validation?: "valid" | "invalid" | "unavailable";
  /** SMP-FR-KVKD: the failure of a validation that is `unavailable`, or null. */
  validationFailure?: OpenSpecFailure | null;
  /** OSI-FR-DENT: the locations whose changes could not be read. */
  unreadLocations?: number;
}

/** SMP-FR-MNZW: whether the index is the index of the project's OpenSpec specs. */
export const isOpenSpecIndex = (index: SpecificationIndex): boolean =>
  index.source === "openspec";

/** SMI-FR-ZMZB: the location of a change chip, as the author reads it. */
export function changeLocationLabel(chip: ChangeChip): string {
  return chip.location.kind === "stream" ? chip.location.streamName : "active worktree";
}

/** SMN-FR-YEVY: one change in one location, whichever capability shows it. */
export function changeIdentity(chip: ChangeChip): string {
  const where =
    chip.location.kind === "stream" ? `stream:${chip.location.streamId}` : "active_worktree";
  return `${chip.change}@${where}`;
}

/**
 * SMI-FR-ZMZB: the stable key of a change chip, unique in one index. A change
 * that touches two capabilities gives one chip on each, so the key names the
 * capability node too.
 */
export function changeChipKey(nodeId: string, chip: ChangeChip): string {
  return `change:${nodeId}|${changeIdentity(chip)}`;
}

/** What a selection or a hover names. */
export type MapRef =
  | { kind: "index"; id: string }
  | { kind: "spec"; code: string }
  | { kind: "change"; key: string };

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
    case "change":
      return ref.key;
  }
}

export function sameRef(a: MapRef | null, b: MapRef | null): boolean {
  if (!a || !b) return a === b;
  return refKey(a) === refKey(b);
}
