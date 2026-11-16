/**
 * The node references of the Map (`MPR-map-plan-proposals.md` MPR-FR-CEKS): the
 * way a node of either lens is named to the Discussion column
 * (`SMP-specification-map.md` SMP-FR-IHFM, SMP-FR-DUUU).
 */
import type { MapNodeRef } from "../../types";
import type { TreeIndex } from "./tree";
import type { MapRef } from "./types";

/** MDL-FR-QELI: the reference of an item or a milestone of the delivery plan. */
export function deliveryNodeOf(item: { id: string; kind: string; name: string }): MapNodeRef {
  return {
    lens: "delivery",
    kind: item.kind === "milestone" ? "milestone" : "item",
    id: item.id,
    label: item.name === "" ? item.id : item.name,
  };
}

/**
 * SMN-FR-BSZV: the reference of a node of the Specifications lens, or null for
 * a node that has no thread, such as a change chip.
 */
export function specNodeOf(tree: TreeIndex, ref: MapRef | null): MapNodeRef | null {
  if (ref === null) return null;
  if (ref.kind === "index") {
    const node = tree.nodes.get(ref.id);
    return node ? { lens: "specifications", kind: "index", id: ref.id, label: node.label } : null;
  }
  if (ref.kind === "spec") {
    const spec = tree.specs.get(ref.code);
    return spec ? { lens: "specifications", kind: "spec", id: ref.code, label: spec.label } : null;
  }
  return null;
}

/** MPV-FR-NMYT: the id of the plan item of an issue number. */
export function issueItemId(issueNumber: number): string {
  return `issue-${issueNumber}`;
}
