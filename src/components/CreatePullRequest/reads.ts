/**
 * The reads the Create a PR window makes when it opens, besides the head state
 * (`../../../specifications/ui/CPR-create-pull-request.md` CPR-FR-IDRP,
 * CPR-FR-IPKX, CPR-FR-IOGF, CPR-FR-WCBQ).
 *
 * Each read is made once per opening. An answer that arrives after the window
 * closed is discarded (CPR non-functional). The combine of CPR-FR-WCBQ is the
 * one exception: it is written to the saved state of the head branch, so it
 * runs whether the window is still open or not.
 */
import { useEffect, useState } from "react";

import * as api from "../../api";
import { logInfo, logWarn } from "../../logging";
import { readBranchCompletedRuns } from "../../state/pullRequestSummaries";
import type { PullRequestSummary } from "../../types";
import { parseRejection } from "../Git/errors";
import { sameIssue, type PullRequestTask } from "./linking";

export type TasksRead =
  | { status: "loading" }
  | { status: "ready"; tasks: PullRequestTask[] }
  | { status: "failed" };

/**
 * CPR-FR-IDRP: the GitHub issue of each published draft of the completed runs
 * of `head`, each draft read once. `combine` is true at the first opening for
 * the head branch (CPR-FR-WCBQ).
 */
export function useBranchTasks(head: string, combine: boolean): TasksRead {
  const [read, setRead] = useState<TasksRead>({ status: "loading" });
  useEffect(() => {
    let live = true;
    void (async () => {
      let drafts: { draftId: string; draftName: string }[];
      try {
        const runs = await readBranchCompletedRuns(head, combine);
        const seen = new Set<string>();
        drafts = [];
        for (const run of runs) {
          if (!run.draftId || seen.has(run.draftId)) continue;
          seen.add(run.draftId);
          drafts.push({ draftId: run.draftId, draftName: run.draftName });
        }
      } catch (error) {
        logWarn(["frontend", "remote"], "pull request completed runs read failed", {
          head,
          error: parseRejection(error).code,
        });
        if (live) setRead({ status: "failed" });
        return;
      }
      const tasks = await Promise.all(
        drafts.map(async ({ draftId, draftName }): Promise<PullRequestTask | null> => {
          try {
            const view = await api.getDraftPublication(draftId);
            const record = view?.current;
            if (!record) return null;
            return {
              draftId,
              draftName,
              issue: {
                owner: record.repositoryOwner,
                repository: record.repositoryName,
                number: record.issueNumber,
              },
            };
          } catch (error) {
            logWarn(["frontend", "remote"], "pull request task publication read failed", {
              draftId,
              error: parseRejection(error).code,
            });
            return null;
          }
        }),
      );
      // CPR-FR-IDRP: each issue once, also when two drafts name it.
      const found: PullRequestTask[] = [];
      for (const task of tasks) {
        if (task && !found.some((t) => sameIssue(t.issue, task.issue))) found.push(task);
      }
      logInfo(["frontend", "remote"], "pull request tasks read", {
        head,
        draftCount: drafts.length,
        taskCount: found.length,
      });
      if (live) setRead({ status: "ready", tasks: found });
    })();
    return () => {
      live = false;
    };
    // The read is made once per opening.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [head]);
  return read;
}

export type OpenPullRequestsRead =
  | { status: "loading" }
  | { status: "ready"; pulls: PullRequestSummary[] }
  | { status: "failed" };

/** CPR-FR-IPKX: the open pull requests, read once when the window opens. */
export function useOpenPullRequests(): OpenPullRequestsRead {
  const [read, setRead] = useState<OpenPullRequestsRead>({ status: "loading" });
  useEffect(() => {
    let live = true;
    api
      .listPullRequests("open")
      .then((pulls) => {
        if (live) setRead({ status: "ready", pulls: Array.isArray(pulls) ? pulls : [] });
      })
      .catch((error: unknown) => {
        logWarn(["frontend", "remote"], "pull request open list read failed", {
          error: parseRejection(error).code,
        });
        if (live) setRead({ status: "failed" });
      });
    return () => {
      live = false;
    };
  }, []);
  return read;
}

/**
 * CPR-FR-IPKX: the open pull request whose head is `base`, or null. Null also
 * while the list loads or after it failed, so no kind is stated then.
 */
export function stackedOn(
  read: OpenPullRequestsRead,
  base: string,
): PullRequestSummary | null {
  if (read.status !== "ready" || base === "") return null;
  return read.pulls.find((pr) => pr.headBranch === base) ?? null;
}

/** CPR-FR-IOGF: whether the project resolves an agentic integration. */
export type IntegrationRead = "loading" | "resolves" | "none";

export function useAgenticIntegration(): IntegrationRead {
  const [read, setRead] = useState<IntegrationRead>("loading");
  useEffect(() => {
    let live = true;
    api
      .getProjectAgenticIntegration()
      .then((answer) => {
        if (live) setRead((answer?.vendor ?? null) !== null ? "resolves" : "none");
      })
      .catch((error: unknown) => {
        // A read that fails says nothing about the integration, so Generate
        // stays available and the backend answers the start.
        logWarn(["frontend", "ai"], "pull request agentic integration read failed", {
          error: parseRejection(error).code,
        });
        if (live) setRead("resolves");
      });
    return () => {
      live = false;
    };
  }, []);
  return read;
}
