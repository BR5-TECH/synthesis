/**
 * The rules for a temporary stream
 * (`../../../specifications/ui/WSS-work-stream-selector.md` WSS-FR-PRVK,
 * WSS-FR-GBTD, WSS-FR-GQXN).
 *
 * A temporary stream is merged and deleted when its work is done. These
 * functions read a project's runs and decide two things: whether the work of a
 * stream is done, and what the default message of its merge says.
 */

import type { GraduationRun, WorkStream } from "../../types";

/**
 * WKS-FR-NDSV: a run belongs to a stream when it is assigned to it, or when it
 * is a direct run that targets its working copy.
 */
export function runsOfStream(
  runs: readonly GraduationRun[],
  stream: Pick<WorkStream, "id" | "worktreePath">,
): GraduationRun[] {
  return runs.filter(
    (run) =>
      run.streamId === stream.id ||
      (run.directTarget?.worktreePath ?? null) === stream.worktreePath,
  );
}

/**
 * WSS-FR-PRVK / WSS-FR-GBTD: the work of a stream is done when it has a run
 * and every run of it is `completed`. A run that is failed, discarded, queued,
 * working or waiting on the author is work that is not done.
 */
export function streamWorkIsDone(
  runs: readonly GraduationRun[],
  stream: Pick<WorkStream, "id" | "worktreePath">,
): boolean {
  const own = runsOfStream(runs, stream);
  return own.length > 0 && own.every((run) => run.state === "completed");
}

/**
 * WSS-FR-GQXN: one line for each run of the stream, in the project's run
 * order, each as `- ` and the draft name of the run. A run with no draft name,
 * a merge run for example, has no line.
 */
export function temporaryMergeMessage(
  runs: readonly GraduationRun[],
  stream: Pick<WorkStream, "id" | "worktreePath">,
): string {
  return runsOfStream(runs, stream)
    .map((run) => (run.input?.draftName ?? "").replace(/\s+/g, " ").trim())
    .filter((name) => name !== "")
    .map((name) => `- ${name}`)
    .join("\n");
}
