/**
 * The legacy names of the agent tools
 * (`../../specifications/core/PVC-product-vocabulary.md` PVC-FR-LGYT,
 * PVC-FR-PXOE, and its **Legacy names** table).
 */
import { describe, expect, it } from "vitest";

import { TOOL_ACTIVITY_STATUS, toolActivityStatus } from "./agentActivity";
import { currentToolName } from "./legacyToolNames";
import type { DiscussionTarget } from "../types";

const KINDS: DiscussionTarget["kind"][] = ["draft", "artifact", "note", "change", "map"];
const OTHER_KINDS = KINDS.filter((kind) => kind !== "change");

const EVERY_KIND_ROWS: Array<[string, string]> = [
  ["search_drafts", "search_changes"],
  ["read_draft", "read_change"],
  ["propose_draft_changes", "propose_change_edits"],
  ["decide_draft_changes", "decide_change_edits"],
  ["propose_new_draft", "propose_new_change"],
];

const OPENSPEC_ROWS: Array<[string, string]> = [
  ["propose_change_edits", "propose_openspec_change_edits"],
  ["decide_change_edits", "decide_openspec_change_edits"],
  ["propose_new_change", "propose_new_openspec_change"],
  ["request_change_action", "request_openspec_change_action"],
];

describe("PVC-FR-LGYT, PVC-FR-PXOE: the current name of a saved call", () => {
  it("PVC-FR-LGYT: each of the five draft-tool rows maps on every target kind", () => {
    for (const [legacy, current] of EVERY_KIND_ROWS) {
      for (const kind of [...KINDS, undefined]) {
        expect(currentToolName(legacy, kind)).toBe(current);
      }
    }
  });

  it("PVC-FR-PXOE: the four OpenSpec rows map only on the target kind change", () => {
    for (const [legacy, current] of OPENSPEC_ROWS) {
      expect(currentToolName(legacy, "change")).toBe(current);
      for (const kind of [...OTHER_KINDS, undefined]) {
        expect(currentToolName(legacy, kind)).toBe(legacy);
      }
    }
  });

  it("PVC-FR-PXOE: a draft-tool row maps once, not again through an OpenSpec row", () => {
    // propose_draft_changes becomes propose_change_edits, which on the
    // target kind change is an OpenSpec legacy name. It does not map twice.
    expect(currentToolName("propose_draft_changes", "change")).toBe("propose_change_edits");
    expect(currentToolName("propose_new_draft", "change")).toBe("propose_new_change");
  });

  it("PVC-FR-PXOE: any other name keeps its name on every target kind", () => {
    for (const tool of ["read_file", "search_changes", "propose_openspec_change_edits", "mcp__x__y", ""]) {
      for (const kind of [...KINDS, undefined]) {
        expect(currentToolName(tool, kind)).toBe(tool);
      }
    }
  });

  it("PVC-FR-JBPC: a mapped draft-tool name reads a named status", () => {
    for (const [legacy, current] of EVERY_KIND_ROWS) {
      if (TOOL_ACTIVITY_STATUS[current] === undefined) continue;
      expect(toolActivityStatus(currentToolName(legacy, "draft"))).toBe(toolActivityStatus(legacy));
    }
  });
});
