/**
 * The backend operations of the map session of the Map tab.
 *
 * `"load specification map"` is a command of
 * `../../specifications/core/OSI-openspec-spec-index.md`, and
 * `"get delivery source"` of `../../specifications/core/DSR-delivery-source.md`
 * (SMP-FR-TKUS: the default lens). `./index` does not
 * re-export this module, so the map session takes its operations from here
 * alone and a test replaces them as one set.
 */
import { invoke } from "@tauri-apps/api/core";

import { logDebug } from "../logging";
import type { DeliverySource } from "../types/delivery";
import { getDeliverySource } from "./delivery";
import { DEMO_SPECIFICATION_INDEX } from "../state/specMap/fixture";
import { cloneIndex } from "../state/specMap/tree";
import { toOpenSpecFailure } from "../state/openspec/failureNotice";
import type {
  ChangeChip,
  IndexNode,
  LevelName,
  MapIssue,
  SpecNode,
  SpecificationIndex,
} from "../state/specMap/types";

/** OSI contract: an index node as the wire carries it. */
export interface OpenSpecWireNode {
  id: string;
  label: string;
  summary: string;
  hue?: string;
  children: OpenSpecWireNode[] | SpecNode[];
  /** OSI-FR-VOOS: a planned capability node. */
  planned?: boolean;
  issues?: MapIssue[];
  changes?: ChangeChip[];
}

/** OSI contract: `OpenSpecMapIndex`, camelCase on the wire. */
export interface OpenSpecMapIndex {
  levels: LevelName[];
  roots: OpenSpecWireNode[];
  dependencies: SpecificationIndex["dependencies"];
  validation: "valid" | "invalid" | "unavailable";
  /** OSI-FR-OIOB / SMP-FR-KVKD: the failure of a validation that is unavailable. */
  validationFailure?: unknown;
  unreadLocations: number;
}

/** OSI-FR-OSWD / SMP-FR-HWIC: what `"load specification map"` answers. */
export type SpecificationMapLoad =
  | { source: "demo" }
  | { source: "openspec"; index: OpenSpecMapIndex };

export interface SpecMapOps {
  /** `"load specification map"` */
  load: () => Promise<SpecificationIndex>;
  /** `"get delivery source"` (SMP-FR-TKUS) */
  getDeliverySource: () => Promise<DeliverySource>;
}

function fromWireNode(node: OpenSpecWireNode): IndexNode {
  const { planned, children, ...rest } = node;
  const out: IndexNode = {
    ...rest,
    children: (children as Array<OpenSpecWireNode | SpecNode>).map((child) =>
      "code" in child ? { ...child } : fromWireNode(child),
    ) as IndexNode[] | SpecNode[],
  };
  // OSI-FR-VOOS: the wire's planned capability.
  if (planned === true) out.plannedCapability = true;
  return out;
}

/** SMP-FR-MNZW: the OpenSpec index in the shape the map session holds. */
export function fromOpenSpecIndex(index: OpenSpecMapIndex): SpecificationIndex {
  return {
    levels: index.levels.map((l) => ({ ...l })),
    roots: index.roots.map(fromWireNode),
    dependencies: index.dependencies.map((d) => ({ ...d })),
    source: "openspec",
    validation: index.validation,
    // SMP-FR-KVKD: the failure of a validation that could not run, for the
    // sentence of its notice and its Details.
    ...(index.validation === "unavailable" && index.validationFailure
      ? { validationFailure: toOpenSpecFailure(index.validationFailure) }
      : {}),
    unreadLocations: index.unreadLocations,
  };
}

/**
 * SMP-FR-HWIC / SMP-FR-MNZW: load the index. For `demo` the UI serves a fresh
 * copy of its built-in demo index on every load. A failure rejects with an
 * `OpenSpecFailure` (OSI-FR-PJJO, OFP-FR-QLQJ).
 */
export async function loadSpecificationMap(): Promise<SpecificationIndex> {
  const answer = await invoke<SpecificationMapLoad>("load_specification_map");
  if (answer.source === "openspec") return fromOpenSpecIndex(answer.index);
  const index = cloneIndex(DEMO_SPECIFICATION_INDEX);
  index.source = "demo";
  logDebug(["frontend"], "specification map served the demo index", {
    roots: index.roots.length,
    dependencies: index.dependencies.length,
  });
  return index;
}

export const specMapOps: SpecMapOps = {
  load: loadSpecificationMap,
  getDeliverySource,
};
