import { describe, expect, it } from "vitest";

import {
  ARTIFACT_TYPES,
  DEFAULT_TYPE_LENS,
  filterToLens,
  LENS_ALL_ARTIFACTS_LABEL,
  LENS_ALL_FILES_LABEL,
  lensToFilter,
  presentTypes,
  typeLensPositions,
  type TypeLens,
} from "./artifactTypes";
import type { ArtifactType, ArtifactTypeFilter } from "./types";

/**
 * The artifact-type lens crosses the IPC boundary under a different spelling
 * from the one the components use — `all_artifacts` / `all_files` persisted
 * (PSS-FR-18) versus `artifacts` / `files` in memory. A divergence here is
 * silent at runtime: the panel would restore a lens that matches nothing and
 * the project would look empty, which is exactly the failure LIB-FR-14 is
 * supposed to prevent.
 */
describe("artifact-type lens <-> persisted filter (PSS-FR-18 / LIB-FR-14)", () => {
  it("maps the two sentinels to their persisted spellings", () => {
    expect(lensToFilter("artifacts")).toBe("all_artifacts");
    expect(lensToFilter("files")).toBe("all_files");
    expect(filterToLens("all_artifacts")).toBe("artifacts");
    expect(filterToLens("all_files")).toBe("files");
  });

  it("passes each of the eight built-in types through unchanged", () => {
    for (const { value } of ARTIFACT_TYPES) {
      expect(lensToFilter(value)).toBe(value);
      expect(filterToLens(value)).toBe(value);
    }
  });

  it("round-trips every lens the single-select can hold", () => {
    const lenses: TypeLens[] = [
      "artifacts",
      ...ARTIFACT_TYPES.map((t) => t.value),
      "files",
    ];
    for (const lens of lenses) {
      expect(filterToLens(lensToFilter(lens))).toBe(lens);
    }
  });

  it("falls back to the default lens for a value this build does not know", () => {
    // A record written by a newer build. Rendering it as a type that matches
    // nothing would look like an empty project, so it resolves to the default
    // (LIB-FR-12) instead.
    expect(filterToLens("all_diagrams" as ArtifactTypeFilter)).toBe(
      DEFAULT_TYPE_LENS,
    );
    expect(filterToLens(undefined)).toBe(DEFAULT_TYPE_LENS);
  });

  it("keeps the default lens as the All artifacts sentinel", () => {
    // LIB-FR-12 / PSS-FR-18 name the same default on both sides of the wire.
    expect(DEFAULT_TYPE_LENS).toBe("artifacts");
    expect(lensToFilter(DEFAULT_TYPE_LENS)).toBe("all_artifacts");
  });

  /**
   * The persisted vocabulary, spelled out. A round-trip test cannot catch a
   * divergence from the Rust side — if the backend serialized `"instruction"`
   * where this side expects `"instructions"`, `filterToLens` would quietly fall
   * back to the default lens and every test here would still pass. This literal
   * list is asserted against the same eight strings in
   * `src-tauri/src/project_settings.rs`'s `the_wire_vocabulary_is_exactly_these_ten_strings`,
   * so a rename on either side breaks a named assertion rather than silently
   * degrading at runtime.
   */
  it("pins the exact wire vocabulary shared with the Rust side", () => {
    expect([
      "all_artifacts",
      ...ARTIFACT_TYPES.map((t) => lensToFilter(t.value)),
      "all_files",
    ]).toEqual([
      "all_artifacts",
      "skill",
      "agent",
      "prompt",
      "spec",
      "flow",
      "instructions",
      "scenario",
      "scratchpad",
      "all_files",
    ]);
  });

  /**
   * Every menu and dropdown label in the app reads with each word capitalised, and
   * these two are shared by the Library and the Changes panel (CHG-FR-14). A label
   * typed at each `<option>` instead of read from here is a label that drifts on
   * one surface and not the other — visible to the user, invisible to every other
   * test.
   */
  it("spells the two sentinel lenses in title case, for both surfaces to read", () => {
    expect(LENS_ALL_ARTIFACTS_LABEL).toBe("All Artifacts");
    expect(LENS_ALL_FILES_LABEL).toBe("All Files");
    // And every type label in the table follows the same rule.
    for (const t of ARTIFACT_TYPES) {
      for (const word of t.label.split(/[\s(]+/)) {
        if (!/^[A-Za-z]/.test(word)) continue;
        expect(word[0]).toBe(word[0].toUpperCase());
      }
    }
  });
});

// ---------------------------------------------------------------------------
// LIB-FR-19 (and CHG-FR-14, which adopts it): the lens offers the types the
// content in front of the author actually has.
// ---------------------------------------------------------------------------

describe("presentTypes", () => {
  it("collects the resolved types, skipping the unclassified", () => {
    expect([
      ...presentTypes(["skill", undefined, "spec", null, "skill"]),
    ]).toEqual(["skill", "spec"]);
  });

  it("is empty for a collection with nothing classified", () => {
    expect(presentTypes([undefined, undefined]).size).toBe(0);
    expect(presentTypes([]).size).toBe(0);
  });
});

describe("typeLensPositions", () => {
  const values = (present: ArtifactType[], active: TypeLens) =>
    typeLensPositions(new Set(present), active).map((p) => p.value);

  it("bounds the row with the two sentinels and nothing else when nothing is classified", () => {
    // A project of tooling files alone offers the escape hatch and the default,
    // and no type button that would narrow to an empty tree.
    expect(values([], DEFAULT_TYPE_LENS)).toEqual(["artifacts", "files"]);
  });

  it("orders the present types by the built-in table, not by discovery order", () => {
    // `scenario` precedes nothing in the fixture but follows `skill` in
    // ARTIFACT_TYPES, so a row built by appending as types are met would differ.
    expect(values(["scenario", "skill"], DEFAULT_TYPE_LENS)).toEqual([
      "artifacts",
      "skill",
      "scenario",
      "files",
    ]);
  });

  it("keeps the active type in place after its last file leaves", () => {
    // SNV-FR-63's exemption, at its source: the author's lens is still in the
    // row — in its own ordered position — so the tree narrows to nothing rather
    // than the panel changing lens on their behalf.
    expect(values(["spec"], "skill")).toEqual([
      "artifacts",
      "skill",
      "spec",
      "files",
    ]);
  });

  it("adds no type button for a sentinel lens", () => {
    expect(values([], "files")).toEqual(["artifacts", "files"]);
    expect(values([], "artifacts")).toEqual(["artifacts", "files"]);
  });

  it("carries each type's chip, label, and type attribute", () => {
    const skill = typeLensPositions(new Set(["skill" as ArtifactType]), "artifacts")
      .find((p) => p.value === "skill")!;
    const table = ARTIFACT_TYPES.find((t) => t.value === "skill")!;
    expect(skill).toMatchObject({
      tag: table.chip,
      title: table.label,
      dataType: "skill",
    });

    // The sentinels name no type, so they carry none — the row must not paint
    // them in some artifact's colour.
    const positions = typeLensPositions(new Set(), "artifacts");
    expect(positions[0]).toMatchObject({
      value: "artifacts",
      title: LENS_ALL_ARTIFACTS_LABEL,
    });
    expect(positions[0].dataType).toBeUndefined();
    expect(positions[1]).toMatchObject({
      value: "files",
      title: LENS_ALL_FILES_LABEL,
    });
    expect(positions[1].dataType).toBeUndefined();
  });
});
