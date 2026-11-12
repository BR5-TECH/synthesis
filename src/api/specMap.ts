/**
 * The backend operations of the Map tab.
 *
 * `"load specification map"` is a command of
 * `../../specifications/core/OSI-openspec-spec-index.md`. The other two are
 * stubs: no core spec and no `#[tauri::command]` exist for them yet
 * (`SMO-specification-map-organization.md`, `SMD-specification-map-drafts.md`,
 * each marking its operation a stub), so their wrappers do not call `invoke`.
 * `./index` does not re-export this module, so the map session takes its
 * operations from here alone and a test replaces them as one set.
 */
import { invoke } from "@tauri-apps/api/core";

import { logDebug } from "../logging";
import { DEMO_SPECIFICATION_INDEX } from "../state/specMap/fixture";
import { cloneIndex } from "../state/specMap/organize";
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

/** SMO UI contract boundary: the one edit `"save specification map organization"` takes. */
export type OrganizationEdit =
  | {
      kind: "create";
      parentId: string | null;
      node: { id: string; label: string; summary: string };
    }
  | { kind: "edit"; nodeId: string; label: string; summary: string }
  | { kind: "move"; nodeId: string; parentId: string | null; index: number }
  | { kind: "delete"; nodeId: string };

export interface SpecMapOps {
  /** `"load specification map"` */
  load: () => Promise<SpecificationIndex>;
  /** `"save specification map organization"` */
  saveOrganization: (edit: OrganizationEdit) => Promise<void>;
  /** `"attach draft to specification map node"` */
  attachDraft: (args: { draftId: string; nodeId: string }) => Promise<void>;
}

function fromWireNode(node: OpenSpecWireNode): IndexNode {
  const { planned, children, ...rest } = node;
  const out: IndexNode = {
    ...rest,
    children: (children as Array<OpenSpecWireNode | SpecNode>).map((child) =>
      "code" in child ? { ...child } : fromWireNode(child),
    ) as IndexNode[] | SpecNode[],
  };
  // The session keeps the drafts of SMD-FR-OYLC in `planned`, so the wire's
  // planned capability takes a name of its own.
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

/** SMO-FR-CZLA: the stub keeps no state. Only the edit kind is logged. */
export async function saveSpecificationMapOrganization(
  edit: OrganizationEdit,
): Promise<void> {
  logDebug(["frontend"], "specification map organization stub received an edit", {
    kind: edit.kind,
  });
}

/** SMD-FR-OYLC: the stub keeps no state. */
export async function attachDraftToSpecificationMapNode(_args: {
  draftId: string;
  nodeId: string;
}): Promise<void> {
  logDebug(["frontend"], "specification map draft placement stub received a placement", {});
}

export const specMapOps: SpecMapOps = {
  load: loadSpecificationMap,
  saveOrganization: saveSpecificationMapOrganization,
  attachDraft: attachDraftToSpecificationMapNode,
};
