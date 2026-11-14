/**
 * Serialization of a Flow document (FLO NFR: stable key order and formatting).
 *
 * Each object is built in the key order of the backend model
 * (`src-tauri/src/flow_validation/model.rs`), and an absent optional field is
 * left out rather than written as `null` or as an empty list. So a Flow that was
 * opened and not edited writes back byte for byte, and its Git history shows
 * only what the author changed. No layout is written (FLO-FR-21). `JSON.stringify` keeps the insertion order of
 * string keys, so the order in which each object is built is the order written.
 */

import type { FlowDocument, FlowEdge, FlowItem } from "./types";

function item(i: FlowItem): Record<string, unknown> {
  return {
    id: i.id,
    kind: i.kind,
    name: i.name,
    ...(i.description !== undefined ? { description: i.description } : {}),
    ...(i.artifactIds && i.artifactIds.length > 0
      ? { artifactIds: [...i.artifactIds] }
      : {}),
    ...(i.parentId !== undefined ? { parentId: i.parentId } : {}),
    ...(i.draftId !== undefined ? { draftId: i.draftId } : {}),
    ...(i.footprint && i.footprint.length > 0
      ? { footprint: [...i.footprint] }
      : {}),
    ...(i.due !== undefined ? { due: i.due } : {}),
  };
}

function edge(e: FlowEdge): Record<string, unknown> {
  return { id: e.id, from: e.from, to: e.to, type: e.type };
}

/** FLO-FR-03: the file body of a plan. */
export function serializeFlowDocument(doc: FlowDocument): string {
  const out = {
    version: doc.version,
    // FLO-FR-25: always written, empty or not.
    name: doc.name,
    ...(doc.description ? { description: doc.description } : {}),
    // FLO-FR-YDWT: absent from the file when the Flow has no reference.
    ...(doc.references.length > 0 ? { references: [...doc.references] } : {}),
    items: doc.items.map(item),
    edges: doc.edges.map(edge),
  };
  // A trailing newline: the file is committed, and Git shows a file without
  // one as "\ No newline at end of file".
  return `${JSON.stringify(out, null, 2)}\n`;
}
