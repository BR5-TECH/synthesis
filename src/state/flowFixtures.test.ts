/**
 * FLO-FR-03, FGV-FR-01, FLO NFR on round-trips: the committed Flow fixtures,
 * read by this module's reader and written back by its writer.
 *
 * A file that only parses proves that the reader accepts it. A file that comes
 * back byte for byte proves that the writer would make exactly that file.
 */
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

import { ITEM_KINDS, parseFlowDocument, serializeFlowDocument } from "./flowDocument";

/** Version 2 Flows that exist to be read by both sides of the schema. */
const DIRECTORY = "fixtures/flows";

function bodies(): { path: string; body: string }[] {
  return readdirSync(DIRECTORY)
    .filter((n) => n.endsWith(".flow"))
    .map((n) => {
      const path = join(DIRECTORY, n);
      return { path, body: readFileSync(path, "utf8") };
    });
}

describe("every committed Flow fixture (FGV-FR-01, FLO-FR-03)", () => {
  it("FGV-FR-01: the sweep finds files", () => {
    // A sweep over an empty list passes and guards nothing.
    expect(bodies().length).toBeGreaterThan(0);
  });

  it("FLO-FR-03, FLO NFR: each parses, and re-serializes to itself byte for byte", () => {
    for (const { path, body } of bodies()) {
      const parsed = parseFlowDocument(body);
      expect(parsed.ok ? "" : parsed.error, path).toBe("");
      if (!parsed.ok) continue;
      expect(serializeFlowDocument(parsed.doc), path).toBe(body);
    }
  });

  it("FGV-FR-01: the fixtures use every field of the schema", () => {
    const docs = bodies().map(({ body }) => {
      const r = parseFlowDocument(body);
      if (!r.ok) throw new Error(r.error);
      return r.doc;
    });
    const items = docs.flatMap((d) => d.items);
    expect(new Set(items.map((i) => i.kind))).toEqual(new Set(ITEM_KINDS));
    for (const field of [
      "description",
      "artifactIds",
      "parentId",
      "localOnly",
      "draftId",
      "footprint",
      "due",
      "size",
    ] as const) {
      expect(items.some((i) => i[field] !== undefined), field).toBe(true);
    }
    const edges = docs.flatMap((d) => d.edges);
    expect(new Set(edges.map((e) => e.type))).toEqual(new Set(["blocks", "relates"]));
    expect(docs.some((d) => d.references.length > 0 && d.description)).toBe(true);
    const links = docs.flatMap((d) => d.sync?.links ?? []);
    expect(new Set(links.map((l) => l.ref.kind))).toEqual(new Set(["issue", "milestone"]));
    expect(links.some((l) => l.base.parent)).toBe(true);
    expect(links.some((l) => (l.base.blockedBy ?? []).length > 0)).toBe(true);
    expect(docs.some((d) => (d.sync?.ignored ?? []).length > 0), "sync.ignored").toBe(true);
  });
});
