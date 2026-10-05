/**
 * Comparison identity for the Changes panel and the Diff tabs it opens.
 *
 * A Diff tab is identified by the pair of its file and its comparison
 * (CHG-FR-19): requesting one that is already open jumps focus to it, while the
 * same file under a *different* comparison opens a second tab. That rule is one
 * key function, kept here so the panel that opens tabs and the shell that keys
 * them cannot drift apart.
 */
import type { Comparison, DiffScope, DiffTarget } from "./types";

/** Typed backend errors the panel renders as inline states (CHC contract). */
export const ERR_NOT_A_REPOSITORY = "not a git repository";
export const ERR_UNKNOWN_BRANCH = "unknown branch";

/** A stable, collision-free key for a comparison. */
export function comparisonKey(comparison: Comparison): string {
  return comparison.kind === "branch"
    ? `branch:${comparison.targetBranch}`
    : "uncommitted";
}

/** Human-readable name for a comparison, shown in the Diff tab's header. */
export function comparisonLabel(comparison: Comparison): string {
  return comparison.kind === "branch"
    ? `against ${comparison.targetBranch}`
    : "uncommitted";
}

/**
 * The `get_diff` scope for one file under a comparison (CHG-FR-18).
 *
 * `previousPath` travels with a renamed entry so the backend sees both halves
 * of the rename and can pair them; it is not part of the tab's identity.
 */
export function diffScopeFor(
  comparison: Comparison,
  path: string,
  previousPath?: string,
): DiffScope {
  return comparison.kind === "branch"
    ? {
        kind: "branch",
        path,
        targetBranch: comparison.targetBranch,
        previousPath,
      }
    : { kind: "path", path, previousPath };
}

/** The same key, derived from a scope rather than a comparison. */
export function scopeKey(scope: DiffScope): string {
  switch (scope.kind) {
    case "branch":
      return `branch:${scope.targetBranch}`;
    case "staged":
      return "staged";
    default:
      return "uncommitted";
  }
}

/**
 * A Diff tab's id — the (file, comparison) pair, and nothing else (CHG-FR-19).
 * The `diff:` namespace keeps it distinct from the `art:` ids Editor and Flow
 * tabs use, which is what lets a Diff tab coexist with a live Editor tab for the
 * same artifact without violating the single-tab rule (CHG-FR-20 / TAB-FR-06).
 */
export function diffTabId(target: DiffTarget): string {
  return `diff:${scopeKey(target.scope)}:${target.path}`;
}
