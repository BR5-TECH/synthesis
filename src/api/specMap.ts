/**
 * The backend operations of the Map tab — all three stubs.
 *
 * No core spec and no `#[tauri::command]` exist for them yet
 * (`SMP-specification-map.md`, `SMO-specification-map-organization.md`,
 * `SMD-specification-map-drafts.md`, each marking its operation a stub). So
 * these wrappers do not call `invoke`, and `./index` does not re-export them:
 * every wrapper there mirrors a registered command, and an `invoke` of a
 * command that does not exist fails at runtime.
 *
 * The indexing backend replaces the bodies below. The signatures are the
 * contract the UI already codes against.
 */
import { logDebug } from "../logging";
import { DEMO_SPECIFICATION_INDEX } from "../state/specMap/fixture";
import { cloneIndex } from "../state/specMap/organize";
import type { SpecificationIndex } from "../state/specMap/types";

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

/** SMP-FR-HWIC: the stub serves a fresh copy of the demo index on every load. */
export async function loadSpecificationMap(): Promise<SpecificationIndex> {
  const index = cloneIndex(DEMO_SPECIFICATION_INDEX);
  logDebug(["frontend"], "specification map stub served the demo index", {
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
