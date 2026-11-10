/**
 * Serialization of a Flow document (FLO NFR: stable key order and formatting).
 *
 * Each object is built in the key order of the backend model
 * (`src-tauri/src/flow_validation/model.rs`), and an absent optional field is
 * left out rather than written as `null` or as an empty list. So a Flow that was
 * opened and not edited writes back byte for byte, and its Git history shows
 * only what the author changed. `JSON.stringify` keeps the insertion order of
 * string keys, so the order in which each object is built is the order written.
 */

import type {
  FlowDocument,
  FlowEdge,
  FlowItem,
  FlowSync,
  SyncBase,
  TrackerRef,
} from "./types";

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
    ...(i.localOnly !== undefined ? { localOnly: i.localOnly } : {}),
    ...(i.draftId !== undefined ? { draftId: i.draftId } : {}),
    ...(i.footprint && i.footprint.length > 0
      ? { footprint: [...i.footprint] }
      : {}),
    ...(i.due !== undefined ? { due: i.due } : {}),
    position: { x: i.position.x, y: i.position.y },
    ...(i.size !== undefined
      ? { size: { width: i.size.width, height: i.size.height } }
      : {}),
  };
}

function edge(e: FlowEdge): Record<string, unknown> {
  return { id: e.id, from: e.from, to: e.to, type: e.type };
}

function ref(r: TrackerRef): Record<string, unknown> {
  return {
    kind: r.kind,
    host: r.host,
    owner: r.owner,
    repo: r.repo,
    number: r.number,
    url: r.url,
  };
}

/** A base keeps `null` where it holds `null`; only an absent field is left out. */
function base(b: SyncBase): Record<string, unknown> {
  return {
    name: b.name,
    description: b.description,
    state: b.state,
    ...(b.type !== undefined ? { type: b.type } : {}),
    ...(b.parent !== undefined
      ? { parent: b.parent === null ? null : ref(b.parent) }
      : {}),
    ...(b.milestone !== undefined ? { milestone: b.milestone } : {}),
    ...(b.blockedBy !== undefined ? { blockedBy: b.blockedBy.map(ref) } : {}),
    ...(b.due !== undefined ? { due: b.due } : {}),
  };
}

function sync(s: FlowSync): Record<string, unknown> {
  return {
    tracker: s.tracker,
    links: s.links.map((l) => ({
      itemId: l.itemId,
      ref: ref(l.ref),
      base: base(l.base),
    })),
    // FGV-FR-01: written after `links`, and absent when empty.
    ...(s.ignored && s.ignored.length > 0 ? { ignored: s.ignored.map(ref) } : {}),
  };
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
    ...(doc.sync !== undefined ? { sync: sync(doc.sync) } : {}),
  };
  // A trailing newline: the file is committed, and Git shows a file without
  // one as "\ No newline at end of file".
  return `${JSON.stringify(out, null, 2)}\n`;
}
