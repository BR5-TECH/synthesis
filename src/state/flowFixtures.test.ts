/**
 * FGV-FR-08, FLO-FR-03 / FGV-FR-01: the two implementations of the Flow schema, read
 * against one set of documents.
 *
 * `../core/FGV-flow-graph-validation.md` defines the shape and the Rust
 * validator judges it; this module serializes to it and deserializes from it.
 * Nothing but a shared body makes the two agree, so every Flow committed to this
 * repository is swept from both sides: the Rust suite asserts each one
 * validates, and this asserts each one parses and re-serializes to itself byte
 * for byte.
 *
 * Byte-for-byte is the strong half of the claim. A file that merely parses
 * proves the reader accepts it; a file that comes back identical proves the
 * WRITER would have produced exactly it — which is what makes the Rust sweep a
 * statement about documents the editor emits rather than about hand-written
 * ones.
 */
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

import { parseFlowDocument, serializeFlowDocument } from "./flowDocument";

/**
 * `resources/flows` holds this project's own workflows; `fixtures/flows` holds
 * the shapes they do not happen to use. Both are swept by the Rust test of the
 * same name, and a directory added to one list belongs in the other.
 */
const DIRECTORIES = ["resources/flows", "fixtures/flows"];

function bodies(): { path: string; body: string }[] {
  const out: { path: string; body: string }[] = [];
  for (const dir of DIRECTORIES) {
    for (const name of readdirSync(dir)) {
      if (!name.endsWith(".flow")) continue;
      const path = join(dir, name);
      out.push({ path, body: readFileSync(path, "utf8") });
    }
  }
  return out;
}

describe("every Flow committed to this repository (FGV-FR-01, FGV-FR-08, FLO-FR-03)", () => {
  it("is a directory this test actually found files in", () => {
    // A sweep over an empty list passes silently, which would make the drift
    // guard read as green while guarding nothing.
    expect(bodies().length).toBeGreaterThan(0);
    for (const dir of DIRECTORIES) {
      expect(
        readdirSync(dir).filter((n) => n.endsWith(".flow")).length,
        `no Flow in ${dir}`,
      ).toBeGreaterThan(0);
    }
  });

  it("parses, and re-serializes to itself byte for byte", () => {
    for (const { path, body } of bodies()) {
      const parsed = parseFlowDocument(body);
      expect(parsed.ok ? "" : parsed.error, path).toBe("");
      if (!parsed.ok) continue;
      expect(serializeFlowDocument(parsed.doc), path).toBe(body);
    }
  });

  // FLO-FR-37: the loop half of the schema — a loop's own artifact references
  // and pass bound, a nested loop, members connected among themselves, and the
  // graph reaching the container rather than its insides (FLO-FR-39).
  it("covers a loop carrying every field a loop can carry", () => {
    const parsed = parseFlowDocument(
      readFileSync("fixtures/flows/Loops.flow", "utf8"),
    );
    if (!parsed.ok) throw new Error(parsed.error);
    const doc = parsed.doc;

    const outer = doc.loops.find((l) => l.id === "l1");
    expect(outer?.artifactIds).toHaveLength(1);
    expect(outer?.maxPasses).toBe(5);
    expect(doc.loops.some((l) => l.parentId === "l1")).toBe(true);
    expect(doc.nodes.some((n) => n.parentId === "l1")).toBe(true);
    // An edge to the container and an edge among its members, which is the
    // whole of what FLO-FR-40 permits.
    expect(doc.edges.some((e) => e.to === "l1")).toBe(true);
    expect(doc.edges.some((e) => e.from === "n2" && e.to === "n3")).toBe(true);
  });
});
