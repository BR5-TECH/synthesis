/** Serialization (FLO NFR: stable key order and formatting). */

import type { FlowDocument } from "./types";

/**
 * Serialize a graph back to the file body.
 *
 * Fields are written in a fixed order and absent optionals are omitted rather
 * than emitted as `null`, so a Flow that was opened and not edited round-trips
 * byte-for-byte and a Flow's Git history shows only what the author actually
 * changed (FLO NFR). `JSON.stringify` preserves insertion order for string
 * keys, so building each object in the canonical order is what pins it — never
 * rely on the order the parsed document happened to arrive in.
 */
export function serializeFlowDocument(doc: FlowDocument): string {
  const out = {
    version: doc.version,
    // FLO-FR-25: always written, empty or not — a Flow's name is a field it has,
    // and a file that omits it when unset would read as one that predates it.
    name: doc.name,
    ...(doc.description ? { description: doc.description } : {}),
    // FLO-FR-37: absent from the file on a Flow holding no loop, so a Flow
    // authored before loops existed round-trips byte-for-byte.
    ...(doc.loops.length > 0
      ? {
          loops: doc.loops.map((l) => ({
            id: l.id,
            name: l.name,
            ...(l.artifactIds && l.artifactIds.length > 0
              ? { artifactIds: l.artifactIds }
              : {}),
            ...(l.maxPasses !== undefined ? { maxPasses: l.maxPasses } : {}),
            ...(l.parentId !== undefined ? { parentId: l.parentId } : {}),
            position: { x: l.position.x, y: l.position.y },
            size: { width: l.size.width, height: l.size.height },
          })),
        }
      : {}),
    nodes: doc.nodes.map((n) => ({
      id: n.id,
      name: n.name,
      ...(n.artifactIds && n.artifactIds.length > 0
        ? { artifactIds: n.artifactIds }
        : {}),
      ...(n.prompt !== undefined ? { prompt: n.prompt } : {}),
      ...(n.parentId !== undefined ? { parentId: n.parentId } : {}),
      position: { x: n.position.x, y: n.position.y },
    })),
    edges: doc.edges.map((e) => ({
      id: e.id,
      from: e.from,
      to: e.to,
      ...(e.label !== undefined ? { label: e.label } : {}),
    })),
  };
  // A trailing newline: the file is committed and diffable, and a
  // newline-terminated one does not show up as "\ No newline at end of file".
  return `${JSON.stringify(out, null, 2)}\n`;
}
