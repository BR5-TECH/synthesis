/**
 * What a merge run says about itself
 * (`../../../specifications/ui/GRU-graduation-runs.md` GRU-FR-HDPQ,
 * GRU-FR-NWEC, GRU-FR-AJGM, GRU-FR-JRMA, GRU-FR-QMWX).
 *
 * A merge run is a graduation run that carries `merge` data. It has no draft,
 * so every sentence here names the stream and the base branch and none names a
 * draft (GRU-FR-ZKYU). Pure functions of a run as the backend reports it.
 */

import type {
  GraduationMergeData,
  GraduationMergePublication,
  GraduationRun,
} from "../../types/graduation";

/** GRD-FR-MRNQ: a run is a merge run when its record carries `merge` data. */
export function isMergeRun(run: GraduationRun): boolean {
  return Boolean(run.merge);
}

/**
 * GRU-FR-HDPQ: the title of a run.
 *
 * A merge run's title is `merge.name`. A draft run's is its draft's name. The
 * two are never mixed: a merge run names no draft.
 */
export function runTitle(run: GraduationRun): string {
  return run.merge?.name ?? run.input?.draftName ?? "";
}

/**
 * GRU-FR-QMWX: what a run that failed on `merge_branch_moved` states, and what
 * a Continue or an answer refused with that code states (GRU-FR-EMNV).
 */
export const MERGE_BRANCH_MOVED_STATEMENT =
  "The stream branch or the base branch moved after the merge was handed off. Nothing was written to either branch. Start the merge again from the work stream selector.";

/** GRU-FR-DBUS: what each blocker of a merge run's apply states. */
export const MERGE_BLOCKER_SENTENCES: Record<string, string> = {
  merge_dirty_side:
    "A working copy of the stream or of the base branch holds uncommitted changes, so the merge could not be applied.",
  merge_guard_held:
    "Another merge or update of the repository holds the repository, so the merge could not be applied.",
  merge_apply_failed: "The application could not write the merge result.",
};

/** GRU-FR-DBUS: what every apply blocker says about what was and was not done. */
export const MERGE_BLOCKER_TAIL =
  "Nothing was written to either branch. No new pass is spent. Continue retries the apply.";

/** GRU-FR-NWEC: the publication choice, in words. */
export function publicationSentence(
  publication: GraduationMergePublication,
): string {
  return publication.kind === "commit"
    ? "The result is committed under the author's message."
    : "The result is left uncommitted in the base worktree.";
}

/** GRU-FR-UKNC / GRU-FR-NWEC: the provenance line of a merge run. */
export function mergeProvenanceLine(run: GraduationRun): string {
  const merge = run.merge!;
  return `Merges the work stream “${run.streamName}” (branch “${merge.streamBranch}”) into the base branch “${merge.baseBranch}”.`;
}

/**
 * GRU-FR-AJGM: what either side did to each path Git could not settle.
 *
 * Keyed by path. The change words are the backend's four (`created`,
 * `updated`, `deleted`, `unchanged`); a value it does not send reads as
 * unknown rather than as a guess.
 */
export function fateByPath(merge: GraduationMergeData): Record<string, string> {
  const fates: Record<string, string> = {};
  for (const conflict of merge.conflicts) {
    fates[conflict.path] =
      `${merge.baseBranch}: ${conflict.baseChange || "unknown"} · stream: ${conflict.streamChange || "unknown"}`;
  }
  return fates;
}

/** GRU-FR-JRMA: the first seven characters of a commit id. */
export function shortCommit(id: string): string {
  return id.slice(0, 7);
}

/**
 * GRU-FR-JRMA: where the merge landed. Null before the run has a result, so a
 * run that is not `completed` renders none.
 */
export function mergeResultSentence(run: GraduationRun): string | null {
  const merge = run.merge;
  if (!merge?.result || run.state !== "completed") return null;
  if (merge.result.published === "commit") {
    return `Merged into ${merge.baseBranch} as a commit.`;
  }
  return `Merged into ${merge.baseBranch} and left as an uncommitted change in the base worktree.`;
}

/**
 * GRU-FR-QYEE / GRU-FR-QMWX: the one line a failed run states.
 *
 * A merge run that failed because a branch moved states that, in words this
 * surface owns, rather than the backend's own message.
 */
export function failureStatement(run: GraduationRun): string {
  if (run.merge && run.failure?.code === "merge_branch_moved") {
    return MERGE_BRANCH_MOVED_STATEMENT;
  }
  return run.failure?.message ?? "The run failed.";
}
