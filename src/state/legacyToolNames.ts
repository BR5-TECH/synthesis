/**
 * The legacy names of the agent tools
 * (`../../specifications/core/PVC-product-vocabulary.md` PVC-FR-LGYT,
 * PVC-FR-PXOE, PVC-FR-JBPC, and its **Legacy names** table).
 *
 * A call that an earlier build saved carries the name that the tool had then.
 * A saved activity row shows such a call with the tool's current name. This
 * is the mirror of `src-tauri/src/tools/legacy_names.rs`: the two must hold
 * the same rows.
 *
 * Three earlier OpenSpec names are now the current names of the change tools.
 * So the name alone does not tell which tool an old call used: the target
 * kind of its conversation tells it (PVC-FR-PXOE). The function here is pure.
 */
import type { DiscussionTarget } from "../types";

/** The names that map on every target kind. */
const EVERY_KIND: ReadonlyMap<string, string> = new Map([
  ["search_drafts", "search_changes"],
  ["read_draft", "read_change"],
  ["propose_draft_changes", "propose_change_edits"],
  ["decide_draft_changes", "decide_change_edits"],
  ["propose_new_draft", "propose_new_change"],
]);

/**
 * The names that map only in a conversation on an OpenSpec change (target
 * kind `change`). Such a conversation never has the change tools of
 * PVC-FR-HMJD, so there these names are the old OpenSpec names.
 */
const OPENSPEC_CHANGE: ReadonlyMap<string, string> = new Map([
  ["propose_change_edits", "propose_openspec_change_edits"],
  ["decide_change_edits", "decide_openspec_change_edits"],
  ["propose_new_change", "propose_new_openspec_change"],
  ["request_change_action", "request_openspec_change_action"],
]);

/**
 * PVC-FR-LGYT, PVC-FR-PXOE: the current name of a saved call of `tool` in a
 * conversation on a target of `targetKind`. A name that is not a legacy name
 * for that target kind is returned as it is. With no target kind known, only
 * the rows of every kind apply.
 */
export function currentToolName(tool: string, targetKind?: DiscussionTarget["kind"]): string {
  const everyKind = EVERY_KIND.get(tool);
  if (everyKind !== undefined) return everyKind;
  if (targetKind === "change") return OPENSPEC_CHANGE.get(tool) ?? tool;
  return tool;
}
