/**
 * The source of **Create a PR** of a stream row
 * (`../../../specifications/ui/WSS-work-stream-selector.md` WSS-FR-KHGP,
 * WSS-FR-ALEU; `CPR-create-pull-request.md` CPR-FR-MBTA).
 *
 * The stack of the stream's branch is read first. When it names a parent, the
 * parent is the base; any other answer, a failure included, keeps the base
 * branch the stream records.
 */
import * as api from "../../api";
import { stackParentOf } from "../../hooks/useCurrentBranchPullRequest";
import { logWarn } from "../../logging";
import type { WorkStreamSummary } from "../../types";
import type { PullRequestSource } from "../CreatePullRequest/types";
import { parseRejection } from "../Git/errors";

export async function streamPullRequestSource(
  summary: WorkStreamSummary,
): Promise<PullRequestSource> {
  const { stream } = summary;
  const stack = await api.getBranchStack(stream.branch).catch((error: unknown) => {
    logWarn(["frontend", "remote"], "stream branch stack read failed", {
      streamId: stream.id,
      error: parseRejection(error).code,
    });
    return null;
  });
  const parent = stackParentOf(stack ?? null, stream.branch);
  return {
    head: stream.branch,
    base: parent ?? stream.baseBranch,
    title: stream.name,
    ...(parent ? { stackParent: parent } : {}),
  };
}
